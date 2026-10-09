use std::path::PathBuf;

use arena_server::{Certificate, ServerSettings, build_server_app};
use arena_shared::config::SERVER_PORT;

/// `ARENA_PORT` (UDP, default 5888). `ARENA_TLS_CERT` and `ARENA_TLS_KEY`: PEM files of a real
/// certificate (deployed); without them, a self-signed one whose digest goes to `ARENA_DIGEST_OUT`
/// (default `client/web/digest.txt`, so run from the workspace root for the dev web page).
fn main() {
    arena_server::logging::log_panics();
    let certificate = match (std::env::var_os("ARENA_TLS_CERT"), std::env::var_os("ARENA_TLS_KEY")) {
        (Some(cert), Some(key)) => Certificate::Pem { cert: cert.into(), key: key.into() },
        _ => Certificate::SelfSigned {
            digest_out: Some(
                std::env::var_os("ARENA_DIGEST_OUT").map_or_else(|| PathBuf::from("client/web/digest.txt"), PathBuf::from),
            ),
        },
    };
    let port = std::env::var("ARENA_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(SERVER_PORT);
    build_server_app(ServerSettings { port, certificate }).run();
}
