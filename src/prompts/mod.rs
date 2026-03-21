//! Workflow instruction constants.
//!
//! All workflow step instruction text lives here, organized by workflow.
//! These are compiled-in `const` strings — the canonical defaults.
//!
//! ## Override priority (highest first)
//! 1. KB-level: `.factbase/instructions/{workflow}.toml` in the repo root
//! 2. User-level: `~/.config/factbase/instructions/{workflow}.toml`
//! 3. Compiled constants in this module
//!
//! Override files use TOML with step names as keys:
//! ```toml
//! scan = """Custom scan instruction..."""
//! check = """Custom check instruction..."""
//! ```

pub mod correct;
pub mod enrich;
pub mod improve;
pub mod ingest;
pub mod resolve;
pub mod setup;
pub mod shared;
pub mod transition;
