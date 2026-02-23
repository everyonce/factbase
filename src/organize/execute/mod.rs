//! Execution of reorganization operations.
//!
//! This module handles the actual execution of merge, split, move, and retype
//! operations with proper verification and rollback support.

mod merge;
mod r#move;
mod retype;
mod split;

pub use merge::{execute_merge, MergeResult};
pub use r#move::{execute_move, MoveResult};
pub use retype::{execute_retype, extract_type_override, RetypeResult};
pub use split::{execute_split, SplitResult};
