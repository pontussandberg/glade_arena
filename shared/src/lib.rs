//! Code shared by the server and the client.
//!
//! `protocol` defines what goes over the wire. `sim` holds the gameplay rules as pure
//! functions, so the server (authoritative) and the client (prediction) run the exact
//! same code, which is what keeps rollbacks rare.

pub mod config;
pub mod map;
pub mod protocol;
pub mod sim;

pub use protocol::ProtocolPlugin;
