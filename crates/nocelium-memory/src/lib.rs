//! Nocelium Memory — Nomen client for collective memory over Unix socket.

mod client;
mod error;
mod types;

pub use client::MemoryClient;
pub use error::MemoryError;
pub use types::{Memory, Visibility};
