use std::net::{Ipv4Addr, SocketAddr};

use arena_client::{ClientNetPlugin, ClientSettings, render::RenderPlugin};
use arena_shared::config::*;
#[cfg(not(target_family = "wasm"))]
use arena_shared::protocol::ClassId;
use bevy::prelude::*;
use bevy::winit::WinitSettings;
use lightyear::prelude::client::ClientPlugins;

#[cfg(target_family = "wasm")]
mod hidden_tab;

fn main() {
    #[cfg(target_family = "wasm")]
    console_error_panic_hook::set_once();

    let settings = client_settings();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("Arena (client {})", settings.client_id),
            canvas: Some("#game".into()),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }));
    // Keep simulating at full rate when the window is unfocused (two windows side by side).
    app.insert_resource(WinitSettings::continuous());
    #[cfg(target_family = "wasm")]
    app.add_plugins(hidden_tab::HiddenTabPlugin);
    app.add_plugins(ClientPlugins { tick_duration: TICK_DURATION });
    app.add_plugins(ClientNetPlugin { settings });
    #[cfg(not(target_family = "wasm"))]
    if std::env::var_os("ARENA_BOT").is_some() {
        app.add_plugins(arena_client::bot::BotPlugin);
    }
    app.add_plugins(RenderPlugin);
    if dev_mode() {
        app.add_plugins(arena_client::dev::DevPlugin);
    }
    app.run();
}

fn default_server_addr() -> SocketAddr {
    SocketAddr::new(Ipv4Addr::LOCALHOST.into(), SERVER_PORT)
}

/// Dev tools (`dev.rs`): `ARENA_DEV=1` natively, `?dev` on the page.
#[cfg(not(target_family = "wasm"))]
fn dev_mode() -> bool {
    std::env::var_os("ARENA_DEV").is_some()
}

#[cfg(target_family = "wasm")]
fn dev_mode() -> bool {
    web_sys::window().is_some_and(|w| js_sys::Reflect::get(&w, &"ARENA_DEV".into()).is_ok_and(|v| v.is_truthy()))
}

/// Native dev client: `arena-client [client_id] [class]`, no certificate validation. Without a
/// class (e.g. `javelinist`) it opens the lobby. `ARENA_SERVER=ip:port` picks another server
/// (like the page's `?server=`); `ARENA_BOT=1` lets a simple bot play this client; `ARENA_DEV=1`
/// turns on dev tools.
#[cfg(not(target_family = "wasm"))]
fn client_settings() -> ClientSettings {
    let client_id = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or_else(random_id);
    let server_addr = std::env::var("ARENA_SERVER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(default_server_addr);
    let class = std::env::args().nth(2).and_then(|key| ClassId::by_key(&key));
    ClientSettings {
        client_id,
        server_addr,
        cert_digest: String::new(),
        conditioner: None,
        class,
    }
}

#[cfg(not(target_family = "wasm"))]
fn random_id() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos ^ ((std::process::id() as u64) << 32)
}

/// Browser client: index.html puts the server's certificate digest (and optionally a server
/// address) on `window` before starting the wasm module.
#[cfg(target_family = "wasm")]
fn client_settings() -> ClientSettings {
    let window = web_sys::window().expect("no window");
    let get = |key: &str| {
        js_sys::Reflect::get(&window, &key.into())
            .ok()
            .and_then(|v| v.as_string())
    };
    let cert_digest = get("ARENA_CERT_DIGEST").expect("index.html must set window.ARENA_CERT_DIGEST");
    let server_addr = get("ARENA_SERVER")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(default_server_addr);
    ClientSettings {
        client_id: (js_sys::Math::random() * u32::MAX as f64) as u64,
        server_addr,
        cert_digest,
        conditioner: None,
        class: None,
    }
}
