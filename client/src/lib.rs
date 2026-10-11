//! Game client: connects to the server, joins a room (`rooms.rs`), picks a class, sends inputs,
//! and predicts its own player, projectiles and swings.
//!
//! The networking part (`ClientNetPlugin`) has no rendering, so integration tests can run it
//! headless as a bot. `render` adds the window, 3D scene and keyboard/mouse input on top.

use std::net::SocketAddr;
use std::time::Duration;

use arena_shared::config::*;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::prelude::*;
use lightyear::core::timeline::is_in_rollback;
use lightyear::netcode::NetcodeClient;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::prelude::client::input::InputSystems;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;

#[cfg(feature = "render")]
pub mod action_bar;
pub mod bot;
#[cfg(feature = "render")]
pub mod browser;
#[cfg(feature = "render")]
pub mod camera;
#[cfg(feature = "render")]
pub mod cloth;
#[cfg(feature = "render")]
pub mod dummies;
#[cfg(feature = "render")]
pub mod casting;
#[cfg(feature = "render")]
pub mod esc_menu;
#[cfg(feature = "render")]
pub mod feedback;
#[cfg(feature = "render")]
pub mod arena;
#[cfg(feature = "render")]
pub mod lobby;
#[cfg(feature = "render")]
pub mod minimap;
#[cfg(feature = "render")]
pub mod pickups;
#[cfg(feature = "render")]
pub mod render;
#[cfg(feature = "render")]
pub mod rig;
pub mod rooms;
#[cfg(all(test, feature = "render"))]
mod preview;
#[cfg(feature = "render")]
pub mod sculpt;
#[cfg(feature = "render")]
pub mod stat_frame;
#[cfg(feature = "render")]
pub mod swish;
#[cfg(feature = "render")]
pub mod tooltip;

#[derive(Clone)]
pub struct ClientSettings {
    /// Must be unique per connected client.
    pub client_id: u64,
    pub server_addr: SocketAddr,
    /// Hex SHA-256 of the server certificate, for a self-signed one (local dev). Empty for a real
    /// certificate (`server_url`); native clients may also leave it empty to skip validation.
    pub cert_digest: String,
    /// The server's WebTransport URL (`https://host:port`), to connect by name to a server with a
    /// real certificate. `server_addr` then only goes in the netcode connect token, which the
    /// server checks against its own address: `127.0.0.1` and the server's port pass for a server
    /// listening on all addresses (`0.0.0.0`).
    pub server_url: Option<String>,
    /// Simulated latency/jitter/loss on received packets, for testing bad networks.
    pub conditioner: Option<LinkConditionerConfig>,
    /// Class to join as. `None` waits for the room's lobby (or a bot) to set `ChosenClass`.
    pub class: Option<ClassId>,
    /// A room to join (or make and start) right away, skipping the browser.
    pub quick_join: Option<String>,
    /// The guest name we had last time, to ask for again.
    pub guest_name: Option<String>,
}

/// Systems that turn a human's mouse and keyboard into `DesiredInput`. The bot switches this set
/// off and drives `DesiredInput` itself.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerControls;

/// The class this client enters the arena as. Sent to the server whenever it's picked in a room.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct ChosenClass(pub Option<ClassId>);

/// What the local player wants to do this frame. Written by keyboard/mouse (render) or by a bot,
/// then copied into the networked input buffer every tick.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct DesiredInput(pub PlayerInput);

/// Builds a client app without rendering, for tests and bots.
pub fn build_headless_client_app(settings: ClientSettings) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))),
        bevy::state::app::StatesPlugin,
    ));
    app.add_plugins(ClientPlugins { tick_duration: TICK_DURATION });
    app.add_plugins(ClientNetPlugin { settings });
    app
}

pub struct ClientNetPlugin {
    pub settings: ClientSettings,
}

impl Plugin for ClientNetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ProtocolPlugin);
        app.add_plugins(rooms::RoomsNetPlugin {
            quick_join: self.settings.quick_join.clone(),
            guest_name: self.settings.guest_name.clone(),
        });
        app.init_resource::<DesiredInput>();
        app.insert_resource(ChosenClass(self.settings.class));
        // Apply our own inputs immediately and cover the round trip with prediction.
        app.insert_resource(
            InputTimelineConfig::default().with_input_delay(InputDelayConfig::no_input_delay()),
        );
        app.insert_resource(PredictionManager::default());

        spawn_client_entity(app.world_mut(), &self.settings);
        app.add_systems(Startup, |mut commands: Commands, client: Single<Entity, With<Client>>| {
            commands.trigger(Connect { entity: client.into_inner() });
        });

        app.add_observer(mark_controlled_player);
        // Not during rollback replays: lightyear replays the inputs it buffered then, and
        // `write_input` would use up a Q press that hasn't been sent yet.
        app.add_systems(
            FixedPreUpdate,
            write_input.in_set(InputSystems::WriteClientInputs).run_if(not(is_in_rollback)),
        );
        // Same rules, same order as the server, but only for what this client predicts.
        app.add_systems(
            FixedUpdate,
            (predict_player_movement, predict_attack, predict_ability, cancel_walk_on_attack, predict_projectiles).chain(),
        );
    }
}

fn spawn_client_entity(world: &mut World, settings: &ClientSettings) {
    let auth = Authentication::Manual {
        server_addr: settings.server_addr,
        client_id: settings.client_id,
        private_key: DEV_PRIVATE_KEY,
        protocol_id: PROTOCOL_ID,
    };
    let netcode = NetcodeClient::new(
        auth,
        NetcodeConfig { client_timeout_secs: 3, token_expire_secs: -1, ..default() },
    )
    .expect("invalid netcode config");
    let conditioner = settings.conditioner.clone().map(RecvLinkConditioner::new);
    world.spawn((
        Name::from("Client"),
        Client,
        ReplicationReceiver,
        Link::default().with_conditioner(conditioner),
        LocalAddr(SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), 0)),
        PeerAddr(settings.server_addr),
        netcode,
        WebTransportClientIo { certificate_digest: settings.cert_digest.clone(), target: settings.server_url.clone() },
    ));
}

/// The server marks our own player as `Controlled`; that's the one we write inputs to.
fn mark_controlled_player(
    trigger: On<Add, Controlled>,
    players: Query<(), (With<PlayerId>, Without<InputMarker<PlayerInput>>)>,
    mut commands: Commands,
) {
    if players.contains(trigger.entity) {
        commands.entity(trigger.entity).insert(InputMarker::<PlayerInput>::default());
    }
}

/// Copies `DesiredInput` into the networked input. A Q press is sent for one tick.
fn write_input(
    mut desired: ResMut<DesiredInput>,
    mut query: Query<&mut ActionState<PlayerInput>, With<InputMarker<PlayerInput>>>,
) {
    if let Ok(mut action) = query.single_mut() {
        action.0 = desired.0;
        desired.0.ability = false;
    }
}

/// Starting an attack or a dash drops the destination: afterwards you stand still, unless you
/// clicked somewhere new meanwhile, which is then where you walk. Runs in the tick it starts, so
/// a click read later that frame counts as the new one; once per attack or dash (by its start
/// tick), so rollbacks replaying that tick don't drop it. (Arriving or being moved also forgets
/// the destination; that's in `render::read_local_input`.)
fn cancel_walk_on_attack(
    mut desired: ResMut<DesiredInput>,
    me: Query<(&AttackState, &AbilityState), With<InputMarker<PlayerInput>>>,
    mut cancelled_for: Local<[Option<u32>; 2]>,
) {
    let Ok((attack, ability)) = me.single() else { return };
    let started = [attack.windup.map(|w| w.started_at), ability.dash.map(|d| d.started_at)];
    for (started, cancelled) in started.into_iter().zip(cancelled_for.iter_mut()) {
        if started.is_some() && *cancelled != started {
            *cancelled = started;
            desired.0.move_to = None;
        }
    }
}

// The four systems below only run once the client's timeline is synced with the server
// (`SyncedLocalTimeline` makes Bevy skip them until then).

/// Our movement, right away. Slows and roots (`Chilled`) and hastes (`Hasted`) are the server's:
/// one lands on us a round trip late, and the rollback it causes replays our movement with it
/// from when it began.
fn predict_player_movement(
    synced: SyncedLocalTimeline,
    mut players: Query<
        (&mut Pos, &ClassId, &ActionState<PlayerInput>, &AttackState, &AbilityState, (&Chilled, &Hasted), &Health),
        (With<Predicted>, With<PlayerId>),
    >,
) {
    let tick = synced.current_tick().0;
    for (mut pos, class, input, attack, ability, (chilled, hasted), health) in &mut players {
        if !health.alive() {
            continue;
        }
        pos.set_if_neq(Pos(sim::move_player(pos.0, &input.0, *class, attack, ability, chilled, hasted, tick)));
    }
}

/// Our auto-attack, right away: projectiles are spawned locally, swings are shown locally. Who
/// gets hit (and damage) is up to the server.
fn predict_attack(
    synced: SyncedLocalTimeline,
    mut commands: Commands,
    mut players: Query<
        (&PlayerId, &ClassId, &Pos, &ActionState<PlayerInput>, &mut AttackState, &mut LastSwing, &Health),
        With<Predicted>,
    >,
) {
    let tick = synced.current_tick().0;
    for (id, class, pos, input, mut state, mut last_swing, health) in &mut players {
        if !health.alive() {
            continue;
        }
        let (next, released) = sim::step_attack(tick, id.0, *class, pos.0, &input.0, *state);
        state.set_if_neq(next);
        let Some(attack) = released else { continue };
        match attack {
            // Matched to the server's copy by hash when it arrives.
            sim::Attack::Projectile(projectile) => {
                commands.spawn(projectile.bundle());
            }
            sim::Attack::Melee(swing) => *last_swing = swing,
        }
    }
}

/// Our Q, right away: a thrown ability is spawned locally, a dash starts moving us, a nova's
/// burst shows (from `AbilityState`). Who gets hit (and a dash readying the auto-attack) is up to
/// the server.
fn predict_ability(
    synced: SyncedLocalTimeline,
    mut commands: Commands,
    mut players: Query<
        (&PlayerId, &ClassId, &Pos, &ActionState<PlayerInput>, &AttackState, &Chilled, &mut AbilityState, &Health),
        With<Predicted>,
    >,
) {
    let tick = synced.current_tick().0;
    for (id, class, pos, input, attack, chilled, mut state, health) in &mut players {
        if !health.alive() {
            continue;
        }
        let (next, cast) = sim::step_ability(tick, id.0, *class, pos.0, &input.0, attack, chilled, *state);
        state.set_if_neq(next);
        if let Some(sim::Cast::Throw(projectile)) = cast {
            commands.spawn(projectile.bundle());
        }
    }
}

/// Every projectile, ours and others': where it is this tick. Others' arrive from the server
/// a round trip late, with the position they had back then; this puts them where they really
/// are now, on the same clock as our own player.
fn predict_projectiles(
    synced: SyncedLocalTimeline,
    mut commands: Commands,
    mut projectiles: Query<(Entity, &mut Pos, &Projectile)>,
) {
    let tick = synced.current_tick().0;
    for (entity, mut pos, projectile) in &mut projectiles {
        pos.set_if_neq(Pos(sim::projectile_pos(projectile, tick as f32)));
        if sim::projectile_expired(pos.0, projectile, tick) {
            // Rollback-aware despawn: restored if a rollback rewinds past this point.
            commands.entity(entity).prediction_despawn();
        }
    }
}
