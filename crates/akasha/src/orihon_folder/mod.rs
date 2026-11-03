//! Orihon's compactor: the engine that builds live-tree node lines through the room's folder
//! model, locally through Ollama or hosted through the operator's bound OMP gateway.

pub mod engine;
mod gateway;
pub mod model;

pub use engine::{Completion, Engine, Task};
pub use gateway::{hosted_route, validate_runtime};
