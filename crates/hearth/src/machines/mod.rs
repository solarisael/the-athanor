//! The House's state machines. Each module owns one machine: its states, its legal moves,
//! and the decision that picks the next move. Time arrives as input; nothing here reads a clock.
//! The contract for each machine lives in `docs/state-machines/`.

pub mod hallway_knock;
pub mod knock_turn;

/// One move a machine made, for Insula's transition log. Every name is a mechanical atom
/// the machine module owns; `subject_id` names the one row or object that moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub machine: &'static str,
    pub subject_id: String,
    pub from: &'static str,
    pub event: &'static str,
    pub to: &'static str,
}
