//! The TLS certificate WebTransport runs on: made up for local dev, or a real one loaded from
//! PEM files and picked up again when it's renewed.

use super::*;

/// How often to check whether the certificate was renewed.
pub(super) const CERT_CHECK_EVERY: Duration = Duration::from_secs(600);

/// The certificate file the server runs on, and when it was last modified as loaded.
#[derive(Resource)]
pub(super) struct CertificateFile {
    pub(super) path: PathBuf,
    pub(super) loaded: Option<std::time::SystemTime>,
}

pub(super) fn self_signed(digest_out: Option<PathBuf>) -> Identity {
    let identity = Identity::self_signed(["localhost", "127.0.0.1", "::1"])
        .expect("failed to generate self-signed certificate");
    let digest: String = identity.certificate_chain().as_slice()[0]
        .hash()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    info!("WebTransport certificate digest: {digest}");
    if let Some(path) = &digest_out {
        std::fs::write(path, &digest).expect("failed to write certificate digest");
        info!("Wrote certificate digest to {}", path.display());
    }
    identity
}

pub(super) fn load_pem(cert: &std::path::Path, key: &std::path::Path) -> Identity {
    if !(cert.exists() && key.exists()) {
        info!(cert = %cert.display(), "waiting for the TLS certificate");
        while !(cert.exists() && key.exists()) {
            std::thread::sleep(Duration::from_secs(2));
        }
    }
    // wtransport reads the files with tokio, which needs its runtime around.
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("failed to start tokio");
    let identity = runtime
        .block_on(Identity::load_pemfiles(cert, key))
        .unwrap_or_else(|e| panic!("failed to load the TLS certificate {}: {e}", cert.display()));
    info!(cert = %cert.display(), "loaded the TLS certificate");
    identity
}

pub(super) fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Exits when the certificate file changes, to be restarted with it (see `Certificate::Pem`).
pub(super) fn restart_on_renewal(
    time: Res<Time<Real>>,
    mut next: Local<Duration>,
    file: Res<CertificateFile>,
    mut exit: MessageWriter<AppExit>,
) {
    if time.elapsed() < *next {
        return;
    }
    *next = time.elapsed() + CERT_CHECK_EVERY;
    if modified(&file.path) != file.loaded {
        info!(cert = %file.path.display(), "the TLS certificate was renewed; exiting to restart with it");
        exit.write(AppExit::Success);
    }
}
