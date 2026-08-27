//! Layout design tokens for the operator shell.
//!
//! The shell decides its own docking and column counts in Rust, so the numbers
//! that decide them are named here rather than sitting inline at the two places
//! that read them. A screen that needs to agree with the shell's layout reads
//! these, never a second copy of the same number.
//!
//! enough: plain consts are the seed of a design-token set. The way up is one
//! token source the .tres themes and Rust both read, so a token moves once and
//! the running shell and the scene files cannot disagree.

/// At or above this width the shell docks both navigators.
pub const WIDE_BREAKPOINT: f32 = 1_200.0;
/// At or above this width the shell docks the left navigator only.
pub const COMPACT_BREAKPOINT: f32 = 800.0;

/// Center-frame margins that hold the docked navigators off the content.
pub const WIDE_LEFT_MARGIN: i32 = 252;
pub const WIDE_RIGHT_MARGIN: i32 = 316;
pub const COMPACT_LEFT_MARGIN: i32 = 232;
/// No navigator docked on that side, so the center frame keeps the full width.
pub const NO_MARGIN: i32 = 0;

/// Columns of the recall state grid, one count per layout class.
pub const WIDE_RECALL_STATE_COLUMNS: i32 = 4;
pub const COMPACT_RECALL_STATE_COLUMNS: i32 = 2;
pub const NARROW_RECALL_STATE_COLUMNS: i32 = 1;
