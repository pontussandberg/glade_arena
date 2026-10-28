//! Game client: connects to the server, joins a room (`rooms.rs`), picks a class, and sends
//! commands (`PlayerCommand`), League style: nothing is predicted. Our own fighter is shown as the
//! server last said it was; others are interpolated.
//!
//! The networking part (`ClientNetPlugin`) has no rendering, so integration tests can run it
//! headless as a bot. `render` adds the window, 3D scene and keyboard/mouse input on top.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Duration;

use arena_shared::config::*;
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::netcode::NetcodeClient;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

#[cfg(feature = "render")]
pub mod abilities;

#[cfg(feature = "render")]
pub mod audio;
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
mod ease;
#[cfg(feature = "render")]
pub mod casting;
#[cfg(feature = "render")]
pub mod esc_menu;
#[cfg(feature = "render")]
pub mod settings;
#[cfg(feature = "render")]
pub mod practice;
#[cfg(feature = "render")]
pub mod feedback;
#[cfg(feature = "render")]
pub mod fx;
#[cfg(all(feature = "render", not(target_family = "wasm")))]
pub mod perf;
#[cfg(feature = "render")]
mod item_icons;
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
#[cfg(feature = "render")]
pub mod scoreboard;
#[cfg(feature = "render")]
pub mod prompt;
#[cfg(all(test, feature = "render"))]
mod preview;
#[cfg(feature = "render")]
pub mod sculpt;
#[cfg(feature = "render")]
mod spring;
#[cfg(feature = "render")]
pub mod stat_frame;
#[cfg(feature = "render")]
pub mod swish;
#[cfg(feature = "render")]
pub mod tooltip;
#[cfg(feature = "render")]
pub mod waygates;
#[cfg(feature = "render")]
pub mod waves;

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

/// Sends the server what changed in `DesiredInput` (`send_commands`); after `PlayerControls` and
/// the bot.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SendCommands;

/// The class this client enters the arena as. Sent to the server whenever it's picked in a room.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct ChosenClass(pub Option<ClassId>);

/// What the local player wants to do. Written by keyboard/mouse (render) or by a bot; what
/// changes in it is sent to the server as commands. `move_to` is also where the destination
/// marker is drawn: forgetting it here (arrived, attacking) only hides the marker, since the
/// server forgets it by itself.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct DesiredInput(pub PlayerInput);

/// Set to stop walking (S): sent as `PlayerCommand::Stop`, then cleared.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct StopWalking(pub bool);

/// Extra latency to fake on everything we receive (the practice tools' "Fake ping"), to feel how
/// the game plays at a higher ping: it adds to the round trip, as a farther server would.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct FakeLatency(pub Duration);

/// Our own fighter (the one the server says we control).
#[derive(Component, Debug)]
pub struct Me;

/// Which tick each `PlayerCommand::Walk` we sent was for, and which way, oldest first: what our
/// own fighter's prediction replays (`render::glide`), since the server steps what was sent,
/// from the tick it was sent for.
#[derive(Resource, Default)]
pub struct SentWalks(pub VecDeque<(u32, Vec2)>);

/// How many sent walks are kept for replaying.
const SENT_WALKS_KEPT: usize = 64;

/// Which tick each `PlayerCommand::Draw` we sent was for, and whether the attack button was
/// held, oldest first: what our own fighter's prediction walks slower by (`render::glide`), and
/// whether our shot is held drawn at a tick before the server says so (`rig`).
#[derive(Resource, Default)]
pub struct SentDraws(pub VecDeque<(u32, bool)>);

impl SentDraws {
    /// Whether the attack button was held at `tick`, as sent.
    pub fn held_at(&self, tick: u32) -> bool {
        self.0.iter().rev().find(|(t, _)| *t <= tick).is_some_and(|(_, held)| *held)
    }
}

/// The server tick the next walk we send is for: the newest tick our fighter's prediction steps
/// (`render::glide` sets it each frame); 0 without the action camera, for at once.
#[derive(Resource, Default)]
pub struct WalkSchedule(pub u32);

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
        app.init_resource::<StopWalking>();
        app.init_resource::<FakeLatency>();
        app.insert_resource(ChosenClass(self.settings.class));

        spawn_client_entity(app.world_mut(), &self.settings);
        app.add_systems(Startup, |mut commands: Commands, client: Single<Entity, With<Client>>| {
            commands.trigger(Connect { entity: client.into_inner() });
        });

        app.add_observer(mark_controlled_player);
        app.configure_sets(Update, SendCommands.after(PlayerControls));
        app.init_resource::<WalkWhenOrdered>();
        app.init_resource::<SentWalks>();
        app.init_resource::<SentDraws>();
        app.init_resource::<WalkSchedule>();
        app.add_systems(Update, (forget_walk_on_attack, send_commands).chain().in_set(SendCommands));
        app.add_systems(Update, fake_latency.run_if(resource_changed::<FakeLatency>));
    }
}

/// Delays what we receive by `FakeLatency`, with Lightyear's link conditioner (replacing one from
/// `ClientSettings`). Packets already held back keep their times, so none are lost on a switch.
fn fake_latency(latency: Res<FakeLatency>, mut link: Single<&mut Link, With<Client>>) {
    if latency.is_added() && latency.0.is_zero() {
        return;
    }
    let mut conditioner = RecvLinkConditioner::new(LinkConditionerConfig { incoming_latency: latency.0, ..default() });
    if let Some(old) = link.recv.conditioner.as_mut() {
        conditioner.time_queue = std::mem::take(&mut old.time_queue);
    }
    link.recv.conditioner = Some(conditioner);
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

/// The server marks our own player as `Controlled`: that's `Me`.
fn mark_controlled_player(trigger: On<Add, Controlled>, players: Query<(), With<PlayerId>>, mut commands: Commands) {
    if players.contains(trigger.entity) {
        commands.entity(trigger.entity).insert(Me);
    }
}

/// What `send_commands` last told the server.
#[derive(Default)]
struct Sent {
    move_to: Option<Vec2>,
    walk: Vec2,
    aim: Vec2,
    climb: f32,
    /// Whether Space was held, and the attack button, as last sent.
    jump: bool,
    draw: bool,
    /// When the attack, walk or aim was last sent (`Time<Real>`), to send changes at most once a
    /// tick.
    attack_at: Duration,
    walk_at: Duration,
    aim_at: Duration,
}

/// Sends the server what changed in `DesiredInput`: a new destination, a stop, walking keys, an
/// attack click, a Q press. Clicks go out right away (attacks at most once a tick); changes that
/// come with every mouse or camera move (walking direction) at most once a tick. Nothing is sent
/// while we have no fighter.
fn send_commands(
    time: Res<Time<Real>>,
    mut desired: ResMut<DesiredInput>,
    mut walk_when_ordered: ResMut<WalkWhenOrdered>,
    mut stop: ResMut<StopWalking>,
    me: Query<Option<&AttackState>, With<Me>>,
    mut client: Query<&mut MessageSender<PlayerCommand>, (With<Client>, With<Connected>)>,
    mut sent: Local<Sent>,
    #[cfg(feature = "render")] mode: Option<Res<crate::camera::CameraMode>>,
    (mut sent_walks, mut sent_draws): (ResMut<SentWalks>, ResMut<SentDraws>),
    schedule: Res<WalkSchedule>,
) {
    let (Ok(mut sender), Ok(attack)) = (client.single_mut(), me.single()) else {
        *sent = Sent::default();
        sent_walks.0.clear();
        sent_draws.0.clear();
        // Nothing carries over to our next fighter: not a destination, not a click.
        desired.0 = PlayerInput { aim: desired.0.aim, ..default() };
        stop.0 = false;
        return;
    };
    let mut send = |command| sender.send::<Reliable>(command);
    let input = desired.0;
    let now = time.elapsed();
    let due = |at: Duration| now >= at + TICK_DURATION;
    // Forgotten here (arrived, attacking, respawned) is forgotten on the server too: only a
    // new destination, or S, is news.
    if input.move_to != sent.move_to {
        if let Some(point) = input.move_to {
            send(PlayerCommand::MoveTo(point));
        }
        sent.move_to = input.move_to;
    }
    if std::mem::take(&mut stop.0) {
        send(PlayerCommand::Stop);
    }
    let started_or_stopped = (input.walk == Vec2::ZERO) != (sent.walk == Vec2::ZERO);
    if input.walk != sent.walk && (started_or_stopped || due(sent.walk_at)) {
        send(PlayerCommand::Walk(input.walk, schedule.0));
        (sent.walk, sent.walk_at) = (input.walk, now);
        sent_walks.0.push_back((schedule.0, input.walk));
        while sent_walks.0.len() > SENT_WALKS_KEPT {
            sent_walks.0.pop_front();
        }
    }
    // In action mode the aim is the camera's: its direction and its pitch (`climb`). It's sent
    // as the camera turns while our attack winds up (the only time the server has a use for it),
    // so the attack follows it until it goes off (`PlayerCommand::Aim`); the click that starts
    // one carries the direction itself, and the pitch goes ahead of it if it changed. Out of
    // action mode the pitch is sent level once, so no shot keeps the last one.
    #[cfg(feature = "render")]
    let action = mode.as_ref().is_some_and(|mode| mode.free);
    #[cfg(not(feature = "render"))]
    let action = false;
    let winding_up = attack.is_some_and(|attack| attack.winding_up().is_some());
    let turned = input.aim != sent.aim || input.climb != sent.climb;
    let pitched = input.climb != sent.climb;
    let firing = input.fire && due(sent.attack_at);
    if (action && winding_up && turned && due(sent.aim_at)) || (pitched && (firing || !action)) {
        send(PlayerCommand::Aim(input.aim, input.climb));
        (sent.aim, sent.climb, sent.aim_at) = (input.aim, input.climb, now);
    }
    if firing {
        send(PlayerCommand::Attack(input.aim));
        (sent.aim, sent.attack_at) = (input.aim, now);
        desired.0.fire = false;
        walk_when_ordered.0 = input.move_to;
    }
    if input.casts().next().is_some() {
        for order in input.casts() {
            send(PlayerCommand::Cast(order.slot, order.aim));
        }
        desired.0.casts = default();
        walk_when_ordered.0 = input.move_to;
    }
    if input.jump != sent.jump {
        send(PlayerCommand::Jump(input.jump));
        sent.jump = input.jump;
    }
    // The attack button, for the tick a walk would be (`PlayerCommand::Draw`).
    if input.draw != sent.draw {
        send(PlayerCommand::Draw(input.draw, schedule.0));
        sent.draw = input.draw;
        sent_draws.0.push_back((schedule.0, input.draw));
        while sent_draws.0.len() > SENT_WALKS_KEPT {
            sent_draws.0.pop_front();
        }
    }
}

/// Where we were walking to when we last ordered an attack or an ability: what starting one
/// drops.
#[derive(Resource, Default)]
struct WalkWhenOrdered(Option<Vec2>);

/// Starting an attack or a dash drops the destination (the server drops it by itself): hide its
/// marker. Once per attack or dash (by its start tick), as the server's state shows it, and only
/// the destination we had when we ordered it: one clicked since (to cancel a windup, say) has
/// gone to the server after the order, so it stands, and still needs sending if it hasn't been.
fn forget_walk_on_attack(
    mut desired: ResMut<DesiredInput>,
    walk_when_ordered: Res<WalkWhenOrdered>,
    me: Query<(&AttackState, &Dashing), With<Me>>,
    mut forgotten_for: Local<[Option<u32>; 2]>,
) {
    let Ok((attack, dashing)) = me.single() else { return };
    let started = [attack.swing.map(|w| w.started_at), dashing.0.map(|d| d.started_at)];
    for (started, forgotten) in started.into_iter().zip(forgotten_for.iter_mut()) {
        if started.is_some() && *forgotten != started {
            *forgotten = started;
            if desired.0.move_to == walk_when_ordered.0 {
                desired.0.move_to = None;
            }
        }
    }
}
