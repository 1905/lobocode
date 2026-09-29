//! Laptop logic shared by the CLI and app. Same config, state and pod environment as Go.
pub mod clock;
pub mod error;
pub mod http;
pub use error::{Error, Result};
pub mod config;
