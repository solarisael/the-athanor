use crate::cranes::broker::{Broker, HALLWAY_SUBJECT_PREFIX};
use anyhow::{Context, Result};
use hearth::hallway::HallwayPostReceipt;
use serde::{Deserialize, Serialize};

pub fn hallway_room_subject(room: &str) -> String {
    format!("{HALLWAY_SUBJECT_PREFIX}{room}")
}

pub fn hallway_message_id(idempotency_key: &str, room: &str) -> String {
    format!("{idempotency_key}:{room}")
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HallwayPostProjection {
    pub schema_version: u8,
    pub hallway: String,
    pub sequence: i64,
    pub message_id: i64,
    pub from_room: String,
    pub from_spirit: String,
    pub created_at: String,
    pub to_rooms: Vec<String>,
}

impl HallwayPostProjection {
    pub fn from_receipt(receipt: &HallwayPostReceipt) -> Self {
        let message = &receipt.message;
        Self {
            schema_version: 1,
            hallway: message.hallway.clone(),
            sequence: message.sequence,
            message_id: message.id,
            from_room: message.room.clone(),
            from_spirit: message.spirit.clone(),
            created_at: message.created_at.clone(),
            to_rooms: message.to_rooms.clone(),
        }
    }

    pub fn recipients<'a>(&'a self, allowed_rooms: &'a [String]) -> Vec<&'a str> {
        let rooms = if self.to_rooms.is_empty() { allowed_rooms } else { &self.to_rooms };
        let mut recipients: Vec<_> = rooms.iter()
            .filter(|room| *room != &self.from_room)
            .map(String::as_str)
            .collect();
        recipients.sort_unstable();
        recipients.dedup();
        recipients
    }
}

pub async fn publish(
    context: &async_nats::jetstream::Context,
    projection: &HallwayPostProjection,
    allowed_rooms: &[String],
    idempotency_key: &str,
) -> Result<()> {
    Broker::hallway_stream(context).await.context("configure hallway stream")?;
    let payload: bytes::Bytes = serde_json::to_vec(projection)?.into();
    let mut failure = None;
    for room in projection.recipients(allowed_rooms) {
        let mut headers = async_nats::HeaderMap::new();
        headers.insert("Nats-Msg-Id", hallway_message_id(idempotency_key, room));
        let sent = async {
            context.publish_with_headers(hallway_room_subject(room), headers, payload.clone())
                .await?.await?;
            Ok::<_, anyhow::Error>(())
        }.await;
        if let Err(error) = sent { failure = Some(error); }
    }
    match failure { Some(error) => Err(error), None => Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearth::hallway::{HallwayMessage, HallwayPostDisposition};

    fn receipt() -> HallwayPostReceipt {
        HallwayPostReceipt {
            ok: true,
            disposition: HallwayPostDisposition::Posted,
            message: HallwayMessage {
                id: 42, sequence: 7, hallway: "family".into(), room: "kodo".into(),
                spirit: "Kodo".into(), session: "session".into(), body: "private".into(),
                reply_to: None, created_at: "2026-09-08T00:00:00Z".into(),
                thread: "2026-09-08".into(), to_rooms: vec![],
            },
        }
    }

    #[test]
    fn projection_refuses_private_fields() {
        let value = serde_json::to_value(HallwayPostProjection::from_receipt(&receipt())).unwrap();
        assert!(serde_json::from_value::<HallwayPostProjection>(value.clone()).is_ok());
        for field in ["body", "title"] {
            let mut private = value.clone();
            private[field] = "must not cross".into();
            assert!(serde_json::from_value::<HallwayPostProjection>(private).is_err());
        }
    }

    #[test]
    fn receipt_routes_broadcast_and_addressed_posts_without_sender_or_duplicates() {
        let allowed = vec!["kodo".into(), "kintsu".into(), "tuner".into()];
        let mut receipt = receipt();
        let projection = HallwayPostProjection::from_receipt(&receipt);
        assert_eq!(projection.recipients(&allowed).into_iter().map(hallway_room_subject).collect::<Vec<_>>(),
            ["athanor.hallway.room.kintsu", "athanor.hallway.room.tuner"]);
        receipt.message.to_rooms = vec!["kodo".into(), "tuner".into(), "tuner".into()];
        assert_eq!(HallwayPostProjection::from_receipt(&receipt).recipients(&allowed), ["tuner"]);
        assert_eq!(hallway_message_id("post-key", "tuner"), "post-key:tuner");
        assert_ne!(hallway_message_id("post-key", "tuner"), hallway_message_id("post-key", "kintsu"));
    }
}
