// the House clock: absolute stage instants off the wire, never a local stopwatch
pub mod clock;
pub mod config;
pub mod decide;
pub mod keeper;
// public so the smoke fixtures speak the keeper's own method names and wire
// vocabulary instead of repeating string literals (coding#446 rule 4)
pub mod protocol;
pub mod resolve;
mod session;
