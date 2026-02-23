//! Planning for reorganization operations.
//!
//! This module creates plans for merge, split, and other reorganization
//! operations with fact-level accounting.

mod merge;
mod split;

pub use merge::{plan_merge, MergePlan};
pub use split::{plan_split, ProposedDocument, SplitPlan};
