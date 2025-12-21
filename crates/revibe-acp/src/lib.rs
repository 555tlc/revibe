//! Revibe ACP (Agent Communication Protocol) implementation
//!
//! This crate provides the ACP server implementation for Revibe,
//! allowing communication between the Revibe agent and external clients
//! using the Agent Communication Protocol.

pub mod agent;
pub mod error;
pub mod tools;
pub mod types;

pub use agent::*;
pub use error::*;
pub use types::*;
