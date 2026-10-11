//! MCP (Model Context Protocol) support.
//!
//! DeskSeek is an MCP *client*: it launches servers the user configured and
//! offers their tools to the model. The protocol work — discovery, version
//! negotiation, pagination, multi-round-trip requests — is done by the official
//! `rmcp` SDK rather than by hand.

pub mod config;
pub mod manager;

pub use config::McpServer;
pub use manager::{McpManager, ServerStatus};
