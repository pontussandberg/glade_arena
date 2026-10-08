//! Game client: connects to the server, picks a class, sends inputs, and predicts its own player,
//! projectiles and swings.
//!
//! The networking part (`ClientNetPlugin`) has no rendering, so integration tests can run it
//! headless as a bot. `render` adds the window, 3D scene and keyboard/mouse input on top.

use std::net::SocketAddr;
use std::time::Duration;

use arena_shared::config::*;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::prelude::*;
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
pub mod camera;
#[cfg(feature = "render")]
pub mod dev;
#[cfg(feature = "render")]
pub mod feedback;
#[cfg(feature = "render")]
pub mod glade;
#[cfg(feature = "render")]
pub mod join;
#[cfg(feature = "render")]
pub mod minimap;
#[cfg(feature = "render")]
pub mod render;
#[cfg(feature = "render")]
pub mod rig;

#[derive(Clone)]
pub struct ClientSettings {
    /// Must be unique per connected client.
    pub client_id: u64,
    pub server_addr: SocketAddr,
    /// Hex SHA-256 of the server certificate. Required in the browser; native dev clients may
    /// leave it empty to skip validation.
    pub cert_digest: String,
    /// Simulated latency/jitter/loss on received packets, for testing bad networks.
    pub conditioner: Option<LinkConditionerConfig>,
    /// Class to join as. `None` waits for the join screen (or a bot) to set `ChosenClass`.
    pub class: Option<ClassId>,
}

/// Systems that turn a human's mouse and keyboard into `DesiredInput`. The bot switches this set
/// off and drives `DesiredInput` itself.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerControls;

/// The class this client will join as. Sent to the server once per connection.
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
        app.add_systems(Update, send_class_choice);
        app.add_systems(FixedPreUpdate, write_input.in_set(InputSystems::WriteClientInputs));
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
        WebTransportClientIo { certificate_digest: settings.cert_digest.clone(), target: None },
    ));
}

/// Joining: tell the server our class when we connect (again after a reconnect) or when it's
/// picked, whichever comes last. The server spawns our player and ignores repeats.
fn send_class_choice(
    chosen: Res<ChosenClass>,
    client: Single<(&mut MessageSender<ChooseClass>, Ref<Connected>), With<Client>>,
) {
    let (mut sender, connected) = client.into_inner();
    if let Some(class) = chosen.0
        && (chosen.is_changed() || connected.is_added())
    {
        sender.send::<Reliable>(ChooseClass(class));
    }
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

fn predict_player_movement(
    _synced: SyncedLocalTimeline,
    mut players: Query<
        (&mut Pos, &ClassId, &ActionState<PlayerInput>, &AttackState, &AbilityState, &Health),
        (With<Predicted>, With<PlayerId>),
    >,
) {
    for (mut pos, class, input, attack, ability, health) in &mut players {
        if !health.alive() {
            continue;
        }
        pos.set_if_neq(Pos(sim::move_player(pos.0, &input.0, *class, attack, ability)));
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
    let tick = synced.current_tick().0 as u32;
    for (id, class, pos, input, mut state, mut last_swing, health) in &mut players {
        if !health.alive() {
            continue;
        }
        let (next, released) = sim::step_attack(tick, id.0, *class, pos.0, &input.0, *state);
        state.set_if_neq(next);
        let Some(attack) = released else { continue };
        match attack {
            // Matched to the server's copy by hash when it arrives.
            sim::Attack::Projectile(spawn, projectile) => {
                commands.spawn(projectile.bundle(spawn));
            }
            sim::Attack::Melee(swing) => *last_swing = swing,
        }
    }
}

/// Our Q, right away: a thrown ability is spawned locally, a dash starts moving us. Who gets hit
/// (and a dash readying the auto-attack) is up to the server.
fn predict_ability(
    synced: SyncedLocalTimeline,
    mut commands: Commands,
    mut players: Query<
        (&PlayerId, &ClassId, &Pos, &ActionState<PlayerInput>, &AttackState, &mut AbilityState, &Health),
        With<Predicted>,
    >,
) {
    let tick = synced.current_tick().0 as u32;
    for (id, class, pos, input, attack, mut state, health) in &mut players {
        if !health.alive() {
            continue;
        }
        let (next, thrown) = sim::step_ability(tick, id.0, *class, pos.0, &input.0, attack, *state);
        state.set_if_neq(next);
        if let Some((spawn, projectile)) = thrown {
            commands.spawn(projectile.bundle(spawn));
        }
    }
}

fn predict_projectiles(
    synced: SyncedLocalTimeline,
    mut commands: Commands,
    // Interpolated projectiles (other players') get their position from the server instead.
    mut projectiles: Query<(Entity, &mut Pos, &Projectile), Without<Interpolated>>,
) {
    let tick = synced.current_tick().0 as u32;
    for (entity, mut pos, projectile) in &mut projectiles {
        pos.0 = sim::step_projectile(pos.0, projectile);
        if sim::projectile_expired(pos.0, projectile, tick) {
            // Rollback-aware despawn: restored if a rollback rewinds past this point.
            commands.entity(entity).prediction_despawn();
        }
    }
}
