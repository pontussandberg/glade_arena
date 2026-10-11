//! Authoritative match server.
//!
//! Guests meet in rooms (`rooms.rs`); each room is its own arena, all in this one world. In a
//! started room, clients pick a class, then only send commands (`PlayerCommand`: walk there,
//! attack, cast), League style: each is applied the moment it arrives. The server runs the shared sim,
//! decides hits, damage and crowd control against where everyone is now, and replicates the
//! result every tick. Nothing is predicted: a client shows its own fighter as the server last
//! said, and others interpolated.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use arena_shared::classes::{AbilityKind, CRIT_MULTIPLIER, Chill};
use arena_shared::config::*;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::prelude::*;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use crate::rooms::InRoom;

pub mod logging;
pub mod rooms;

pub struct ServerSettings {
    pub port: u16,
    pub certificate: Certificate,
}

/// The TLS certificate WebTransport runs on.
pub enum Certificate {
    /// Made up at start, for local dev: valid 14 days, for localhost only. Browsers trust it by
    /// its SHA-256 digest, written to `digest_out` for the dev web page to fetch.
    SelfSigned { digest_out: Option<PathBuf> },
    /// A real certificate (deployed: the one Caddy gets from Let's Encrypt), as PEM files. Waited
    /// for if they're not there yet (Caddy may still be getting it), and when the certificate
    /// file changes (renewed), the server exits to be restarted with the new one: a running
    /// WebTransport server can't swap certificates.
    Pem { cert: PathBuf, key: PathBuf },
}

/// How often to check whether the certificate was renewed.
const CERT_CHECK_EVERY: Duration = Duration::from_secs(600);

/// The certificate file the server runs on, and when it was last modified as loaded.
#[derive(Resource)]
struct CertificateFile {
    path: PathBuf,
    loaded: Option<std::time::SystemTime>,
}

/// A headless server app, ready to `run()` or to be stepped manually with `update()` in tests.
pub fn build_server_app(settings: ServerSettings) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_millis(2))),
        logging::log_plugin(),
        bevy::state::app::StatesPlugin,
    ));
    app.add_plugins(ServerPlugins { tick_duration: TICK_DURATION });
    app.add_plugins(ServerGamePlugin);
    spawn_server_entity(&mut app, settings);
    app
}

fn spawn_server_entity(app: &mut App, settings: ServerSettings) {
    let identity = match settings.certificate {
        Certificate::SelfSigned { digest_out } => self_signed(digest_out),
        Certificate::Pem { cert, key } => {
            let identity = load_pem(&cert, &key);
            app.insert_resource(CertificateFile { loaded: modified(&cert), path: cert });
            app.add_systems(Update, restart_on_renewal);
            identity
        }
    };

    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), settings.port);
    info!(
        port = settings.port,
        tick_hz = TICK_HZ,
        protocol = format!("{PROTOCOL_ID:#018x}"),
        version = env!("CARGO_PKG_VERSION"),
        "server starting"
    );
    app.world_mut().spawn((
        Name::from("Server"),
        Server::new(None),
        NetcodeServer::new(NetcodeConfig {
            protocol_id: PROTOCOL_ID,
            private_key: DEV_PRIVATE_KEY,
            ..default()
        }),
        LocalAddr(addr),
        WebTransportServerIo { certificate: identity },
    ));
    app.add_systems(Startup, |mut commands: Commands, server: Single<Entity, With<Server>>| {
        commands.trigger(Start { entity: server.into_inner() });
    });
}

pub struct ServerGamePlugin;

impl Plugin for ServerGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((ProtocolPlugin, RoomPlugin, logging::LoggingPlugin, rooms::RoomsPlugin));
        app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));
        app.add_observer(on_new_link);
        // As soon as they're received, before this frame's ticks run.
        app.add_systems(PreUpdate, apply_commands.after(MessageSystems::Receive));
        app.add_systems(
            FixedUpdate,
            (
                move_players,
                walk_dummies,
                take_pickups,
                attack,
                use_abilities,
                move_projectiles,
                resolve_projectile_hits,
                restore_dummies,
                respawn,
                place_players,
                release_held_taps,
            )
                .chain(),
        );
        // Also every frame, so players who join between fixed ticks are placed before their
        // first replication.
        app.add_systems(Update, place_players.after(rooms::RoomSystems));
    }
}

/// Replicated to everyone; interpolated by all but the owner, who shows it as last received.
fn replicated_player(owner: PeerId) -> impl Bundle {
    (
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(owner)),
    )
}

/// Projectiles are only replicated: clients draw them from where and when they were thrown.
fn spawn_projectile(commands: &mut Commands, projectile: Projectile, (room, team): Side) {
    commands.spawn((projectile.bundle(), room, room.rooms(), team, Replicate::to_clients(NetworkTarget::All)));
}

/// Server-only: this player (new or respawning) still needs a spawn point.
#[derive(Component)]
struct NeedsSpawnPoint;

/// Server-only: killed; out of the fight until `respawn_at` (its `Health` is 0 meanwhile, which
/// is what clients see).
#[derive(Component)]
struct Dead {
    respawn_at: u32,
    placed: bool,
}

/// A respawning player is moved to its spawn point this long before it comes back to life:
/// longer than other clients' interpolation delay, so they never see it slide from where it
/// died to where it respawns (it's still hidden while that happens).
const PLACE_BEFORE_RESPAWN_TICKS: u32 = TICK_HZ as u32 / 4;

/// Players who can move, attack and be hit: placed and alive.
type InPlay = (Without<NeedsSpawnPoint>, Without<Dead>);

fn self_signed(digest_out: Option<PathBuf>) -> Identity {
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

fn load_pem(cert: &std::path::Path, key: &std::path::Path) -> Identity {
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

fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Exits when the certificate file changes, to be restarted with it (see `Certificate::Pem`).
fn restart_on_renewal(
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

/// Every new link needs a `ReplicationSender` before we can replicate anything to it.
fn on_new_link(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(ReplicationSender);
}

/// What every fighter is, player or dummy: who, of which class, where and on which side, and
/// its fight state (health, crowd control, attack and ability), fresh.
fn fighter(id: PeerId, class: ClassId, room: InRoom, team: Team, at: Vec2) -> impl Bundle {
    (
        PlayerId(id),
        (class, team, room, room.rooms()),
        Pos(at),
        Health(class.def().max_hp),
        (Chilled::default(), Hasted::default(), RecentHits::default()),
        (AttackState::default(), AbilityState::default(), LastSwing::default()),
    )
}

/// A room member's fighter, as `class`, on `team`. Despawned with the client's link when it
/// disconnects; `rooms.rs` despawns it when the member leaves the room.
fn spawn_player(commands: &mut Commands, link: Entity, id: PeerId, class: ClassId, room: InRoom, team: Team) -> Entity {
    commands
        .spawn((
            Name::from("Player"),
            fighter(id, class, room, team, Vec2::ZERO),
            NeedsSpawnPoint,
            DashHits::default(),
            PlayerInput::default(),
            Tap::default(),
            // Replication starts in `place_players`, once it has a real position.
            ControlledBy { owner: link, lifetime: default() },
        ))
        .id()
}

/// Server-only: a target dummy, for practice: a fighter of `class` standing at a spot, never
/// moving or attacking, hit like anyone else, and back to full health instead of dying.
#[derive(Component)]
pub(crate) struct Dummy;

/// A target dummy of `class` standing `at` (a walkable spot) in `room`, as `id` (none of the
/// clients'). Replicated straight away: it already stands where it'll stay.
pub(crate) fn spawn_dummy(commands: &mut Commands, id: PeerId, class: ClassId, room: InRoom, at: Vec2) -> Entity {
    commands
        .spawn((
            Name::from("Dummy"),
            Dummy,
            fighter(id, class, room, Team(arena_shared::rooms::NO_TEAM), at),
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ))
        .id()
}

/// Server-only: a practice dummy that walks back and forth between `ends`, toward `ends[toward]`,
/// at its class's walking speed (slowed, rooted and hasted like anyone).
#[derive(Component)]
pub(crate) struct Patrol {
    ends: [Vec2; 2],
    toward: usize,
}

impl Patrol {
    /// How far a walking dummy goes each way from where it was placed, at most.
    const REACH: f32 = 3.0;

    /// The longest clear straight line through `at`, along x or y (`REACH` each way, shortened
    /// at walls), if it's long enough to be worth walking.
    pub(crate) fn through(at: Vec2) -> Option<Patrol> {
        let map = arena_shared::map::map();
        let end = |dir: Vec2| {
            (1..=(Self::REACH * 4.0) as i32)
                .map(|step| at + dir * step as f32 * 0.25)
                .take_while(|p| map.line_walkable(at, *p))
                .last()
                .unwrap_or(at)
        };
        let ends = [Vec2::X, Vec2::Y].map(|dir| [end(-dir), end(dir)]);
        let ends = ends.into_iter().max_by(|a, b| a[0].distance(a[1]).total_cmp(&b[0].distance(b[1])))?;
        (ends[0].distance(ends[1]) >= 1.5).then_some(Patrol { ends, toward: 1 })
    }
}

/// Walking dummies take a step toward the end they're heading for, and turn around there. Not
/// while dead (they're back at once, `restore_dummies`).
fn walk_dummies(timeline: Res<LocalTimeline>, mut dummies: Query<(&mut Pos, &ClassId, &mut Patrol, &Chilled, &Hasted), InPlay>) {
    let now = timeline.tick().0;
    for (mut pos, class, mut patrol, chilled, hasted) in &mut dummies {
        if chilled.rooted_at(now) {
            continue;
        }
        let step = class.def().move_speed * chilled.speed_factor(now) * hasted.speed_factor(now) * TICK_DT;
        let target = patrol.ends[patrol.toward];
        let next = pos.0.move_towards(target, step);
        if next == target {
            patrol.toward = 1 - patrol.toward;
        }
        pos.set_if_neq(Pos(next));
    }
}

/// A dummy that would die is back on the spot, fresh as a respawned player (full health, its
/// slows, roots and haste gone); its hits still show.
fn restore_dummies(
    mut commands: Commands,
    mut dummies: Query<(Entity, &ClassId, &mut Health, (&mut Chilled, &mut Hasted), &mut AttackState, &mut AbilityState), (With<Dummy>, With<Dead>)>,
) {
    for (dummy, class, mut health, (mut chilled, mut hasted), mut attack, mut ability) in &mut dummies {
        health.0 = class.def().max_hp;
        chilled.set_if_neq(Chilled::default());
        hasted.set_if_neq(Hasted::default());
        attack.set_if_neq(AttackState::default());
        ability.set_if_neq(AbilityState::default());
        commands.entity(dummy).remove::<Dead>();
    }
}

/// Places waiting players one at a time, each away from everyone already placed in their room,
/// so players joining or respawning in the same tick don't land on the same spot.
fn place_players(
    mut commands: Commands,
    mut players: Query<(Entity, &PlayerId, &InRoom, &mut Pos, Option<&mut PlayerInput>, Has<NeedsSpawnPoint>, Has<Replicate>)>,
) {
    let mut placed: Vec<(InRoom, Vec2)> =
        players.iter().filter(|(.., waiting, _)| !waiting).map(|(_, _, room, pos, ..)| (*room, pos.0)).collect();
    for (entity, id, room, mut pos, input, waiting, replicated) in &mut players {
        if !waiting {
            continue;
        }
        pos.0 = sim::pick_spawn_point(placed.iter().filter(|(r, _)| r == room).map(|(_, p)| *p));
        placed.push((*room, pos.0));
        // Wherever it was walking to before, it doesn't walk there from here.
        if let Some(mut input) = input {
            input.move_to = None;
        }
        let mut player = commands.entity(entity);
        player.remove::<NeedsSpawnPoint>();
        // New players start replicating here, so clients never see a placeholder position.
        if !replicated {
            player.insert(replicated_player(id.0));
        }
    }
}

/// Server-only: an attack press that no tick has seen yet (`pressed`), and its release, held
/// back (`release`) until one has, so a click shorter than a tick still attacks.
#[derive(Component, Default)]
struct Tap {
    pressed: bool,
    release: bool,
}

/// Each client's commands change its own fighter's intent, the moment they arrive. A link only
/// ever drives the fighter it controls; commands while it has none (still picking a class) are
/// dropped.
fn apply_commands(
    mut links: Query<(Entity, &mut MessageReceiver<PlayerCommand>), With<ClientOf>>,
    mut players: Query<(&ControlledBy, &mut PlayerInput, &mut Tap)>,
) {
    for (link, mut receiver) in &mut links {
        let mut player = players.iter_mut().find(|(controlled_by, ..)| controlled_by.owner == link);
        for command in receiver.receive() {
            let Some((_, input, tap)) = &mut player else { continue };
            match command {
                PlayerCommand::Attack { aim, fire: true } => {
                    tap.pressed = true;
                    tap.release = false;
                    PlayerCommand::Attack { aim, fire: true }.apply(input);
                }
                PlayerCommand::Attack { aim, fire: false } if tap.pressed => {
                    tap.release = true;
                    input.aim = aim;
                }
                command => command.apply(input),
            }
        }
    }
}

/// After a tick: an attack press has been seen, so a release held back for it goes through.
fn release_held_taps(mut players: Query<(&mut PlayerInput, &mut Tap)>) {
    for (mut input, mut tap) in &mut players {
        if std::mem::take(&mut tap.release) {
            input.fire = false;
        }
        tap.pressed = false;
    }
}

/// Walking (or dashing). Arriving forgets the destination, so nothing lingers.
fn move_players(
    timeline: Res<LocalTimeline>,
    mut players: Query<(&mut Pos, &ClassId, &mut PlayerInput, &AttackState, &mut AbilityState, &Chilled, &Hasted), InPlay>,
) {
    let now = timeline.tick().0;
    for (mut pos, class, mut input, attack, mut ability, chilled, hasted) in &mut players {
        let mut next = *ability;
        pos.set_if_neq(Pos(sim::move_player(pos.0, &input, *class, attack, &mut next, chilled, hasted, now)));
        ability.set_if_neq(next);
        if input.move_to.is_some_and(|point| sim::arrived(pos.0, point)) {
            input.move_to = None;
        }
    }
}

/// A room's pickups, lying at their spots for everyone in it to see.
fn spawn_pickups(commands: &mut Commands, room: InRoom) -> Vec<Entity> {
    arena_shared::map::PICKUP_SPOTS
        .into_iter()
        .map(|(at, kind)| {
            let pickup = Pickup { kind, at, back_at: None, taken_by: None };
            commands.spawn((Name::from("Pickup"), pickup, room, room.rooms(), Replicate::to_clients(NetworkTarget::All))).id()
        })
        .collect()
}

/// A fighter touching a pickup that's lying there takes it (the first found, if several), and
/// it's gone for `PICKUP_RESPAWN_TICKS`. A heal is taken even at full health; what it restores
/// goes in `RecentHits` (as a heal), for the client's number. A haste starts next tick.
fn take_pickups(
    timeline: Res<LocalTimeline>,
    mut pickups: Query<(&mut Pickup, &InRoom)>,
    // Not dummies: one standing on a pickup's spot would take it every time it came back.
    mut players: Query<(&PlayerId, &ClassId, &Pos, &mut Health, &mut Hasted, &mut RecentHits, &InRoom), (InPlay, Without<Dummy>)>,
) {
    let now = timeline.tick().0;
    for (mut pickup, room) in &mut pickups {
        match pickup.back_at {
            Some(back_at) if now < back_at => continue,
            Some(_) => pickup.back_at = None,
            None => {}
        }
        let at = pickup.at;
        let Some((id, class, _, mut health, mut hasted, mut hits, _)) =
            players.iter_mut().find(|(_, _, pos, .., in_room)| *in_room == room && sim::touches_pickup(pos.0, at))
        else {
            continue;
        };
        match pickup.kind {
            PickupKind::Heal => {
                let healed = (health.0 + sim::heal_amount(class.def().max_hp)).min(class.def().max_hp);
                if healed > health.0 {
                    hits.push(healed - health.0, HitKind::Heal);
                    health.0 = healed;
                }
            }
            PickupKind::Haste => hasted.start(now),
        }
        pickup.back_at = Some(now + PICKUP_RESPAWN_TICKS);
        pickup.taken_by = Some(id.0);
        debug!(player = ?id.0, kind = ?pickup.kind, health = health.0, "took a pickup");
    }
}

/// What a hit does: damage, and crowd control on top. Only an auto-attack's `can_crit`.
#[derive(Clone, Copy)]
struct Blow {
    amount: i32,
    chill: Chill,
    can_crit: bool,
}

/// Who dealt a hit, of which class (whose crit odds it rolls), and `with` what (for the log).
type Attacker<'a> = (PeerId, ClassId, &'a str);

/// Where a fighter (or a projectile) fights: which room, which side.
type Side = (InRoom, Team);

/// Whether something on `side` can hurt someone on `other`: same room, not allies.
fn foes(side: Side, other: Side) -> bool {
    side.0 == other.0 && !side.1.allied(other.1)
}

/// Apply a hit dealt by `attacker`; at zero health the player is out of the fight for
/// `RESPAWN_TICKS`. A blow that can crit rolls on the attacker's class's odds, against whether the
/// target is frozen now: a root this very hit applies starts next tick, so it doesn't count.
fn damage(
    commands: &mut Commands,
    now: u32,
    (player, id): (Entity, PeerId),
    (health, chilled, hits): (&mut Health, &mut Chilled, &mut RecentHits),
    blow: Blow,
    (by, class, with): Attacker,
) {
    let crit = blow.can_crit && fastrand::f32() < class.def().crit.chance_against(chilled.rooted_at(now));
    let amount = if crit { (blow.amount as f32 * CRIT_MULTIPLIER).round() as i32 } else { blow.amount };
    health.0 = (health.0 - amount).max(0);
    hits.push(amount, if crit { HitKind::Crit } else { HitKind::Damage });
    if !blow.chill.is_none() {
        chilled.apply(blow.chill, now);
    }
    debug!(attacker = ?by, target = ?id, with, amount, crit, health = health.0, "hit");
    if health.0 == 0 {
        info!(killer = ?by, victim = ?id, with, "kill");
        commands.entity(player).insert(Dead { respawn_at: now + RESPAWN_TICKS, placed: false });
    }
}

/// Respawning: first moved to a spawn point while still hidden, then back at full health.
fn respawn(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    // Dummies come back on the spot instead (`restore_dummies`).
    mut dead: Query<(Entity, &ClassId, &mut Health, (&mut Chilled, &mut Hasted), &mut AttackState, &mut AbilityState, &mut Dead), Without<Dummy>>,
) {
    let now = timeline.tick().0;
    for (player, class, mut health, (mut chilled, mut hasted), mut attack, mut ability, mut dead) in &mut dead {
        if !dead.placed && now + PLACE_BEFORE_RESPAWN_TICKS >= dead.respawn_at {
            dead.placed = true;
            commands.entity(player).insert(NeedsSpawnPoint);
        }
        if now >= dead.respawn_at {
            health.0 = class.def().max_hp;
            chilled.set_if_neq(Chilled::default());
            hasted.set_if_neq(Hasted::default());
            *attack = AttackState::default();
            // Practice's No cooldowns stays as it was.
            *ability = AbilityState { free: ability.free, ..default() };
            commands.entity(player).remove::<Dead>();
            debug!(?player, "respawned");
        }
    }
}

type Targets<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        (&'static InRoom, &'static Team),
        &'static Pos,
        &'static mut Health,
        &'static mut Chilled,
        &'static mut RecentHits,
    ),
    InPlay,
>;

/// Deals `blow` to every foe of `attacker` (on `side`) that `hits` says is hit, judged against
/// where they are now (no lag compensation, as in League: what the server sees is what counts).
/// Returns how many.
fn hit_foes(
    commands: &mut Commands,
    now: u32,
    targets: &mut Targets,
    attacker: Attacker,
    side: Side,
    blow: Blow,
    mut hits: impl FnMut(PeerId, Vec2) -> bool,
) -> usize {
    let mut count = 0;
    for (target, target_id, (room, team), target_pos, mut health, mut chilled, mut recent) in targets {
        if target_id.0 != attacker.0 && foes(side, (*room, *team)) && hits(target_id.0, target_pos.0) {
            damage(commands, now, (target, target_id.0), (&mut health, &mut chilled, &mut recent), blow, attacker);
            count += 1;
        }
    }
    count
}

/// Auto-attacks, as they go off after their windup: projectiles are spawned; melee swings are
/// resolved right here. Starting a windup drops the destination: afterwards the fighter stands
/// still until its player clicks somewhere new.
fn attack(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut attackers: Query<
        (&PlayerId, &ClassId, &Pos, &mut PlayerInput, &mut AttackState, &mut LastSwing, (&InRoom, &Team)),
        InPlay,
    >,
    mut targets: Targets,
) {
    let now = timeline.tick();
    for (id, class, pos, mut input, mut state, mut last_swing, (room, team)) in &mut attackers {
        let side = (*room, *team);
        // In two halves (`sim::step_attack`): a swing that lands arms a quick windup in between.
        let (mut next, released) = sim::release_attack(now.0, id.0, *class, pos.0, *state);
        match released {
            None => {}
            Some(sim::Attack::Projectile(projectile)) => spawn_projectile(&mut commands, projectile, side),
            Some(sim::Attack::Melee(swing)) => {
                *last_swing = swing;
                let blow = Blow { amount: class.def().attack.damage, chill: class.def().attack.chill, can_crit: true };
                let with = (id.0, *class, "auto-attack");
                let landed = hit_foes(&mut commands, now.0, &mut targets, with, side, blow, |_, at| {
                    sim::melee_hits(pos.0, swing.dir, *class, at)
                });
                if landed > 0
                    && let Some(follow_up) = class.def().attack.follow_up
                {
                    next.quick_until = Some(now.0 + follow_up.within_ticks);
                }
            }
        }
        let next = sim::start_attack(now.0, &input, next);
        if next.windup.is_some_and(|w| w.started_at == now.0) {
            input.move_to = None;
        }
        // Only on change: AttackState is replicated.
        state.set_if_neq(next);
    }
}

/// Server-only: who the current dash (started at `.0`) has already cut, so each target is cut
/// once per dash.
#[derive(Component, Default)]
struct DashHits(u32, Vec<PeerId>);

/// Q abilities. Thrown ones are spawned. Dashes cut whoever the dasher passes through, each
/// target once per dash; a rooted dasher cuts no one (its dash stops). Novas hit everyone around
/// the caster. A Q press is used up here, whether or not the ability was ready (as in League);
/// a dash drops the destination like an attack does.
fn use_abilities(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut users: Query<
        (
            &PlayerId,
            &ClassId,
            &Pos,
            &mut PlayerInput,
            &mut AbilityState,
            &mut DashHits,
            (&InRoom, &Team),
        ),
        InPlay,
    >,
    mut targets: Targets,
) {
    let now = timeline.tick();
    // Read up front: `targets` holds every player's `Chilled` mutably. A root or slow landing
    // during this loop starts next tick anyway.
    let chills: Vec<(PeerId, Chilled)> = targets.iter().map(|(_, id, .., chilled, _)| (id.0, *chilled)).collect();
    for (id, class, pos, mut input, mut state, mut hits, (room, team)) in &mut users {
        let side = (*room, *team);
        let chilled = chills.iter().find(|(p, _)| *p == id.0).map(|(_, c)| *c).unwrap_or_default();
        // Cut whoever this tick's dash step reached (`move_players` just made it). Checked before
        // the ability steps, which ends the dash on its last tick: so every step is checked, the
        // last one included, and the spot the dash started from isn't.
        if let (Some(dash), AbilityKind::Dash { damage: cut, .. }) = (state.dash, &class.def().ability.kind)
            && !chilled.rooted_at(now.0)
        {
            if hits.0 != dash.started_at {
                hits.0 = dash.started_at;
                hits.1.clear();
            }
            let with = (id.0, *class, class.def().ability.name.as_str());
            let blow = Blow { amount: *cut, chill: Chill::default(), can_crit: false };
            hit_foes(&mut commands, now.0, &mut targets, with, side, blow, |target, at| {
                let fresh = !hits.1.contains(&target) && sim::dash_hits(pos.0, at);
                if fresh {
                    hits.1.push(target);
                }
                fresh
            });
        }
        let (next, cast) = sim::step_ability(now.0, id.0, *class, pos.0, &input, &chilled, *state);
        input.ability = false;
        if next.dash.is_some_and(|d| d.started_at == now.0) {
            input.move_to = None;
        }
        state.set_if_neq(next);
        match cast {
            Some(sim::Cast::Throw(projectile)) => spawn_projectile(&mut commands, projectile, side),
            Some(sim::Cast::Nova) => {
                let AbilityKind::Nova { damage, chill, .. } = class.def().ability.kind else { continue };
                let with = (id.0, *class, class.def().ability.name.as_str());
                let blow = Blow { amount: damage, chill, can_crit: false };
                hit_foes(&mut commands, now.0, &mut targets, with, side, blow, |_, at| sim::nova_hits(pos.0, *class, at));
            }
            None => {}
        }
    }
}

fn move_projectiles(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut projectiles: Query<(Entity, &mut Pos, &Projectile)>,
) {
    let tick = timeline.tick().0;
    for (entity, mut pos, projectile) in &mut projectiles {
        pos.0 = sim::projectile_pos(projectile, tick as f32);
        if sim::projectile_expired(pos.0, projectile, tick) {
            commands.entity(entity).despawn();
        }
    }
}

/// Projectile hits on the shooter's foes (allies' pass through), judged against where everyone
/// is now, like every hit. Clients see the result through replicated `Health` (and `Chilled`).
fn resolve_projectile_hits(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    projectiles: Query<(Entity, &Pos, &Projectile, &InRoom, &Team)>,
    mut targets: Targets,
) {
    let now = timeline.tick();
    for (projectile_entity, projectile_pos, projectile, room, team) in &projectiles {
        for (target, target_id, (target_room, target_team), target_pos, mut health, mut chilled, mut recent) in &mut targets {
            if target_id.0 == projectile.owner
                || !foes((*room, *team), (*target_room, *target_team))
                || !sim::projectile_hits(projectile_pos.0, projectile, target_pos.0) {
                continue;
            }
            commands.entity(projectile_entity).try_despawn();
            let blow = Blow {
                amount: sim::projectile_damage(projectile, now.0),
                chill: sim::projectile_chill(projectile),
                can_crit: !projectile.ability,
            };
            let def = projectile.class.def();
            let with = if projectile.ability { def.ability.name.as_str() } else { "auto-attack" };
            let hit = (&mut *health, &mut *chilled, &mut *recent);
            damage(&mut commands, now.0, (target, target_id.0), hit, blow, (projectile.owner, projectile.class, with));
            break;
        }
    }
}
