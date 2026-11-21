use super::{Effect, invalid, mode};
use hearth::orihon::{FolderBackend, FolderRuntime, FolderSettings, Mode, RecordingState};
use protocol::{ProtocolErrorBody, organ::OrganRequest};
use serde::Deserialize;

/// Only a new generation snapshots the book cursor. A status request writes nothing.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ModeWrite {
    Status,
    Settings,
    NewGeneration,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    mode: Option<Mode>,
    model: Option<String>,
    backend: Option<FolderBackend>,
    runtime: Option<FolderRuntime>,
}

impl Request {
    fn into_folder(self) -> Result<Option<FolderSettings>, ProtocolErrorBody> {
        match (self.model, self.backend, self.runtime) {
            (None, None, None) => Ok(None),
            _ if self.mode != Some(Mode::Fold) => {
                Err(invalid("A folder configuration requires fold mode"))
            }
            (Some(model), backend, runtime) => Ok(Some(FolderSettings {
                model,
                backend: backend.unwrap_or_default(),
                runtime,
            })),
            _ => Err(invalid("A folder configuration requires a model")),
        }
    }
}

pub(super) fn change_mode(
    state: &mut RecordingState,
    request: OrganRequest,
) -> Result<(ModeWrite, Effect), ProtocolErrorBody> {
    let request: Request = serde_json::from_value(serde_json::Value::Object(request.params))
        .map_err(|_| invalid("Orihon mode must be off, log, or fold"))?;
    let next = request.mode;
    let folder = request.into_folder()?;

    let Some(next) = next else {
        return Ok((ModeWrite::Status, Effect::None));
    };

    let selected = folder.as_ref().or(state.folder.as_ref());
    mode::validate_policy(next, selected)?;

    let effect = match next {
        Mode::Fold => Effect::Join {
            folder: selected
                .ok_or_else(|| invalid("fold mode requires a folder configuration"))?
                .clone(),
            explicit: folder.is_some(),
        },
        Mode::Log | Mode::Off => Effect::Leave,
    };
    let write = if state.mode == Mode::Off && next != Mode::Off {
        state.generation = uuid::Uuid::new_v4().to_string();
        ModeWrite::NewGeneration
    } else {
        ModeWrite::Settings
    };

    state.mode = next;
    if let Some(folder) = folder {
        state.folder = Some(folder);
    }

    Ok((write, effect))
}
