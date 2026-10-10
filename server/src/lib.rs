//! Authoritative match server.
//!
//! Guests meet in rooms (`rooms.rs`); each room is its own arena, all in this one world. In a
//! started room, clients pick a class, then only send inputs. The server runs the shared sim on them, decides
//! hits, damage and crowd control (melee, dashes and novas with lag compensation), and
//! replicates the result. A
//! client's own player is predicted on that client, other players are interpolated; every
//! client predicts every projectile.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use arena_shared::classes::{AbilityKind, CRIT_MULTIPLIER, Chill};
use arena_shared::config::*;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::prelude::*;
use lightyear::interpolation::plugin::InterpolationDelay;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::input::native::{ActionState, NativeBuffer, NativeStateSequence};
use lightyear::prelude::server::input::{InputValidationAppExt, authorize_controlled_targets};
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use crate::logging::InputGaps;
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
        // A client may only send inputs for the player it controls. Lightyear doesn't check this
        // by default, so without it a modified client could drive anyone else's fighter.
        app.add_input_validator(authorize_controlled_targets::<NativeStateSequence<PlayerInput>>);
        app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));
        app.add_observer(on_new_link);
        app.add_systems(
            FixedUpdate,
            (
                neutralize_stale_inputs,
                move_players,
                take_pickups,
                record_history,
                attack,
                use_abilities,
                move_projectiles,
                resolve_projectile_hits,
                respawn,
                place_players,
            )
                .chain(),
        );
        // Also every frame, so players who join between fixed ticks are placed before their
        // first replication.
        app.add_systems(Update, place_players.after(rooms::RoomSystems));
    }
}

/// Replicated to everyone; predicted by the owning client, interpolated by the rest.
fn owner_predicted(owner: PeerId) -> impl Bundle {
    (
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::Single(owner)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(owner)),
    )
}

/// Projectiles are predicted by everyone: the shooter matches the server's copy to the one it
/// already threw (`PreSpawned`); the others learn of it late and catch it up to where it really
/// is. Either way it flies on the same clock as each client's own player, which is what makes
/// a dodge look the way the server judges it (see `resolve_projectile_hits`).
fn spawn_projectile(commands: &mut Commands, projectile: Projectile, (room, team): Side) {
    commands.spawn((
        projectile.bundle(),
        room,
        room.rooms(),
        team,
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::All),
    ));
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
const PLACE_BEFORE_RESPAWN_TICKS: u32 = 16;

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

/// A room member's fighter, as `class`, on `team`. Despawned with the client's link when it
/// disconnects; `rooms.rs` despawns it when the member leaves the room.
fn spawn_player(commands: &mut Commands, link: Entity, id: PeerId, class: ClassId, room: InRoom, team: Team) -> Entity {
    commands
        .spawn((
            Name::from("Player"),
            PlayerId(id),
            (class, team, room, room.rooms()),
            Pos::default(),
            NeedsSpawnPoint,
            Health(class.def().max_hp),
            (Chilled::default(), Hasted::default(), RecentHits::default()),
            AttackState::default(),
            AbilityState::default(),
            DashHits::default(),
            LastSwing::default(),
            PosHistory::default(),
            InputGaps::default(),
            ActionState::<PlayerInput>::default(),
            // Replication starts in `place_players`, once it has a real position.
            ControlledBy { owner: link, lifetime: default() },
        ))
        .id()
}

/// Places waiting players one at a time, each away from everyone already placed in their room,
/// so players joining or respawning in the same tick don't land on the same spot.
fn place_players(
    mut commands: Commands,
    mut players: Query<(Entity, &PlayerId, &InRoom, &mut Pos, &mut PosHistory, Has<NeedsSpawnPoint>, Has<Replicate>)>,
) {
    let mut placed: Vec<(InRoom, Vec2)> =
        players.iter().filter(|(.., waiting, _)| !waiting).map(|(_, _, room, pos, ..)| (*room, pos.0)).collect();
    for (entity, id, room, mut pos, mut history, waiting, replicated) in &mut players {
        if !waiting {
            continue;
        }
        pos.0 = sim::pick_spawn_point(placed.iter().filter(|(r, _)| r == room).map(|(_, p)| *p));
        placed.push((*room, pos.0));
        // Hits can't be judged against where the player was before the teleport.
        history.0.clear();
        let mut player = commands.entity(entity);
        player.remove::<NeedsSpawnPoint>();
        // New players start replicating here, so clients never see a placeholder position.
        if !replicated {
            player.insert(owner_predicted(id.0));
        }
    }
}

/// When a player's inputs stop arriving in time, lightyear keeps repeating the last one it got,
/// which papers over short gaps. Past this many ticks we stop trusting it: a frozen or badly
/// hitching client should stand still, not keep running and firing on its last input.
const STALE_INPUT_TICKS: i32 = 8;

fn neutralize_stale_inputs(
    timeline: Res<LocalTimeline>,
    mut players: Query<(&PlayerId, &mut ActionState<PlayerInput>, Option<&NativeBuffer<PlayerInput>>, &mut InputGaps)>,
) {
    let tick = timeline.tick();
    for (id, mut action, buffer, mut gaps) in &mut players {
        let newest = buffer.and_then(|b| b.end_tick());
        let fresh = newest.is_some_and(|newest| tick - newest <= STALE_INPUT_TICKS);
        if !fresh {
            action.set_if_neq(ActionState::default());
        }
        // Not before the first input: a player who just joined hasn't sent any yet.
        if newest.is_some() {
            gaps.update(id.0, fresh, tick.0);
        }
    }
}

fn move_players(
    timeline: Res<LocalTimeline>,
    mut players: Query<(&mut Pos, &ClassId, &ActionState<PlayerInput>, &AttackState, &AbilityState, &Chilled, &Hasted), InPlay>,
) {
    let now = timeline.tick().0;
    for (mut pos, class, input, attack, ability, chilled, hasted) in &mut players {
        pos.set_if_neq(Pos(sim::move_player(pos.0, &input.0, *class, attack, ability, chilled, hasted, now)));
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
    mut players: Query<(&PlayerId, &ClassId, &Pos, &mut Health, &mut Hasted, &mut RecentHits, &InRoom), InPlay>,
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

// --- Lag compensation -------------------------------------------------------------------------
//
// An attacker sees other players slightly in the past (interpolated), so judging their hits
// against where targets are *now* makes clear hits miss at any real ping. Instead the server
// keeps a short position history per player and checks hits against where the attacker saw
// the target: their own interpolation delay back in time ("favor the shooter"), capped. Only
// for melee, dashes and novas, which land the instant they're made; projectiles are judged in
// the present (`resolve_projectile_hits`).

/// How far back a hit may be judged: ~250 ms.
const MAX_REWIND_TICKS: u32 = 16;

/// Recent positions, one per consecutive tick, newest last: (tick, position after that tick's
/// movement). Cleared on teleport.
#[derive(Component, Default)]
struct PosHistory(std::collections::VecDeque<(u32, Vec2)>);

impl PosHistory {
    /// Where this player was at `tick` plus `overstep` (0..1) of the next tick.
    fn at(&self, (tick, overstep): (u32, f32)) -> Option<Vec2> {
        let first = self.0.front()?.0;
        let get = |t: u32| t.checked_sub(first).and_then(|i| self.0.get(i as usize)).map(|(_, p)| *p);
        let now = get(tick)?;
        Some(get(tick + 1).map_or(now, |next| now.lerp(next, overstep)))
    }
}

fn record_history(timeline: Res<LocalTimeline>, mut players: Query<(&Pos, &mut PosHistory)>) {
    let tick = timeline.tick().0;
    for (pos, mut history) in &mut players {
        history.0.push_back((tick, pos.0));
        while history.0.len() > MAX_REWIND_TICKS as usize + 2 {
            history.0.pop_front();
        }
    }
}

/// The (tick, overstep) at which the player behind `link` sees other players.
fn view_time(now: Tick, link: Entity, delays: &Query<&InterpolationDelay, With<ClientOf>>) -> (u32, f32) {
    let (tick, overstep) = delays.get(link).map_or((now, 0.0), |d| d.tick_and_overstep(now));
    let earliest = now.0 - MAX_REWIND_TICKS.min(now.0);
    (tick.0.max(earliest), overstep)
}

/// What a hit does: damage, and crowd control on top.
#[derive(Clone, Copy)]
struct Blow {
    amount: i32,
    chill: Chill,
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
/// `RESPAWN_TICKS`. A crit is rolled on the attacker's class's odds, against whether the target
/// is frozen now: a root this very hit applies starts next tick, so it doesn't count.
fn damage(
    commands: &mut Commands,
    now: u32,
    (player, id): (Entity, PeerId),
    (health, chilled, hits): (&mut Health, &mut Chilled, &mut RecentHits),
    blow: Blow,
    (by, class, with): Attacker,
) {
    let crit = fastrand::f32() < class.def().crit.chance_against(chilled.rooted_at(now));
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
    mut dead: Query<(Entity, &ClassId, &mut Health, (&mut Chilled, &mut Hasted), &mut AttackState, &mut AbilityState, &mut Dead)>,
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
            *ability = AbilityState::default();
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
        &'static PosHistory,
        &'static mut Health,
        &'static mut Chilled,
        &'static mut RecentHits,
    ),
    InPlay,
>;

/// Lag-compensated hits: deals `blow` to every foe of `attacker` (on `side`) that `hits` says
/// is hit, judged at where they were at `seen_at` (where the attacker saw them). Returns how many.
#[expect(clippy::too_many_arguments)]
fn hit_where_seen(
    commands: &mut Commands,
    now: u32,
    targets: &mut Targets,
    seen_at: (u32, f32),
    attacker: Attacker,
    side: Side,
    blow: Blow,
    mut hits: impl FnMut(PeerId, Vec2) -> bool,
) -> usize {
    let mut count = 0;
    for (target, target_id, (room, team), target_pos, history, mut health, mut chilled, mut recent) in targets {
        if target_id.0 != attacker.0 && foes(side, (*room, *team)) && hits(target_id.0, history.at(seen_at).unwrap_or(target_pos.0)) {
            damage(commands, now, (target, target_id.0), (&mut health, &mut chilled, &mut recent), blow, attacker);
            count += 1;
        }
    }
    count
}

/// Auto-attacks, as they go off after their windup: projectiles are spawned (and matched to the
/// client's prespawned copy); melee swings are resolved right here against where the attacker
/// saw everyone.
fn attack(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    delays: Query<&InterpolationDelay, With<ClientOf>>,
    mut attackers: Query<
        (&PlayerId, &ClassId, &Pos, &ActionState<PlayerInput>, &ControlledBy, &mut AttackState, &mut LastSwing, (&InRoom, &Team)),
        InPlay,
    >,
    mut targets: Targets,
) {
    let now = timeline.tick();
    for (id, class, pos, input, controlled_by, mut state, mut last_swing, (room, team)) in &mut attackers {
        let side = (*room, *team);
        let (next, released) = sim::step_attack(now.0, id.0, *class, pos.0, &input.0, *state);
        // Only on change: AttackState is replicated.
        state.set_if_neq(next);
        let Some(attack) = released else { continue };
        match attack {
            sim::Attack::Projectile(projectile) => spawn_projectile(&mut commands, projectile, side),
            sim::Attack::Melee(swing) => {
                *last_swing = swing;
                let seen_at = view_time(now, controlled_by.owner, &delays);
                let blow = Blow { amount: class.def().attack.damage, chill: class.def().attack.chill };
                let with = (id.0, *class, "auto-attack");
                hit_where_seen(&mut commands, now.0, &mut targets, seen_at, with, side, blow, |_, seen| {
                    sim::melee_hits(pos.0, swing.dir, *class, seen)
                });
            }
        }
    }
}

/// Server-only: who the current dash (started at `.0`) has already cut, so each target is cut
/// once per dash.
#[derive(Component, Default)]
struct DashHits(u32, Vec<PeerId>);

/// Q abilities. Thrown ones are spawned (and matched to the client's prespawned copy). Dashes
/// cut whoever the dasher passes through, judged against where the dasher saw them, each target
/// once per dash; with `resets_attack`, a hit readies the auto-attack (the client predicted it
/// still cooling down and is corrected by a rollback). A rooted dasher cuts no one (its dash
/// stops). Novas hit everyone around the caster, judged against where the caster saw them.
fn use_abilities(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    delays: Query<&InterpolationDelay, With<ClientOf>>,
    mut users: Query<
        (
            &PlayerId,
            &ClassId,
            &Pos,
            &ActionState<PlayerInput>,
            &ControlledBy,
            &mut AttackState,
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
    for (id, class, pos, input, controlled_by, mut attack, mut state, mut hits, (room, team)) in &mut users {
        let side = (*room, *team);
        let chilled = chills.iter().find(|(p, _)| *p == id.0).map(|(_, c)| *c).unwrap_or_default();
        // Cut whoever this tick's dash step reached (`move_players` just made it). Checked before
        // the ability steps, which ends the dash on its last tick: so every step is checked, the
        // last one included, and the spot the dash started from isn't.
        if let (Some(dash), AbilityKind::Dash { damage: cut, resets_attack, .. }) = (state.dash, &class.def().ability.kind)
            && !chilled.rooted_at(now.0)
        {
            if hits.0 != dash.started_at {
                hits.0 = dash.started_at;
                hits.1.clear();
            }
            let seen_at = view_time(now, controlled_by.owner, &delays);
            let with = (id.0, *class, class.def().ability.name.as_str());
            let blow = Blow { amount: *cut, chill: Chill::default() };
            let cut_now = hit_where_seen(&mut commands, now.0, &mut targets, seen_at, with, side, blow, |target, seen| {
                let fresh = !hits.1.contains(&target) && sim::dash_hits(pos.0, seen);
                if fresh {
                    hits.1.push(target);
                }
                fresh
            });
            if cut_now > 0 && *resets_attack && attack.windup.is_none() && attack.ready_at > now.0 {
                attack.ready_at = now.0;
            }
        }
        let (next, cast) = sim::step_ability(now.0, id.0, *class, pos.0, &input.0, &attack, &chilled, *state);
        state.set_if_neq(next);
        match cast {
            Some(sim::Cast::Throw(projectile)) => spawn_projectile(&mut commands, projectile, side),
            Some(sim::Cast::Nova) => {
                let AbilityKind::Nova { damage, chill, .. } = class.def().ability.kind else { continue };
                let seen_at = view_time(now, controlled_by.owner, &delays);
                let with = (id.0, *class, class.def().ability.name.as_str());
                let blow = Blow { amount: damage, chill };
                hit_where_seen(&mut commands, now.0, &mut targets, seen_at, with, side, blow, |_, seen| {
                    sim::nova_hits(pos.0, *class, seen)
                });
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

/// Projectile hits on the shooter's foes (allies' pass through), judged against where everyone is now: no lag compensation, unlike melee and
/// dashes. Projectiles can be dodged, and every client draws them on its own player's clock, so
/// what the target sees is exactly what's judged here ("favor the target"). The price is paid by
/// the shooter, who sees others a little in the past and may watch a shot pass through someone
/// who had already stepped aside. Clients see the result through replicated `Health` (and
/// `Chilled`).
fn resolve_projectile_hits(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    projectiles: Query<(Entity, &Pos, &Projectile, &InRoom, &Team)>,
    mut targets: Targets,
) {
    let now = timeline.tick();
    for (projectile_entity, projectile_pos, projectile, room, team) in &projectiles {
        for (target, target_id, (target_room, target_team), target_pos, _, mut health, mut chilled, mut recent) in &mut targets {
            if target_id.0 == projectile.owner
                || !foes((*room, *team), (*target_room, *target_team))
                || !sim::projectile_hits(projectile_pos.0, projectile, target_pos.0) {
                continue;
            }
            commands.entity(projectile_entity).try_despawn();
            let blow = Blow { amount: sim::projectile_damage(projectile, now.0), chill: sim::projectile_chill(projectile) };
            let def = projectile.class.def();
            let with = if projectile.ability { def.ability.name.as_str() } else { "auto-attack" };
            let hit = (&mut *health, &mut *chilled, &mut *recent);
            damage(&mut commands, now.0, (target, target_id.0), hit, blow, (projectile.owner, projectile.class, with));
            break;
        }
    }
}
