mod host_link;
mod paper_boat_receipt;
mod protocol;
mod recall_policy;
mod shell;

use godot::prelude::*;

struct AthanorExtension;

#[gdextension]
unsafe impl ExtensionLibrary for AthanorExtension {}
