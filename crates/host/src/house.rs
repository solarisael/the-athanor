use crate::config::{HostConfig, HouseConfig};
use crate::server::{Host, authorized};
use akasha::POOL_ACQUIRE_TIMEOUT;
use akasha::insula_writer::{flush_insula_emitter, init_insula_emitter};
use akasha::spirits::{SPIRITS_CHANNEL, SpiritRecord, active_spirits, import_spirits};
use async_nats::jetstream::ErrorCode;
use async_nats::jetstream::stream::ConsumerErrorKind;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Json, Router};
use origami::cranes::broker::{HALLWAY_STREAM_NAME, connect_options, hallway_consumer_name};
use origami::cranes::{delivery::DeliveryService, outbox::Store};
use protocol::HOST_ROOM_PATH_PREFIX;
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::{PgListener, PgPoolOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::sync::mpsc::{Sender, channel};
use std::thread::{Builder as ThreadBuilder, JoinHandle};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::runtime::Builder as RuntimeBuilder;
use tokio::sync::{Notify, RwLock, oneshot};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower::ServiceExt;

const HOUSE_POOL_CONNECTIONS: u32 = 8;
/// The `summoned_by` of the rooms that the Host imports from its seed, the
/// `rooms` list of runtime.json.
const SEED_SOURCE: &str = "import:runtime.json";
const LISTEN_BACKOFF: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
];
/// How long a closed room waits for the broker to remove its hallway consumer.
const FORGET_TIMEOUT: Duration = Duration::from_secs(5);

/// Builds the configuration of one room from its spirit record.
pub type RoomFactory = Arc<dyn Fn(&SpiritRecord) -> HostConfig + Send + Sync>;

/// Where the Host gets its rooms.
pub enum RoomSource {
    /// These rooms, until the Host stops.
    Configured(Vec<HostConfig>),
    /// The active rows of table `spirits`, followed while the Host runs. When
    /// the table has no row at start, the Host first imports `seed` into it.
    Spirits {
        seed: Vec<HostConfig>,
        factory: RoomFactory,
    },
}

pub struct HostRuntime {
    address: SocketAddr,
    signal: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

impl HostRuntime {
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_some_and(JoinHandle::is_finished)
    }

    pub fn wait(mut self) -> Result<(), String> {
        self.join_thread()
    }

    pub fn shutdown(mut self) -> Result<(), String> {
        self.signal_and_join()
    }

    fn join_thread(&mut self) -> Result<(), String> {
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| "the Athanor Host thread panicked".to_owned())?
    }

    fn signal_and_join(&mut self) -> Result<(), String> {
        if let Some(signal) = self.signal.take() {
            let _ = signal.send(());
        }
        self.join_thread()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot::error::TryRecvError;

    #[test]
    fn wait_reports_a_host_failure_without_requesting_shutdown() {
        let (signal, mut shutdown) = oneshot::channel();
        let thread = std::thread::spawn(move || match shutdown.try_recv() {
            Err(TryRecvError::Empty) => Err("internal Host failure".to_owned()),
            Ok(()) | Err(TryRecvError::Closed) => Err("wait requested Host shutdown".to_owned()),
        });
        let runtime = HostRuntime {
            address: "127.0.0.1:1".parse().unwrap(),
            signal: Some(signal),
            thread: Some(thread),
        };

        assert_eq!(runtime.wait().unwrap_err(), "internal Host failure");
    }

    fn open_room(tasks: TaskTracker) -> OpenRoom {
        OpenRoom {
            spirit: "Spirit".into(),
            operator: Some("sol".into()),
            room_dir: PathBuf::from("room"),
            state_dir: PathBuf::from("room/host"),
            router: Router::new(),
            cancellation: CancellationToken::new(),
            tasks,
        }
    }

    async fn until_stopped(rooms: &Rooms, room: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while rooms.is_closing(room) {
                rooms.stopped.notified().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("room {room} never stopped"));
    }

    // Kills: a close that waits for the tasks of its room, so that one room
    // that stops slowly holds the follower of the spirit record and every
    // other room; and a room key that is free again while its Host still runs.
    #[tokio::test]
    async fn a_room_that_stops_slowly_holds_no_other_room() {
        let house = HouseConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            bearer_token: "rooms-test".into(),
            house_id: "solarisael".into(),
            database_url: None,
            nats_url: None,
            nats_auth: None,
        };
        let rooms = Rooms::new(house, None, CancellationToken::new(), TaskTracker::new());
        // A room task that ignores its room token.
        let release = CancellationToken::new();
        let slow = TaskTracker::new();
        slow.spawn(release.clone().cancelled_owned());
        {
            let mut table = rooms.table.write().await;
            table.insert("hugo".into(), open_room(slow));
            table.insert("salvia".into(), open_room(TaskTracker::new()));
        }

        tokio::time::timeout(Duration::from_secs(5), async {
            rooms.close("hugo", false).await;
            rooms.close("salvia", false).await;
        })
        .await
        .expect("a close waits for no task of its room");
        assert!(rooms.router("hugo").await.is_none());
        assert!(rooms.router("salvia").await.is_none());
        until_stopped(&rooms, "salvia").await;
        assert!(rooms.is_closing("hugo"));

        release.cancel();
        until_stopped(&rooms, "hugo").await;
    }
}

impl Drop for HostRuntime {
    fn drop(&mut self) {
        let _ = self.signal_and_join();
    }
}

// [host/lifetime/owner] [runtime/topology]
pub fn start(house: HouseConfig, rooms: RoomSource) -> Result<HostRuntime, String> {
    let (ready, bound) = channel();
    let (signal, shutdown) = oneshot::channel();
    let thread = ThreadBuilder::new()
        .name("athanor-host".to_owned())
        .spawn(move || own_house(house, rooms, ready, shutdown))
        .map_err(|error| format!("cannot start the Athanor Host thread: {error}"))?;
    match bound.recv() {
        Ok(Ok(address)) => Ok(HostRuntime {
            address,
            signal: Some(signal),
            thread: Some(thread),
        }),
        Ok(Err(reason)) => Err(joined(thread, reason)),
        Err(_) => Err(joined(
            thread,
            "the Athanor Host stopped before it bound a listener".to_owned(),
        )),
    }
}

fn joined(thread: JoinHandle<Result<(), String>>, reason: String) -> String {
    let _ = thread.join();
    reason
}

fn own_house(
    house: HouseConfig,
    rooms: RoomSource,
    ready: Sender<Result<SocketAddr, String>>,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), String> {
    match RuntimeBuilder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(serve_house(house, rooms, ready, shutdown)),
        Err(error) => Err(refused(
            &ready,
            format!("cannot build the Athanor Host runtime: {error}"),
        )),
    }
}

fn refused(ready: &Sender<Result<SocketAddr, String>>, reason: String) -> String {
    let _ = ready.send(Err(reason.clone()));
    reason
}

// [host/lifetime/shutdown] [host/routing]
async fn serve_house(
    house: HouseConfig,
    rooms: RoomSource,
    ready: Sender<Result<SocketAddr, String>>,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), String> {
    let cancellation = CancellationToken::new();
    let tasks = TaskTracker::new();
    let opened = match open_house(house, rooms, &cancellation, &tasks).await {
        Ok(opened) => opened,
        Err(reason) => return Err(refused(&ready, reason)),
    };
    let _ = ready.send(Ok(opened.address));
    let signalled = cancellation.clone();
    let served = axum::serve(opened.listener, opened.router)
        .with_graceful_shutdown(async move {
            let _ = shutdown.await;
            signalled.cancel();
        })
        .await
        .map_err(|error| format!("the Athanor Host server failed: {error}"));
    cancellation.cancel();
    // The follower of the spirit record stops first, so that no room opens
    // while the rooms close; a room that already left the table finishes its
    // close on the same tracker.
    tasks.close();
    tasks.wait().await;
    opened.rooms.close_all().await;
    flush_insula_emitter().await;
    served
}

struct OpenHouse {
    address: SocketAddr,
    listener: TcpListener,
    router: Router,
    rooms: Rooms,
}

// [host/routing] [host/pool/shared] [protocol/room/key]
async fn open_house(
    house: HouseConfig,
    source: RoomSource,
    cancellation: &CancellationToken,
    tasks: &TaskTracker,
) -> Result<OpenHouse, String> {
    house.validate()?;
    let pool = house_pool(&house)?;
    if let Some(pool) = pool.clone() {
        init_insula_emitter(pool.clone());
        let stop = cancellation.clone();
        let retention_pool = pool.clone();
        let house_id = house.house_id.clone();
        tasks.spawn(async move {
            tokio::select! {
                _ = stop.cancelled() => {}
                _ = akasha::native::retention::run(Some(retention_pool), &house_id) => {}
            }
        });
        if let Some(nats_url) = house.nats_url.clone() {
            tasks.spawn(DeliveryService::serve(
                Store::from_pool(pool),
                nats_url,
                house.nats_auth.clone(),
                cancellation.clone(),
            ));
        }
    }
    let rooms = Rooms::new(
        house.clone(),
        pool.clone(),
        cancellation.clone(),
        tasks.clone(),
    );
    match source {
        RoomSource::Configured(configs) => {
            for config in configs {
                rooms.open(config).await?;
            }
        }
        RoomSource::Spirits { seed, factory } => {
            let (Some(pool), Some(database_url)) = (pool, house.database_url.clone()) else {
                return Err("the Athanor Host needs DATABASE_URL to read the spirit record".into());
            };
            import_seed(&pool, &seed).await?;
            rooms.reconcile(&pool, &factory).await?;
            tasks.spawn(follow_spirits(
                rooms.clone(),
                pool,
                database_url,
                factory,
                cancellation.clone(),
            ));
        }
    }
    let router = Router::new()
        .route(&format!("{HOST_ROOM_PATH_PREFIX}{{room}}"), any(forward))
        .route(
            &format!("{HOST_ROOM_PATH_PREFIX}{{room}}/{{*rest}}"),
            any(forward),
        )
        .with_state(rooms.clone());
    let listener = TcpListener::bind(house.bind)
        .await
        .map_err(|error| format!("cannot bind the Athanor Host to {}: {error}", house.bind))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("cannot read the Athanor Host address: {error}"))?;
    tracing::info!(%address, rooms = rooms.len().await, "Athanor Host listening");
    Ok(OpenHouse {
        address,
        listener,
        router,
        rooms,
    })
}

// [host/pool/shared]
fn house_pool(house: &HouseConfig) -> Result<Option<PgPool>, String> {
    house
        .database_url
        .as_deref()
        .map(|url| {
            PgPoolOptions::new()
                .max_connections(HOUSE_POOL_CONNECTIONS)
                // already sqlx's default; stated because the bounce law depends on it
                .test_before_acquire(true)
                .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
                .connect_lazy(url)
        })
        .transpose()
        .map_err(|error| format!("the Athanor Host DATABASE_URL is invalid: {error}"))
}

async fn import_seed(pool: &PgPool, seed: &[HostConfig]) -> Result<(), String> {
    let records = seed
        .iter()
        .map(|config| {
            let operator = config.operator.clone().ok_or_else(|| {
                format!(
                    "configured room {} has no operator; the spirit record needs one",
                    config.room
                )
            })?;
            Ok(SpiritRecord {
                room: config.room.clone(),
                spirit: config.spirit.clone(),
                operator,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let imported = import_spirits(pool, &records, SEED_SOURCE)
        .await
        .map_err(|error| format!("cannot import the configured rooms: {error}"))?;

    if imported > 0 {
        tracing::info!(
            rooms = imported,
            "imported the configured rooms into the spirit record"
        );
    }
    Ok(())
}

/// The rooms that the Host serves now, by room key. Each room has its own
/// child of the House cancellation and its own task tracker, so one room can
/// stop while the others run.
#[derive(Clone)]
struct Rooms {
    table: Arc<RwLock<BTreeMap<String, OpenRoom>>>,
    /// The rooms that left `table` and still stop. A room key opens again
    /// only after its previous Host stopped.
    closing: Arc<Mutex<BTreeSet<String>>>,
    /// Wakes the follower of the spirit record when a closing room stopped,
    /// so that a room key that waited for it opens.
    stopped: Arc<Notify>,
    house: Arc<HouseConfig>,
    pool: Option<PgPool>,
    cancellation: CancellationToken,
    /// The House tracker, which runs the close of each room.
    tasks: TaskTracker,
}

struct OpenRoom {
    spirit: String,
    operator: Option<String>,
    /// The folders this room serves. A reconcile whose factory resolves
    /// other folders rebuilds the room; `athanor move` relies on it.
    room_dir: PathBuf,
    state_dir: PathBuf,
    router: Router,
    cancellation: CancellationToken,
    tasks: TaskTracker,
}

impl Rooms {
    fn new(
        house: HouseConfig,
        pool: Option<PgPool>,
        cancellation: CancellationToken,
        tasks: TaskTracker,
    ) -> Self {
        Self {
            table: Arc::default(),
            closing: Arc::default(),
            stopped: Arc::default(),
            house: Arc::new(house),
            pool,
            cancellation,
            tasks,
        }
    }

    async fn len(&self) -> usize {
        self.table.read().await.len()
    }

    async fn router(&self, room: &str) -> Option<Router> {
        self.table
            .read()
            .await
            .get(room)
            .map(|open| open.router.clone())
    }

    // [host/config/validation] [host/state/room]
    async fn open(&self, config: HostConfig) -> Result<(), String> {
        config.validate()?;
        if let Some(setting) = self.house.disagreement(&config) {
            return Err(format!(
                "the Athanor Host rooms must share one {setting}; room {} disagrees",
                config.room
            ));
        }
        if self.table.read().await.contains_key(&config.room) {
            return Err(format!(
                "the Athanor Host room {} is configured twice",
                config.room
            ));
        }

        let room = config.room.clone();
        let spirit = config.spirit.clone();
        let operator = config.operator.clone();
        let (room_dir, state_dir) = (config.room_dir.clone(), config.state_dir.clone());
        let cancellation = self.cancellation.child_token();
        let tasks = TaskTracker::new();
        let host = Host::new(config, self.pool.clone(), cancellation.clone(), tasks.clone())?;

        host.spawn_receipt_bridge();
        self.table.write().await.insert(
            room,
            OpenRoom {
                spirit,
                operator,
                room_dir,
                state_dir,
                router: host.router(),
                cancellation,
                tasks,
            },
        );
        Ok(())
    }

    /// Takes the room out of the table at once and stops it in a task of its
    /// own, so that a room that stops slowly holds neither the follower of the
    /// spirit record nor any other room. `forget` also removes the hallway
    /// consumer, for a room that left the spirit record.
    async fn close(&self, room: &str, forget: bool) {
        let Some(open) = self.table.write().await.remove(room) else {
            return;
        };

        open.cancellation.cancel();
        open.tasks.close();
        self.closing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(room.to_owned());
        let (rooms, room) = (self.clone(), room.to_owned());
        self.tasks.spawn(async move {
            open.tasks.wait().await;
            if forget {
                forget_hallway_consumer(&rooms.house, &room).await;
            }
            rooms
                .closing
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&room);
            rooms.stopped.notify_one();
            tracing::info!(%room, "closed an Athanor Host room");
        });
    }

    fn is_closing(&self, room: &str) -> bool {
        self.closing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(room)
    }

    async fn close_all(&self) {
        let rooms = std::mem::take(&mut *self.table.write().await);

        for open in rooms.into_values() {
            open.cancellation.cancel();
            open.tasks.close();
            open.tasks.wait().await;
        }
    }

    /// Makes the open rooms equal to the active records: closes each room that
    /// left the record, rebuilds each room whose spirit, operator, room folder,
    /// or state folder differs from what the factory builds now, and opens each
    /// new room. A room that cannot open stays closed until the next reconcile;
    /// the other rooms still open. A room key whose previous Host still stops
    /// opens at the reconcile after that Host stopped.
    async fn reconcile(&self, pool: &PgPool, factory: &RoomFactory) -> Result<(), String> {
        let records = active_spirits(pool)
            .await
            .map_err(|error| format!("cannot read the spirit record: {error}"))?;
        let wanted: BTreeMap<&str, &SpiritRecord> = records
            .iter()
            .map(|record| (record.room.as_str(), record))
            .collect();
        let stale: Vec<(String, bool)> = self
            .table
            .read()
            .await
            .iter()
            .filter_map(|(room, open)| match wanted.get(room.as_str()) {
                None => Some((room.clone(), true)),
                Some(record) => open.differs(&factory(record)).then(|| (room.clone(), false)),
            })
            .collect();

        for (room, forget) in stale {
            self.close(&room, forget).await;
        }
        for record in &records {
            if self.is_closing(&record.room)
                || self.table.read().await.contains_key(&record.room)
            {
                continue;
            }
            match self.open(factory(record)).await {
                Ok(()) => tracing::info!(room = %record.room, "opened an Athanor Host room"),
                Err(reason) => tracing::error!(
                    room = %record.room,
                    %reason,
                    "cannot open the room of a spirit record"
                ),
            }
        }
        Ok(())
    }

    // [host/routing] [security/auth]
    async fn spirits(&self, headers: &HeaderMap) -> Response {
        if !authorized(headers, &self.house.bearer_token) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "unauthenticated",
                    "diagnostic": "Authorization: Bearer <token> is required"
                })),
            )
                .into_response();
        }

        let spirits: Vec<_> = self
            .table
            .read()
            .await
            .iter()
            .map(|(room, open)| {
                json!({"room": room, "spirit": open.spirit, "operator": open.operator})
            })
            .collect();
        Json(json!({"ok": true, "spirits": spirits})).into_response()
    }
}

impl OpenRoom {
    fn differs(&self, config: &HostConfig) -> bool {
        self.spirit != config.spirit
            || self.operator != config.operator
            || self.room_dir != config.room_dir
            || self.state_dir != config.state_dir
    }
}

/// One door for every room: `/room/<room>` and each path below it go to the
/// router of that room without the room prefix, as `Router::nest` did when
/// the rooms were fixed at start.
// [host/routing] [protocol/room/key]
async fn forward(State(rooms): State<Rooms>, mut request: Request) -> Response {
    let path = request.uri().path();
    let room = path
        .strip_prefix(HOST_ROOM_PATH_PREFIX)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default();
    let Some(router) = rooms.router(room).await else {
        return unknown_room(room);
    };
    let rest = &path[HOST_ROOM_PATH_PREFIX.len() + room.len()..];

    if rest == "/spirits" && request.method() == Method::GET {
        return rooms.spirits(request.headers()).await;
    }

    let uri = match room_uri(request.uri(), rest) {
        Ok(uri) => uri,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_path", "diagnostic": error.to_string()})),
            )
                .into_response();
        }
    };
    *request.uri_mut() = uri;
    match router.oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

fn room_uri(uri: &Uri, rest: &str) -> Result<Uri, axum::http::Error> {
    let path = if rest.is_empty() { "/" } else { rest };
    let path_and_query = match uri.query() {
        Some(query) => format!("{path}?{query}"),
        None => path.to_owned(),
    };
    let mut parts = uri.clone().into_parts();

    parts.path_and_query = Some(path_and_query.parse()?);
    Ok(Uri::from_parts(parts)?)
}

fn unknown_room(room: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "unknown_room",
            "room": room,
            "diagnostic": format!("this Athanor Host serves no active room {room:?}")
        })),
    )
        .into_response()
}

/// Follows NOTIFY on the spirit record until the Host stops. Each notification
/// and each new LISTEN connection starts a full reconcile, because a lost
/// connection also loses the notifications sent while it was down.
// [host/routing] [host/lifetime/shutdown]
async fn follow_spirits(
    rooms: Rooms,
    pool: PgPool,
    database_url: String,
    factory: RoomFactory,
    cancellation: CancellationToken,
) {
    let mut failures = 0usize;
    loop {
        let connected = tokio::select! {
            _ = cancellation.cancelled() => return,
            connected = listen_for_spirits(&database_url) => connected,
        };
        let reason = match connected {
            Ok(listener) => {
                failures = 0;
                tokio::select! {
                    _ = cancellation.cancelled() => return,
                    reason = follow(listener, &rooms, &pool, &factory) => reason,
                }
            }
            Err(error) => format!("cannot LISTEN on {SPIRITS_CHANNEL}: {error}"),
        };
        let delay = LISTEN_BACKOFF[failures.min(LISTEN_BACKOFF.len() - 1)];

        failures += 1;
        tracing::warn!(%reason, retry_in = ?delay, "the spirit record follower stopped");
        tokio::select! {
            _ = cancellation.cancelled() => return,
            _ = tokio::time::sleep(delay) => {}
        }
    }
}

async fn listen_for_spirits(database_url: &str) -> Result<PgListener, sqlx::Error> {
    let mut listener = PgListener::connect(database_url).await?;

    listener.listen(SPIRITS_CHANNEL).await?;
    Ok(listener)
}

/// Returns only when the listener or a reconcile fails, with the reason.
async fn follow(
    mut listener: PgListener,
    rooms: &Rooms,
    pool: &PgPool,
    factory: &RoomFactory,
) -> String {
    loop {
        if let Err(reason) = rooms.reconcile(pool, factory).await {
            return reason;
        }
        // `Ok(None)`: the connection dropped and came back. The reconcile at
        // the top of the loop catches up with the lost notifications. A room
        // that stopped also reconciles again, so that a room key that waited
        // for its previous Host opens.
        tokio::select! {
            received = listener.try_recv() => match received {
                Ok(_) => while listener.next_buffered().is_some() {},
                Err(error) => return format!("the LISTEN on {SPIRITS_CHANNEL} failed: {error}"),
            },
            () = rooms.stopped.notified() => {}
        }
    }
}

/// A room that left the spirit record must not keep its durable consumer on
/// the hallway stream. A missing consumer or stream is not an error. A broker
/// that does not answer within `FORGET_TIMEOUT` leaves the consumer in place.
async fn forget_hallway_consumer(house: &HouseConfig, room: &str) {
    let Some(url) = house.nats_url.as_deref() else {
        return;
    };

    let name = hallway_consumer_name(room);
    let removed = async {
        let client = connect_options(house.nats_auth.as_ref())
            .connection_timeout(Duration::from_secs(1))
            .connect(url)
            .await
            .map_err(|error| error.to_string())?;
        let context = async_nats::jetstream::new(client);

        match context
            .delete_consumer_from_stream(&name, HALLWAY_STREAM_NAME)
            .await
        {
            Ok(_) => Ok(()),
            Err(error) => match error.kind() {
                ConsumerErrorKind::JetStream(cause)
                    if cause.error_code() == ErrorCode::CONSUMER_NOT_FOUND
                        || cause.error_code() == ErrorCode::STREAM_NOT_FOUND =>
                {
                    Ok(())
                }
                _ => Err(error.to_string()),
            },
        }
    };
    let removed = tokio::time::timeout(FORGET_TIMEOUT, removed)
        .await
        .unwrap_or_else(|_| {
            Err(format!(
                "the broker did not answer within {} s",
                FORGET_TIMEOUT.as_secs()
            ))
        });

    if let Err(reason) = removed {
        tracing::warn!(
            %room,
            consumer = %name,
            %reason,
            "cannot remove the hallway consumer of a closed room"
        );
    }
}
