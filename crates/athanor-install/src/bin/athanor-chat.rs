//! A terminal mouth for the chat projection.
//!
//! `athanor-chat [--room ROOM]` connects to the room's Host, prints the
//! conversation ring, and turns each stdin line into a say. The room's
//! doorman answers through the same projection, so this is the whole talk
//! loop with no GUI in the path.

use anyhow::{Context, Result, bail};
use athanor_install::omp::ClientProjection;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::env;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

const SCHEMA_VERSION: u8 = 1;

struct Wire {
    house_id: String,
    room: String,
    spirit: String,
    session: String,
}

impl Wire {
    fn envelope(&self, kind: &str, extra: Value) -> String {
        let id = Uuid::new_v4().to_string();
        let now = chrono_like_now();
        let mut body = json!({
            "schema_version": SCHEMA_VERSION,
            "message_id": id,
            "house_id": self.house_id,
            "sender_room": self.room,
            "sender_spirit": self.spirit,
            "sender_session": self.session,
            "recipient": "house-host",
            "command_or_event_type": kind,
            "correlation_id": id,
            "causation_id": "",
            "reply_target": self.session,
            "idempotency_key": id,
            "source_record_refs": [],
            "scope": format!("room:{}:recall_policy", self.room),
            "visibility": "operator",
            "authority_class": "room_state",
            "created_at": now.0,
            "expires_at": now.1,
            "max_hops": 1,
            "projection_id": "chat",
        });
        if let (Some(object), Some(fields)) = (body.as_object_mut(), extra.as_object()) {
            for (key, value) in fields {
                object.insert(key.clone(), value.clone());
            }
        }
        body.to_string()
    }
}

/// Now and one minute from now, both RFC 3339 UTC.
fn chrono_like_now() -> (String, String) {
    let now = chrono::Utc::now();
    let format = |time: chrono::DateTime<chrono::Utc>| {
        time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    };
    (format(now), format(now + chrono::Duration::seconds(60)))
}

fn render(message: &Value) {
    let name = message["authorName"].as_str().unwrap_or("?");
    let at = message["at"].as_str().unwrap_or("");
    let text = message["text"].as_str().unwrap_or("");
    let clock = at.get(11..16).unwrap_or("");
    println!("[{clock}] {name}: {text}");
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let requested_room = match arguments.as_slice() {
        [] => None,
        [flag, value] if flag == "--room" => Some(value.clone()),
        _ => bail!("usage: athanor-chat [--room ROOM]"),
    };
    let client = ClientProjection::installed()?;
    let room = requested_room.unwrap_or_else(|| client.default_room.clone());
    let (url, identity) = client.room_ws_url(&room)?;
    let wire = Wire {
        house_id: client.house_id.clone(),
        room: room.clone(),
        spirit: identity.spirit.clone(),
        session: format!("chat-cli:{}", Uuid::new_v4()),
    };

    let mut request = url.as_str().into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", client.host_token).parse()?,
    );
    let (socket, _) = connect_async(request)
        .await
        .with_context(|| format!("connect to the Host at {url}"))?;
    let (mut sink, mut source) = socket.split();

    sink.send(Message::Text(wire.envelope("athanor.chat.subscribe", json!({})).into()))
        .await?;
    println!("connected to {room}; type to speak, Ctrl+C to leave");

    let (say_sender, mut say_receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { return };
            if say_sender.send(line).is_err() {
                return;
            }
        }
    });

    loop {
        tokio::select! {
            line = say_receiver.recv() => {
                let Some(line) = line else { break };
                let text = line.trim();
                if text.is_empty() { continue; }
                let say = wire.envelope("athanor.chat.say", json!({
                    "chat_say": {
                        "room": room,
                        "text": text,
                        "sayId": format!("say-{}", Uuid::new_v4()),
                    },
                }));
                sink.send(Message::Text(say.into())).await?;
            }
            incoming = source.next() => {
                let Some(incoming) = incoming else { break };
                let Message::Text(frame) = incoming? else { continue };
                let Ok(event) = serde_json::from_str::<Value>(frame.as_str()) else { continue };
                match event["command_or_event_type"].as_str().unwrap_or("") {
                    "athanor.chat.snapshot" | "athanor.chat.delta" => {
                        for message in event["messages"].as_array().into_iter().flatten() {
                            render(message);
                        }
                    }
                    "athanor.chat.command_refused" => {
                        eprintln!(
                            "refused: {}",
                            event["reason"].as_str().unwrap_or("(no reason)")
                        );
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
