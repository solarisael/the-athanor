mod dispatch;
mod familiar_status;
mod host_link;
mod host_session;
mod health;
mod paper_boat_receipt;
mod protocol;
mod recall_policy;
mod routing;
mod shell;

use godot::prelude::*;

struct AthanorExtension;

#[gdextension]
unsafe impl ExtensionLibrary for AthanorExtension {}
