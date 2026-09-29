//! Laptop logic shared by the CLI and app. Same config, state and pod environment as Go.
pub mod clock;
pub mod error;
pub mod http;
pub use error::{Error, Result};
pub mod bootstrap;
pub mod checks;
pub mod config;
pub mod genkey;
pub mod local;
pub mod provider;
pub mod release;
