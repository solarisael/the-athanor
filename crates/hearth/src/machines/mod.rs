//! The House's state machines. Each module owns one machine: its states, its legal moves,
//! and the decision that picks the next move. Time arrives as input; nothing here reads a clock.
//! The contract for each machine lives in `docs/state-machines/`.

pub mod knock_turn;
