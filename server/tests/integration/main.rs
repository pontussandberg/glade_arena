//! The end-to-end tests: a real server and headless bot clients over WebTransport on localhost.
//! One test binary instead of one per file: each links all of Bevy and lightyear, which is slow
//! and, many at once, can run the linker out of memory.

mod common;

mod abilities;
mod combat;
mod inputs;
mod netcode;
mod pickups;
mod rooms;
