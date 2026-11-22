use super::*;
use futures_util::FutureExt;
use protocol::organ::OrganTargetScope;
use serde_json::json;
use std::{future::Future, panic::AssertUnwindSafe};

struct Fixture {
    host: HostConfig,
    pool: PgPool,
    meta: CommandMeta,
    giga: Mutex<GigaRoomWorker>,
    live: orihon::live::Room,
}

impl Fixture {
    async fn call(
        &self,
        host: &HostConfig,
        operation: OrganOperation,
        target_scope: OrganTargetScope,
        params: Value,
    ) -> Result<Value, ProtocolErrorBody> {
        eprintln!("Host request {operation:?} target={target_scope:?} params={params}");
        let Value::Object(params) = params else {
            panic!("organ params must be an object");
        };
        let request = OrganRequest {
            operation,
            target_scope,
            params,
        };

        match super::call(
            host,
            Some(&self.pool),
            &self.meta,
            request,
            &self.giga,
            &self.live,
        )
        .await
        {
            ResponsePayload::Result { result } => Ok(result),
            ResponsePayload::Error { error } => {
                eprintln!(
                    "Host refusal {operation:?}: {}: {}",
                    error.code, error.message
                );
                Err(error)
            }
        }
    }

    async fn result(
        &self,
        host: &HostConfig,
        operation: OrganOperation,
        target_scope: OrganTargetScope,
        params: Value,
    ) -> Value {
        self.call(host, operation, target_scope, params)
            .await
            .expect("the real Host request must succeed; request and refusal are printed above")
    }

    async fn stored_axis(&self, entity_id: &str) -> (String, Option<String>, String) {
        sqlx::query_as("SELECT room, subject, scope FROM named_entities WHERE id = $1")
            .bind(entity_id.parse::<i64>().unwrap())
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn entity_count(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM named_entities")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

async fn observe<F, Fut>(test: F)
where
    F: FnOnce(Fixture) -> Fut,
    Fut: Future<Output = ()>,
{
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("Host canon proofs require a dedicated PostgreSQL URL");
    let lower = url.to_ascii_lowercase();

    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("isolated Host canon database must be reachable");
    let schema = format!("athanor_host_canon_{}", uuid::Uuid::new_v4().simple());
    let root = std::env::temp_dir().join(&schema);
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await
        .expect("isolated Host canon schema must create");
    let observation = AssertUnwindSafe(async {
        sqlx::query(&format!("SET search_path TO {schema}, public"))
            .execute(&pool)
            .await
            .unwrap();
        akasha::migrations::run_migrations(&pool)
            .await
            .expect("canonical migrations must apply to the Host canon schema");
        akasha::operator_registry::register_operator(
            &pool,
            &akasha::operator_registry::OperatorRegistration {
                unit: "canon_wire".into(),
                unit_name: "Canon wire unit".into(),
                key: "sol".into(),
                display_name: "Registered Subject".into(),
            },
        )
        .await
        .unwrap();
        let host = HostConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            bearer_token: "test-token".into(),
            room_dir: root.join("room"),
            state_dir: root.join("state"),
            house_dir: None,
            house_id: "test-house".into(),
            room: "test-room".into(),
            spirit: "Test".into(),
            session: "host-session".into(),
            database_url: Some(url),
            nats_url: None,
            nats_auth: None,
            knock_autonomy: crate::config::KnockAutonomy::Off,
            operator: Some("sol".into()),
        };
        crate::store::atomic_json_write(
            &host.room_state_path(),
            &json!({"room":"test-room","operator":"Display Operator","embodiedSpirit":"Test"}),
        )
        .unwrap();
        let meta = CommandMeta {
            schema_version: 1,
            message_id: "canon-wire".into(),
            house_id: "test-house".into(),
            sender_room: "test-room".into(),
            sender_spirit: "Test".into(),
            sender_session: "caller-session".into(),
            recipient: "house-host".into(),
            correlation_id: "canon-wire".into(),
            causation_id: String::new(),
            reply_target: "caller-session".into(),
            idempotency_key: "canon-wire".into(),
            source_record_refs: vec![],
            scope: "room:test-room:recall_policy".into(),
            visibility: "operator".into(),
            authority_class: "room_state".into(),
            created_at: "2026-11-17T00:00:00Z".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            max_hops: 1,
            projection_id: "context".into(),
        };
        test(Fixture {
            host,
            pool: pool.clone(),
            meta,
            giga: Mutex::new(GigaRoomWorker::default()),
            live: orihon::live::Room::default(),
        })
        .await;
    })
    .catch_unwind()
    .await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await
        .expect("only this Host canon schema must drop");
    pool.close().await;

    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove only this test's Host files");
    }

    if let Err(panic) = observation {
        std::panic::resume_unwind(panic);
    }
}

fn fact(name: &str) -> Value {
    json!({
        "name":name,"kind":"person","summary":format!("Standing fact about {name}."),
        "subject":"sol","attribution":{"actor":"Canon wire regression","origin":"test:host-canon"}
    })
}

fn entities(result: &Value) -> &[Value] {
    result["entities"]
        .as_array()
        .expect("CanonRead returns entities")
}

fn assert_invalid_params(error: ProtocolErrorBody, message: &str) {
    assert_eq!(error.code, "invalid_params");
    assert_eq!(error.message, message);
}

// Kills: losing CanonWrite/CanonRead in organ::bind_identity, or dropping subject,
// includeHistory, or the id/subject guard in protocol's canon conversion.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn host_stamp_writes_private_canon_and_subject_reads_preserve_history() {
    observe(|fixture| async move {
        let first = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonWrite,
                OrganTargetScope::Room,
                fact("WirePrivate"),
            )
            .await;
        let first_id = first["entityId"].as_str().unwrap();

        assert_eq!(first["durable"], true);
        assert_eq!(
            fixture.stored_axis(first_id).await,
            ("test-room".into(), Some("sol".into()), "private".into())
        );

        let mut correction = fact("WirePrivate");
        correction["summary"] = json!("Corrected private fact.");
        correction["supersedes"] = json!([first_id]);
        let current = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonWrite,
                OrganTargetScope::Room,
                correction,
            )
            .await;
        let current_id = current["entityId"].as_str().unwrap();
        let history = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonRead,
                OrganTargetScope::Room,
                json!({"subject":"sol","includeHistory":true}),
            )
            .await;

        assert_eq!(entities(&history).len(), 2);
        assert!(
            entities(&history)
                .iter()
                .any(|row| row["entityId"] == first_id && row["authority"] == "superseded")
        );
        assert!(
            entities(&history)
                .iter()
                .any(|row| row["entityId"] == current_id && row["authority"] == "active")
        );
        assert!(
            entities(&history)
                .iter()
                .all(|row| row["subject"] == "sol" && row["scope"] == "private")
        );

        let active = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonRead,
                OrganTargetScope::Room,
                json!({"subject":"sol","includeHistory":false}),
            )
            .await;

        assert_eq!(entities(&active).len(), 1);
        assert_eq!(entities(&active)[0]["entityId"], current_id);

        let error = fixture
            .call(
                &fixture.host,
                OrganOperation::CanonRead,
                OrganTargetScope::Room,
                json!({"id":current_id,"subject":"sol","includeHistory":true}),
            )
            .await
            .expect_err("id and subject must not select a row together");
        assert_invalid_params(
            error,
            "invalid parameters: id does not combine with subject",
        );
    })
    .await;
}

// Kills: losing scope/minElevation while decoding CanonWriteParams or passing
// them to with_axis; a successful write or an unknown-field error is not the domain refusal.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn unsupported_canon_axes_reach_exact_domain_refusals_without_writes() {
    observe(|fixture| async move {
        for (name, field, value, expected) in [
            (
                "UnsupportedUp",
                "scope",
                json!("up"),
                "invalid parameters: invalid canon scope: the scope up is not available for canon yet",
            ),
            (
                "UnsupportedElevation",
                "minElevation",
                json!(3),
                "invalid parameters: invalid canon minElevation: minElevation needs the scope up",
            ),
        ] {
            let mut params = fact(name);
            params[field] = value;
            let error = fixture
                .call(&fixture.host, OrganOperation::CanonWrite, OrganTargetScope::Room, params)
                .await
                .expect_err("the unsupported axis must reach the domain refusal");

            assert_invalid_params(error, expected);
            assert_eq!(fixture.entity_count().await, 0, "a refused request must not insert a row");
        }
    })
    .await;
}

// Kills: stamping a wrong key in organ::bind_identity or dropping/renaming
// EntityResolveParams.operator in akasha/src/entity.rs's deny_unknown_fields consumer.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn host_stamped_entity_resolution_accepts_a_company_person_fact() {
    observe(|fixture| async move {
        let mut params = fact("Linden");
        params["scope"] = json!("company");
        params["aliases"] = json!(["WireAlias"]);
        let written = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonWrite,
                OrganTargetScope::Room,
                params,
            )
            .await;

        assert_eq!(
            fixture
                .stored_axis(written["entityId"].as_str().unwrap())
                .await,
            ("test-room".into(), Some("sol".into()), "company".into())
        );

        let resolved = fixture
            .result(
                &fixture.host,
                OrganOperation::EntityResolve,
                OrganTargetScope::Room,
                json!({"query":"WireAlias","limit":8}),
            )
            .await;

        assert_eq!(resolved["ok"], true);
        assert_eq!(
            resolved["matches"],
            json!([{
                "canonicalName":"Linden","kind":"person","matchedAlias":"WireAlias"
            }])
        );
    })
    .await;
}

// Kills: removing operator from organ::bound_request's caller-key refusal,
// including accepting a caller-sent value that happens to equal the configured key.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn caller_operator_is_refused_for_all_three_canon_operations() {
    observe(|fixture| async move {
        let mut company = fact("PublicSignal");
        company["scope"] = json!("company");
        let written = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonWrite,
                OrganTargetScope::Room,
                company,
            )
            .await;
        let read = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonRead,
                OrganTargetScope::Room,
                json!({"subject":"sol"}),
            )
            .await;
        let resolved = fixture
            .result(
                &fixture.host,
                OrganOperation::EntityResolve,
                OrganTargetScope::Room,
                json!({"query":"PublicSignal"}),
            )
            .await;

        assert_eq!(entities(&read)[0]["entityId"], written["entityId"]);
        assert_eq!(resolved["matches"][0]["canonicalName"], "PublicSignal");

        let before = fixture.entity_count().await;

        for (operation, mut params) in [
            (OrganOperation::CanonWrite, fact("CallerInjected")),
            (OrganOperation::CanonRead, json!({"subject":"sol"})),
            (
                OrganOperation::EntityResolve,
                json!({"query":"PublicSignal"}),
            ),
        ] {
            params["operator"] = json!("sol");
            let error = fixture
                .call(&fixture.host, operation, OrganTargetScope::Room, params)
                .await
                .expect_err("even the configured operator key belongs to the Host alone");

            assert_invalid_params(
                error,
                "invalid parameters: operator belongs to the Host binding, not organ params",
            );
            assert_eq!(fixture.entity_count().await, before);
        }
    })
    .await;
}

// Kills: inventing a reader/writer when organ::bind_identity sees operator=None,
// or losing protocol's with_reader binding so the private/public contrast collapses.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn a_host_without_an_operator_cannot_read_or_write_the_private_subject() {
    observe(|fixture| async move {
        let private = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonWrite,
                OrganTargetScope::Room,
                fact("HiddenSignal"),
            )
            .await;
        let mut company = fact("VisibleSignal");
        company["scope"] = json!("company");
        let public = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonWrite,
                OrganTargetScope::Room,
                company,
            )
            .await;
        let private_id = private["entityId"].as_str().unwrap();
        let public_id = public["entityId"].as_str().unwrap();
        let owner = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonRead,
                OrganTargetScope::Room,
                json!({"subject":"sol"}),
            )
            .await;

        assert_eq!(entities(&owner).len(), 2);
        assert!(
            entities(&owner)
                .iter()
                .any(|row| row["entityId"] == private_id)
        );

        let mut anonymous = fixture.host.clone();
        anonymous.operator = None;

        for (hidden_selector, public_selector) in [
            (json!({"id":private_id}), json!({"id":public_id})),
            (
                json!({"name":"HiddenSignal"}),
                json!({"name":"VisibleSignal"}),
            ),
        ] {
            let hidden = fixture
                .result(
                    &anonymous,
                    OrganOperation::CanonRead,
                    OrganTargetScope::Room,
                    hidden_selector,
                )
                .await;
            let control = fixture
                .result(
                    &anonymous,
                    OrganOperation::CanonRead,
                    OrganTargetScope::Room,
                    public_selector,
                )
                .await;

            assert!(
                entities(&hidden).is_empty(),
                "the private result must equal an absent result"
            );
            assert_eq!(entities(&control).len(), 1);
            assert_eq!(entities(&control)[0]["entityId"], public_id);
        }

        let by_subject = fixture
            .result(
                &anonymous,
                OrganOperation::CanonRead,
                OrganTargetScope::Room,
                json!({"subject":"sol","includeHistory":true}),
            )
            .await;

        assert_eq!(entities(&by_subject).len(), 1);
        assert_eq!(entities(&by_subject)[0]["entityId"], public_id);

        let before = fixture.entity_count().await;
        let error = fixture
            .call(
                &anonymous,
                OrganOperation::CanonWrite,
                OrganTargetScope::Room,
                fact("AnonymousWrite"),
            )
            .await
            .expect_err("a subject write requires the Host operator");

        assert_invalid_params(
            error,
            "invalid parameters: invalid canon subject: a row with a subject needs a Host operator",
        );
        assert_eq!(fixture.entity_count().await, before);
    })
    .await;
}

// Kills: omitting CanonWrite from the Host House-target allow-list or losing the
// room="house" override in bound_request; a room subject read must still include House canon.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn house_target_writes_house_canon_visible_to_a_room_subject_read() {
    observe(|fixture| async move {
        let local = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonWrite,
                OrganTargetScope::Room,
                fact("LocalSignal"),
            )
            .await;
        let house = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonWrite,
                OrganTargetScope::House,
                fact("HouseSignal"),
            )
            .await;

        assert_eq!(house["room"], "house");
        assert_eq!(
            fixture
                .stored_axis(house["entityId"].as_str().unwrap())
                .await,
            ("house".into(), Some("sol".into()), "private".into())
        );

        let read = fixture
            .result(
                &fixture.host,
                OrganOperation::CanonRead,
                OrganTargetScope::Room,
                json!({"subject":"sol"}),
            )
            .await;

        assert_eq!(entities(&read).len(), 2);

        for (receipt, room) in [(&local, "test-room"), (&house, "house")] {
            assert!(entities(&read).iter().any(|row| {
                row["entityId"] == receipt["entityId"]
                    && row["room"] == room
                    && row["subject"] == "sol"
            }));
        }
    })
    .await;
}
