//! Authoritative match server.
//!
//! Clients pick a class, then only send inputs. The server runs the shared sim on them, decides
//! hits and damage (melee and dashes with lag compensation), and replicates the result. A
//! client's own player is predicted on that client, other players are interpolated; every
//! client predicts every projectile.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use arena_shared::classes::AbilityKind;
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

pub struct ServerSettings {
    pub port: u16,
    /// Where to write the self-signed certificate's SHA-256 digest. Browsers need it to trust the
    /// WebTransport connection, so the dev web page fetches it from here.
    pub digest_out: Option<PathBuf>,
}

/// A headless server app, ready to `run()` or to be stepped manually with `update()` in tests.
pub fn build_server_app(settings: ServerSettings) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_millis(2))),
        bevy::log::LogPlugin::default(),
        bevy::state::app::StatesPlugin,
    ));
    app.add_plugins(ServerPlugins { tick_duration: TICK_DURATION });
    app.add_plugins(ServerGamePlugin);
    spawn_server_entity(&mut app, settings);
    app
}

fn spawn_server_entity(app: &mut App, settings: ServerSettings) {
    let identity = Identity::self_signed(["localhost", "127.0.0.1", "::1"])
        .expect("failed to generate self-signed certificate");
    let digest: String = identity.certificate_chain().as_slice()[0]
        .hash()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    info!("WebTransport certificate digest: {digest}");
    if let Some(path) = &settings.digest_out {
        std::fs::write(path, &digest).expect("failed to write certificate digest");
        info!("Wrote certificate digest to {}", path.display());
    }

    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), settings.port);
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
        app.add_plugins(ProtocolPlugin);
        // A client may only send inputs for the player it controls. Lightyear doesn't check this
        // by default, so without it a modified client could drive anyone else's fighter.
        app.add_input_validator(authorize_controlled_targets::<NativeStateSequence<PlayerInput>>);
        app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));
        app.add_observer(on_new_link);
        app.add_systems(Update, spawn_chosen_classes);
        app.add_systems(
            FixedUpdate,
            (
                neutralize_stale_inputs,
                move_players,
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
        app.add_systems(Update, place_players.after(spawn_chosen_classes));
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
fn spawn_projectile(commands: &mut Commands, projectile: Projectile) {
    commands.spawn((
        projectile.bundle(),
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

/// Every new link needs a `ReplicationSender` before we can replicate anything to it.
fn on_new_link(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(ReplicationSender);
}

/// A connected client picked a class on its join screen: spawn its player. Later picks from the
/// same client are ignored (switching classes means rejoining, for now).
fn spawn_chosen_classes(
    mut links: Query<(Entity, &RemoteId, &mut MessageReceiver<ChooseClass>), With<ClientOf>>,
    players: Query<&ControlledBy, With<PlayerId>>,
    mut commands: Commands,
) {
    for (link, remote, mut receiver) in &mut links {
        let mut has_player = players.iter().any(|c| c.owner == link);
        for ChooseClass(class) in receiver.receive() {
            let Some(class) = class.checked() else { continue };
            if has_player {
                continue;
            }
            has_player = true;
            let id = remote.0;
            info!("Client {id:?} joined as {}", class.def().name);
            commands.spawn((
                Name::from("Player"),
                PlayerId(id),
                class,
                Pos::default(),
                NeedsSpawnPoint,
                Health(class.def().max_hp),
                AttackState::default(),
                AbilityState::default(),
                DashHits::default(),
                LastSwing::default(),
                PosHistory::default(),
                ActionState::<PlayerInput>::default(),
                // Replication starts in `place_players`, once it has a real position.
                // Despawned automatically when this client disconnects.
                ControlledBy { owner: link, lifetime: default() },
            ));
        }
    }
}

/// Places waiting players one at a time, each away from everyone already placed, so players
/// joining or respawning in the same tick don't land on the same spot.
fn place_players(
    mut commands: Commands,
    mut players: Query<(Entity, &PlayerId, &mut Pos, &mut PosHistory, Has<NeedsSpawnPoint>, Has<Replicate>)>,
) {
    let mut placed: Vec<Vec2> =
        players.iter().filter(|(.., waiting, _)| !waiting).map(|(_, _, pos, ..)| pos.0).collect();
    for (entity, id, mut pos, mut history, waiting, replicated) in &mut players {
        if !waiting {
            continue;
        }
        pos.0 = sim::pick_spawn_point(placed.iter().copied());
        placed.push(pos.0);
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
    mut players: Query<(&mut ActionState<PlayerInput>, Option<&NativeBuffer<PlayerInput>>)>,
) {
    let tick = timeline.tick();
    for (mut action, buffer) in &mut players {
        let fresh = buffer
            .and_then(|b| b.end_tick())
            .is_some_and(|newest| tick - newest <= STALE_INPUT_TICKS);
        if !fresh {
            action.set_if_neq(ActionState::default());
        }
    }
}

fn move_players(
    mut players: Query<(&mut Pos, &ClassId, &ActionState<PlayerInput>, &AttackState, &AbilityState), InPlay>,
) {
    for (mut pos, class, input, attack, ability) in &mut players {
        pos.set_if_neq(Pos(sim::move_player(pos.0, &input.0, *class, attack, ability)));
    }
}

// --- Lag compensation -------------------------------------------------------------------------
//
// An attacker sees other players slightly in the past (interpolated), so judging their hits
// against where targets are *now* makes clear hits miss at any real ping. Instead the server
// keeps a short position history per player and checks hits against where the attacker saw
// the target: their own interpolation delay back in time ("favor the shooter"), capped. Only
// for melee and dashes, which land the instant they're made; projectiles are judged in the
// present (`resolve_projectile_hits`).

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
    let tick = timeline.tick().0 as u32;
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
    let earliest = now.0 as u32 - MAX_REWIND_TICKS.min(now.0 as u32);
    ((tick.0 as u32).max(earliest), overstep)
}

/// Apply damage; at zero health the player is out of the fight for `RESPAWN_TICKS`.
fn damage(commands: &mut Commands, now: u32, (player, id): (Entity, PeerId), health: &mut Health, amount: i32, by: PeerId) {
    health.0 = (health.0 - amount).max(0);
    info!("{by:?} hit {id:?} for {amount}, health now {}", health.0);
    if health.0 == 0 {
        info!("{id:?} died");
        commands.entity(player).insert(Dead { respawn_at: now + RESPAWN_TICKS, placed: false });
    }
}

/// Respawning: first moved to a spawn point while still hidden, then back at full health.
fn respawn(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut dead: Query<(Entity, &ClassId, &mut Health, &mut AttackState, &mut AbilityState, &mut Dead)>,
) {
    let now = timeline.tick().0 as u32;
    for (player, class, mut health, mut attack, mut ability, mut dead) in &mut dead {
        if !dead.placed && now + PLACE_BEFORE_RESPAWN_TICKS >= dead.respawn_at {
            dead.placed = true;
            commands.entity(player).insert(NeedsSpawnPoint);
        }
        if now >= dead.respawn_at {
            health.0 = class.def().max_hp;
            *attack = AttackState::default();
            *ability = AbilityState::default();
            commands.entity(player).remove::<Dead>();
        }
    }
}

type Targets<'w, 's> =
    Query<'w, 's, (Entity, &'static PlayerId, &'static Pos, &'static PosHistory, &'static mut Health), InPlay>;

/// Lag-compensated hits: deals `amount` to everyone but `attacker` that `hits` says is hit,
/// judged at where they were at `seen_at` (where the attacker saw them). Returns how many.
fn hit_where_seen(
    commands: &mut Commands,
    now: u32,
    targets: &mut Targets,
    seen_at: (u32, f32),
    attacker: PeerId,
    amount: i32,
    mut hits: impl FnMut(PeerId, Vec2) -> bool,
) -> usize {
    let mut count = 0;
    for (target, target_id, target_pos, history, mut health) in targets {
        if target_id.0 != attacker && hits(target_id.0, history.at(seen_at).unwrap_or(target_pos.0)) {
            damage(commands, now, (target, target_id.0), &mut health, amount, attacker);
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
        (&PlayerId, &ClassId, &Pos, &ActionState<PlayerInput>, &ControlledBy, &mut AttackState, &mut LastSwing),
        InPlay,
    >,
    mut targets: Targets,
) {
    let now = timeline.tick();
    for (id, class, pos, input, controlled_by, mut state, mut last_swing) in &mut attackers {
        let (next, released) = sim::step_attack(now.0 as u32, id.0, *class, pos.0, &input.0, *state);
        // Only on change: AttackState is replicated.
        state.set_if_neq(next);
        let Some(attack) = released else { continue };
        match attack {
            sim::Attack::Projectile(projectile) => spawn_projectile(&mut commands, projectile),
            sim::Attack::Melee(swing) => {
                *last_swing = swing;
                let seen_at = view_time(now, controlled_by.owner, &delays);
                let amount = class.def().attack.damage;
                hit_where_seen(&mut commands, now.0 as u32, &mut targets, seen_at, id.0, amount, |_, seen| {
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
/// still cooling down and is corrected by a rollback).
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
        ),
        InPlay,
    >,
    mut targets: Targets,
) {
    let now = timeline.tick();
    for (id, class, pos, input, controlled_by, mut attack, mut state, mut hits) in &mut users {
        // Cut whoever this tick's dash step reached (`move_players` just made it). Checked before
        // the ability steps, which ends the dash on its last tick: so every step is checked, the
        // last one included, and the spot the dash started from isn't.
        if let (Some(dash), AbilityKind::Dash { damage: cut, resets_attack, .. }) = (state.dash, &class.def().ability.kind) {
            if hits.0 != dash.started_at {
                hits.0 = dash.started_at;
                hits.1.clear();
            }
            let seen_at = view_time(now, controlled_by.owner, &delays);
            let cut_now = hit_where_seen(&mut commands, now.0 as u32, &mut targets, seen_at, id.0, *cut, |target, seen| {
                let fresh = !hits.1.contains(&target) && sim::dash_hits(pos.0, seen);
                if fresh {
                    hits.1.push(target);
                }
                fresh
            });
            if cut_now > 0 && *resets_attack && attack.windup.is_none() && attack.ready_at > now.0 as u32 {
                attack.ready_at = now.0 as u32;
            }
        }
        let (next, thrown) = sim::step_ability(now.0 as u32, id.0, *class, pos.0, &input.0, &attack, *state);
        state.set_if_neq(next);
        if let Some(projectile) = thrown {
            spawn_projectile(&mut commands, projectile);
        }
    }
}

fn move_projectiles(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut projectiles: Query<(Entity, &mut Pos, &Projectile)>,
) {
    let tick = timeline.tick().0 as u32;
    for (entity, mut pos, projectile) in &mut projectiles {
        pos.0 = sim::projectile_pos(projectile, tick as f32);
        if sim::projectile_expired(pos.0, projectile, tick) {
            commands.entity(entity).despawn();
        }
    }
}

/// Projectile hits, judged against where everyone is now: no lag compensation, unlike melee and
/// dashes. Projectiles can be dodged, and every client draws them on its own player's clock, so
/// what the target sees is exactly what's judged here ("favor the target"). The price is paid by
/// the shooter, who sees others a little in the past and may watch a shot pass through someone
/// who had already stepped aside. Clients see the result through replicated `Health`.
fn resolve_projectile_hits(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    projectiles: Query<(Entity, &Pos, &Projectile)>,
    mut targets: Targets,
) {
    let now = timeline.tick();
    for (projectile_entity, projectile_pos, projectile) in &projectiles {
        for (target, target_id, target_pos, _, mut health) in &mut targets {
            if target_id.0 == projectile.owner || !sim::projectile_hits(projectile_pos.0, projectile, target_pos.0) {
                continue;
            }
            commands.entity(projectile_entity).try_despawn();
            let amount = sim::projectile_damage(projectile, now.0 as u32);
            damage(&mut commands, now.0 as u32, (target, target_id.0), &mut health, amount, projectile.owner);
            break;
        }
    }
}
