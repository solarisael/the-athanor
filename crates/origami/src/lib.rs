//! The House message shapes. Boats: stasis with a return point (sleep/
//! wake). Cranes: movement with a destination (NATS pointers). Hallways:
//! room letters (PostgreSQL + Host only — no NATS lane behind them yet).
//! PostgreSQL owns every body; NATS only ever carries pointers.

pub mod boats;
pub mod cranes;
pub mod hallways;
pub mod sea;
