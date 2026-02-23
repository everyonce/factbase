//! Web UI server module.
//!
//! Provides a localhost web interface for human-in-the-loop operations
//! (review questions, organize suggestions, orphan management).

pub mod api;
mod assets;
mod server;

pub use assets::{index_handler, serve_asset, static_handler, Assets};
pub use server::{start_web_server, WebServer};
