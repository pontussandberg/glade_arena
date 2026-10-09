//! Server logs: what's worth knowing about a server you can't watch. Info level is the operator's
//! view: start, connects and disconnects (with why), joins, kills, a status line every minute
//! (players, each client's ping), and warnings when the server hitches or a client's inputs stop
//! arriving for a while. Debug adds every hit and respawn.
//!
//! `RUST_LOG` overrides the levels as usual (`RUST_LOG=arena_server=debug`). `ARENA_LOG_FORMAT=json`
//! writes one JSON object per line instead of human-readable lines, for a log collector.

use std::time::Duration;

use arena_shared::config::TICK_DURATION;
use arena_shared::protocol::PlayerId;
use bevy::log::{BoxedFmtLayer, Level, LogPlugin};
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

/// Lightyear and the transport are chatty at info; their warnings and errors still come through.
const QUIET_CRATES: &str = "lightyear=warn,lightyear_netcode=warn,wtransport=warn,quinn=warn,aeronet=warn";
/// How often the status line is written.
const STATUS_EVERY: Duration = Duration::from_secs(60);
/// A frame longer than this means the server fell behind its tick rate.
const HITCH: Duration = Duration::from_millis(100);
const STARTUP: Duration = Duration::from_secs(2);
/// A client whose inputs stop arriving for longer than this gets a warning (shorter gaps are only
/// counted, in the status line).
const LONG_INPUT_GAP_TICKS: u32 = 64;

pub fn log_plugin() -> LogPlugin {
    LogPlugin { level: Level::INFO, filter: QUIET_CRATES.into(), fmt_layer: json_layer, ..default() }
}

fn json_layer(_: &mut App) -> Option<BoxedFmtLayer> {
    let json = std::env::var("ARENA_LOG_FORMAT").is_ok_and(|format| format.eq_ignore_ascii_case("json"));
    json.then(|| {
        let layer = tracing_subscriber::fmt::layer().json().flatten_event(true).with_current_span(false).with_span_list(false);
        Box::new(layer) as BoxedFmtLayer
    })
}

/// Panics go through the log too, so a JSON log collector sees them as errors, not stray text.
pub fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error!(target: "panic", "{info}");
        default(info);
    }));
}

pub struct LoggingPlugin;

impl Plugin for LoggingPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(log_connect).add_observer(log_disconnect);
        app.add_systems(Last, (log_hitches, log_status));
    }
}

/// Server-only, per player: inputs that stopped arriving (see `neutralize_stale_inputs`). `since`
/// is the tick the current gap started; `count` is how many gaps since the last status line.
#[derive(Component, Default)]
pub(crate) struct InputGaps {
    since: Option<u32>,
    count: u32,
}

impl InputGaps {
    /// Called every tick with whether this player's inputs are arriving in time.
    pub(crate) fn update(&mut self, id: PeerId, fresh: bool, tick: u32) {
        match (self.since, fresh) {
            (None, false) => {
                self.since = Some(tick);
                self.count += 1;
            }
            (Some(since), true) => {
                self.since = None;
                let ticks = tick - since;
                if ticks > LONG_INPUT_GAP_TICKS {
                    warn!(client = ?id, gap_ms = ticks_to_ms(ticks), "inputs stopped arriving for a while");
                }
            }
            _ => {}
        }
    }
}

fn ticks_to_ms(ticks: u32) -> u128 {
    (TICK_DURATION * ticks).as_millis()
}

fn log_connect(trigger: On<Add, Connected>, links: Query<(&RemoteId, Option<&PeerAddr>), With<ClientOf>>) {
    let Ok((id, addr)) = links.get(trigger.entity) else { return };
    info!(client = ?id.0, addr = ?addr.map(|a| a.0), "client connected");
}

fn log_disconnect(
    trigger: On<Add, Disconnected>,
    links: Query<(&RemoteId, &Disconnected), With<ClientOf>>,
    players: Query<(), With<PlayerId>>,
) {
    let Ok((id, disconnected)) = links.get(trigger.entity) else { return };
    // Their player is despawned with the link, right after this.
    let players = players.iter().count().saturating_sub(1);
    info!(client = ?id.0, reason = %disconnected.reason, players, "client disconnected");
}

/// The server runs its fixed ticks in catch-up after a long frame; say so when it happens (not
/// while it's starting, which takes a few long frames).
fn log_hitches(time: Res<Time<Real>>) {
    let frame = time.delta();
    if frame > HITCH && time.elapsed() > STARTUP {
        warn!(frame_ms = frame.as_millis(), ticks_behind = frame.div_duration_f32(TICK_DURATION) as u32, "server hitch");
    }
}

/// Every `STATUS_EVERY`: how many are playing, and each client's ping, jitter and input gaps.
fn log_status(
    time: Res<Time<Real>>,
    mut next: Local<Duration>,
    links: Query<(&RemoteId, &Link), (With<ClientOf>, With<Connected>)>,
    mut players: Query<(&PlayerId, &mut InputGaps)>,
) {
    if time.elapsed() < *next {
        return;
    }
    *next = time.elapsed() + STATUS_EVERY;
    let mut clients = Vec::new();
    for (id, link) in &links {
        let gaps = players.iter_mut().find(|(player, _)| player.0 == id.0).map_or(0, |(_, mut gaps)| {
            std::mem::take(&mut gaps.count)
        });
        clients.push(format!(
            "{:?}: {}±{} ms, {gaps} input gaps",
            id.0,
            link.stats.rtt.as_millis(),
            link.stats.jitter.as_millis()
        ));
    }
    info!(connected = links.iter().len(), players = players.iter().len(), clients = clients.join("; "), "status");
}
