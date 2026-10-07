use std::path::PathBuf;

use arena_server::{ServerSettings, build_server_app};
use arena_shared::config::SERVER_PORT;

fn main() {
    // Run from the workspace root so the dev web page can pick up the digest.
    let digest_out = std::env::var("ARENA_DIGEST_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("client/web/digest.txt"));
    let port = std::env::var("ARENA_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(SERVER_PORT);
    build_server_app(ServerSettings { port, digest_out: Some(digest_out) }).run();
}
