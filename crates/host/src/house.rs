use crate::config::HostConfig;
use crate::server::Host;
use akasha::insula_writer::{flush_insula_emitter, init_insula_emitter};
use axum::Router;
use origami::cranes::{delivery::DeliveryService, outbox::Store};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::mpsc::{Sender, channel};
use std::thread::{Builder as ThreadBuilder, JoinHandle};
use tokio::net::TcpListener;
use tokio::runtime::Builder as RuntimeBuilder;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

const HOUSE_POOL_CONNECTIONS: u32 = 8;

pub struct HostRuntime {
    address: SocketAddr,
    signal: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

impl HostRuntime {
    pub fn address(&self) -> SocketAddr {
        self.address
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
}

impl Drop for HostRuntime {
    fn drop(&mut self) {
        let _ = self.signal_and_join();
    }
}

// [host/lifetime/owner] [runtime/topology]
pub fn start(rooms: Vec<HostConfig>) -> Result<HostRuntime, String> {
    let (ready, bound) = channel();
    let (signal, shutdown) = oneshot::channel();
    let thread = ThreadBuilder::new()
        .name("athanor-host".to_owned())
        .spawn(move || own_house(rooms, ready, shutdown))
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
    rooms: Vec<HostConfig>,
    ready: Sender<Result<SocketAddr, String>>,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), String> {
    match RuntimeBuilder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(serve_house(rooms, ready, shutdown)),
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
    rooms: Vec<HostConfig>,
    ready: Sender<Result<SocketAddr, String>>,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), String> {
    let cancellation = CancellationToken::new();
    let tasks = TaskTracker::new();
    let house = match open_house(rooms, &cancellation, &tasks).await {
        Ok(house) => house,
        Err(reason) => return Err(refused(&ready, reason)),
    };
    let _ = ready.send(Ok(house.address));
    let signalled = cancellation.clone();
    let served = axum::serve(house.listener, house.router)
        .with_graceful_shutdown(async move {
            let _ = shutdown.await;
            signalled.cancel();
        })
        .await
        .map_err(|error| format!("the Athanor Host server failed: {error}"));
    cancellation.cancel();
    tasks.close();
    tasks.wait().await;
    flush_insula_emitter().await;
    served
}

struct OpenHouse {
    address: SocketAddr,
    listener: TcpListener,
    router: Router,
}

// [host/routing] [host/pool/shared] [protocol/room/key]
async fn open_house(
    rooms: Vec<HostConfig>,
    cancellation: &CancellationToken,
    tasks: &TaskTracker,
) -> Result<OpenHouse, String> {
    let (bind, pool, nats_url) = house_settings(&rooms)?;
    if let Some(pool) = pool.clone() {
        init_insula_emitter(pool.clone());
        if let Some(nats_url) = nats_url {
            tasks.spawn(DeliveryService::serve(
                Store::from_pool(pool),
                nats_url,
                cancellation.clone(),
            ));
        }
    }
    let count = rooms.len();
    let mut router = Router::new();
    for config in rooms {
        let prefix = config.room_path();
        let host = Host::new(config, pool.clone(), cancellation.clone(), tasks.clone())?;
        host.spawn_receipt_bridge();
        router = router.nest(&prefix, host.router());
    }
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|error| format!("cannot bind the Athanor Host to {bind}: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("cannot read the Athanor Host address: {error}"))?;
    tracing::info!(%address, rooms = count, "Athanor Host listening");
    Ok(OpenHouse {
        address,
        listener,
        router,
    })
}

// [host/config/validation] [host/pool/shared]
fn house_settings(
    rooms: &[HostConfig],
) -> Result<(SocketAddr, Option<PgPool>, Option<String>), String> {
    let Some(first) = rooms.first() else {
        return Err("the Athanor Host needs at least one room".into());
    };
    let mut keys = HashSet::with_capacity(rooms.len());
    for room in rooms {
        room.validate()?;
        if !keys.insert(room.room.as_str()) {
            return Err(format!(
                "the Athanor Host room {} is configured twice",
                room.room
            ));
        }
        for (setting, agrees) in [
            ("bind address", first.bind == room.bind),
            ("bearer token", first.bearer_token == room.bearer_token),
            ("house identifier", first.house_id == room.house_id),
            ("DATABASE_URL", first.database_url == room.database_url),
            ("ATHANOR_NATS_URL", first.nats_url == room.nats_url),
        ] {
            if !agrees {
                return Err(format!(
                    "the Athanor Host rooms must share one {setting}; room {} disagrees",
                    room.room
                ));
            }
        }
    }
    let pool = first
        .database_url
        .as_deref()
        .map(|url| {
            PgPoolOptions::new()
                .max_connections(HOUSE_POOL_CONNECTIONS)
                .connect_lazy(url)
        })
        .transpose()
        .map_err(|error| format!("the Athanor Host DATABASE_URL is invalid: {error}"))?;
    Ok((first.bind, pool, first.nats_url.clone()))
}
