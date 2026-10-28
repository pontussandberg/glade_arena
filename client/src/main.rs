use std::net::{Ipv4Addr, SocketAddr};

use arena_client::{ClientNetPlugin, ClientSettings, render::RenderPlugin};
use arena_shared::config::*;
use arena_shared::protocol::ClassId;
use bevy::prelude::*;
use bevy::winit::WinitSettings;
use lightyear::prelude::client::ClientPlugins;

#[cfg(target_family = "wasm")]
mod hidden_tab;

/// How often a client in the background (`ARENA_BACKGROUND`) draws a frame, a second.
const BACKGROUND_FPS: f32 = 30.0;

fn main() {
    #[cfg(target_family = "wasm")]
    console_error_panic_hook::set_once();

    let settings = client_settings();
    let mut app = App::new();
    // `ARENA_BACKGROUND=1` (native): a small window, drawn at a low frame rate, for a bot played
    // beside a client being measured, taking as little of the machine from it as it can.
    #[cfg(not(target_family = "wasm"))]
    let background = std::env::var_os("ARENA_BACKGROUND").is_some();
    #[cfg(target_family = "wasm")]
    let background = false;
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("Arena (client {})", settings.client_id),
            canvas: Some("#game".into()),
            fit_canvas_to_parent: true,
            resolution: if background { (480, 270).into() } else { default() },
            ..default()
        }),
        ..default()
    }));
    // Keep simulating at full rate when the window is unfocused (two windows side by side).
    app.insert_resource(if background {
        let mode = bevy::winit::UpdateMode::reactive_low_power(std::time::Duration::from_secs_f32(1.0 / BACKGROUND_FPS));
        WinitSettings { focused_mode: mode, unfocused_mode: mode }
    } else {
        WinitSettings::continuous()
    });
    #[cfg(target_family = "wasm")]
    app.add_plugins(hidden_tab::HiddenTabPlugin);
    app.add_plugins(ClientPlugins { tick_duration: TICK_DURATION });
    app.add_plugins(ClientNetPlugin { settings });
    if bot_plays() {
        app.add_plugins(arena_client::bot::BotPlugin);
    }
    app.add_plugins(RenderPlugin);
    #[cfg(not(target_family = "wasm"))]
    if let Some(out) = std::env::var_os("ARENA_PERF") {
        app.add_plugins(arena_client::perf::PerfPlugin { out: out.into() });
    }
    #[cfg(not(target_family = "wasm"))]
    if let Some(dir) = std::env::var_os("ARENA_SHOTS") {
        app.insert_resource(Snapshots { dir: dir.into(), next: 0.0, taken: 0 });
        app.add_systems(Update, take_snapshots);
    }
    app.run();
}

/// `ARENA_SHOTS=dir`: a screenshot of the window every `SNAPSHOT_EVERY` seconds, saved there as
/// numbered PNGs, to look at a session afterwards (with `ARENA_BOT=1`, one nobody played).
#[cfg(not(target_family = "wasm"))]
#[derive(Resource)]
struct Snapshots {
    dir: std::path::PathBuf,
    next: f32,
    taken: u32,
}

#[cfg(not(target_family = "wasm"))]
const SNAPSHOT_EVERY: f32 = 4.0;

#[cfg(not(target_family = "wasm"))]
fn take_snapshots(mut commands: Commands, time: Res<Time>, mut shots: ResMut<Snapshots>) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    if time.elapsed_secs() < shots.next {
        return;
    }
    shots.next = time.elapsed_secs() + SNAPSHOT_EVERY;
    shots.taken += 1;
    let path = shots.dir.join(format!("shot_{:03}.png", shots.taken));
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

fn default_server_addr() -> SocketAddr {
    SocketAddr::new(Ipv4Addr::LOCALHOST.into(), SERVER_PORT)
}

/// Native dev client: `arena-client [client_id] [class]`, no certificate validation. Without a
/// class (e.g. `javelinist`) it opens the server browser; with one, it joins (or makes and
/// starts) the room `ARENA_ROOM` (default "Sparring") as it. `ARENA_SERVER=ip:port` picks another server
/// (like the page's `?server=`), `ARENA_SERVER_URL=https://host:port` a deployed one by name;
/// `ARENA_BOT=1` lets a simple bot play this client.
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
        server_url: std::env::var("ARENA_SERVER_URL").ok(),
        conditioner: None,
        class,
        quick_join: class.map(|_| std::env::var("ARENA_ROOM").unwrap_or_else(|_| "Sparring".into())),
        guest_name: None,
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

/// Browser client: index.html puts on `window`, before starting the wasm module, either the
/// deployed server's URL (`ARENA_SERVER_URL`, real certificate) or, in local dev, the
/// self-signed certificate's digest (`ARENA_CERT_DIGEST`) and optionally a server address.
#[cfg(target_family = "wasm")]
fn client_settings() -> ClientSettings {
    let window = web_sys::window().expect("no window");
    let get = |key: &str| {
        js_sys::Reflect::get(&window, &key.into())
            .ok()
            .and_then(|v| v.as_string())
    };
    let cert_digest = get("ARENA_CERT_DIGEST").unwrap_or_default();
    let server_addr = get("ARENA_SERVER")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(default_server_addr);
    ClientSettings {
        client_id: (js_sys::Math::random() * u32::MAX as f64) as u64,
        server_addr,
        cert_digest,
        server_url: get("ARENA_SERVER_URL"),
        conditioner: None,
        // In local dev, `?class=…&room=…` joins that room as that class, as the native client's
        // arguments do.
        class: get("ARENA_CLASS").and_then(|key| ClassId::by_key(&key)),
        quick_join: get("ARENA_ROOM"),
        guest_name: arena_client::rooms::load_guest_name(),
    }
}

/// Whether a bot plays this client: `ARENA_BOT=1` natively, `?bot` on the page in local dev.
fn bot_plays() -> bool {
    #[cfg(target_family = "wasm")]
    return web_sys::window().is_some_and(|window| js_sys::Reflect::has(&window, &"ARENA_BOT".into()).unwrap_or(false));
    #[cfg(not(target_family = "wasm"))]
    return std::env::var_os("ARENA_BOT").is_some();
}
