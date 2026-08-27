mod disclosure;
mod dispatch;
mod familiar_status;
mod harness_control;
mod health;
mod host_link;
mod host_session;
mod paper_boat_receipt;
mod protocol;
mod recall_policy;
mod routing;
mod shell;
mod tokens;

use godot::prelude::*;

struct AthanorExtension;

#[gdextension]
unsafe impl ExtensionLibrary for AthanorExtension {}
