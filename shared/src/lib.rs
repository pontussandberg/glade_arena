//! Code shared by the server and the client.
//!
//! `protocol` defines what goes over the wire. `sim` holds the gameplay rules as pure
//! functions: the server runs them; the client uses the few it draws with, and the movement and
//! attack steps its prediction of our own fighter replays (`render::glide`).

pub mod abilities;
pub mod classes;
pub mod config;
pub mod items;
pub mod map;
pub mod protocol;
pub mod rooms;
pub mod sim;

pub use protocol::ProtocolPlugin;
