//! Fighters coming into a match and going out of it: a new link's player, its fighter of the
//! class it picked, placed at a spawn point, dead and back again.

use super::*;

use arena_shared::items::Satchel;

use crate::rooms::{InRoom, Lobby};

/// Replicated to everyone; interpolated by all but the owner, who predicts it
/// (`render::glide`; a headless client shows it as last received).
fn replicated_player(owner: PeerId) -> impl Bundle {
    (
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(owner)),
    )
}

/// Replicated to everyone and interpolated by all: a fighter no client controls (a dummy, a
/// monster, the shopkeeper, a decoy), there from the start.
pub(crate) fn replicated_npc() -> impl Bundle {
    (Replicate::to_clients(NetworkTarget::All), InterpolationTarget::to_clients(NetworkTarget::All))
}

/// Server-only: this player (new, respawning, or on a map just changed to) still needs a spawn
/// point.
#[derive(Component)]
pub(crate) struct NeedsSpawnPoint;

/// Server-only: killed (by `killer`); out of the fight until `respawn_at` (its `Health` is 0
/// meanwhile, which is what clients see).
#[derive(Component)]
pub(crate) struct Dead {
    pub(crate) respawn_at: u32,
    pub(super) placed: bool,
    pub(crate) killer: PeerId,
}

/// Server-only: asked to be back on its feet at once (`RoomRequest::RespawnNow`), taken up by
/// `count_down_respawns`.
#[derive(Component)]
pub(crate) struct RespawnNow;

/// A respawning player is moved to its spawn point this long before it comes back to life:
/// longer than other clients' interpolation delay, so they never see it slide from where it
/// died to where it respawns (it's still hidden while that happens).
pub(crate) const PLACE_BEFORE_RESPAWN_TICKS: u32 = TICK_HZ as u32 / 4;

/// Players who can move, attack and be hit: placed and alive.
pub(crate) type InPlay = (Without<NeedsSpawnPoint>, Without<Dead>);

/// Every new link needs a `ReplicationSender` before we can replicate anything to it.
pub(super) fn on_new_link(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(ReplicationSender);
}

/// What every fighter is, player or dummy: who, of which class, where and on which side, and
/// its fight state (health, crowd control, attack and ability), fresh.
pub(super) fn fighter(id: PeerId, class: ClassId, room: InRoom, team: Team, at: Vec2) -> impl Bundle {
    (
        PlayerId(id),
        (class, team, room, room.rooms()),
        Pos(at),
        (Health(class.def().max_hp), MaxHealth(class.def().max_hp)),
        (Chilled::default(), Hasted::default(), Frenzy::default(), Fury::default(), RecentHits::default()),
        (AttackState::default(), Cooldowns::default(), Dashing::default(), arena_shared::protocol::Motion::default(), LastSwing::default(), Walking::default(), WalkOrders::default()),
    )
}

/// A room member's fighter, as `class`, on `team`. Despawned with the client's link when it
/// disconnects; `rooms.rs` despawns it when the member leaves the room.
pub(super) fn spawn_player(commands: &mut Commands, link: Entity, id: PeerId, class: ClassId, room: InRoom, team: Team) -> Entity {
    commands
        .spawn((
            Name::from("Player"),
            fighter(id, class, room, team, Vec2::ZERO),
            NeedsSpawnPoint,
            DashHits::default(),
            OnGate::default(),
            PlayerInput::default(),
            (Satchel::default(), ItemState::default()),
            // Replication starts in `place_players`, once it has a real position.
            ControlledBy { owner: link, lifetime: default() },
        ))
        .id()
}

/// The shopkeeper, standing `at` in `room`: a fighter no one can hurt, on the players' side.
pub(crate) fn spawn_shopkeeper(commands: &mut Commands, room: InRoom, at: Vec2) -> Entity {
    let class = ClassId::by_key("merchant").expect("the merchant's class file");
    // Its own number, far from the ones handed out for monsters and dummies.
    let id = PeerId::Local(u64::MAX - room.key.0 as u64);
    commands
        .spawn((
            Name::from("Shopkeeper"),
            Shopkeeper,
            Untouchable,
            fighter(id, class, room, Team(arena_shared::rooms::TEAM_A), at),
            replicated_npc(),
        ))
        .id()
}

/// Places waiting players one at a time, each away from everyone already placed in their room,
/// so players joining or respawning in the same tick don't land on the same spot.
pub(super) fn place_players(
    mut commands: Commands,
    lobby: Res<Lobby>,
    mut players: Query<(Entity, &PlayerId, &InRoom, &mut Pos, Option<&mut PlayerInput>, Has<NeedsSpawnPoint>, Has<Replicate>)>,
) {
    let mut placed: Vec<(InRoom, Vec2)> =
        players.iter().filter(|(.., waiting, _)| !waiting).map(|(_, _, room, pos, ..)| (*room, pos.0)).collect();
    for (entity, id, room, mut pos, input, waiting, replicated) in &mut players {
        if !waiting {
            continue;
        }
        pos.0 = lobby.spawn_point(room, placed.iter().filter(|(r, _)| r == room).map(|(_, p)| *p));
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

/// Respawning: first moved to a spawn point while still hidden, then back at full health.
pub(super) fn respawn(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    // Dummies come back on the spot instead (`restore_dummies`); zombies don't (`bury_zombies`).
    mut dead: Query<(Entity, &MaxHealth, &mut Health, (&mut Chilled, &mut Hasted, &mut Frenzy, &mut Fury), &mut AttackState, (&mut Cooldowns, &mut Dashing), &mut Dead), (Without<Dummy>, Without<Zombie>, Without<Decoy>)>,
) {
    let now = timeline.tick().0;
    for (player, max, mut health, (mut chilled, mut hasted, mut frenzy, mut fury), mut attack, (mut cooldowns, mut dashing), mut dead) in &mut dead {
        if !dead.placed && now + PLACE_BEFORE_RESPAWN_TICKS >= dead.respawn_at {
            dead.placed = true;
            commands.entity(player).insert(NeedsSpawnPoint);
        }
        if now >= dead.respawn_at {
            health.0 = max.0;
            chilled.set_if_neq(Chilled::default());
            hasted.set_if_neq(Hasted::default());
            // Its frenzy's over (the zombies it killed still count: `Member::zombie_kills`).
            frenzy.set_if_neq(Frenzy::default());
            fury.set_if_neq(Fury::default());
            *attack = AttackState::default();
            // Practice's No cooldowns stays as it was.
            *cooldowns = Cooldowns { free: cooldowns.free, ..default() };
            *dashing = Dashing::default();
            commands.entity(player).remove::<Dead>();
            debug!(target: "arena_server", ?player, "respawned");
        }
    }
}

/// How long until each dead player is back, for them to see (`Respawning`): none on one down
/// until the wave's end or an ally's rescue, and gone once it's back, however it came back. One
/// that asked to be back at once (`RespawnNow`) is, as soon as it's moved to its spawn point.
pub(super) fn count_down_respawns(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut players: Query<(Entity, Option<&mut Dead>, Option<&mut Respawning>, Has<RespawnNow>), (Or<(With<Dead>, With<Respawning>, With<RespawnNow>)>, Without<Dummy>, Without<Zombie>, Without<Decoy>)>,
) {
    let now = timeline.tick().0;
    for (player, dead, respawning, hurry) in &mut players {
        if hurry {
            commands.entity(player).remove::<RespawnNow>();
        }
        let Some(mut dead) = dead.filter(|dead| dead.respawn_at < crate::waves::UNTIL_CLEARED) else {
            if respawning.is_some() {
                commands.entity(player).remove::<Respawning>();
            }
            continue;
        };
        if hurry {
            dead.respawn_at = dead.respawn_at.min(now + PLACE_BEFORE_RESPAWN_TICKS + 1);
        }
        let secs = Respawning { secs: dead.respawn_at.saturating_sub(now).div_ceil(TICK_HZ as u32) };
        match respawning {
            Some(mut respawning) => {
                respawning.set_if_neq(secs);
            }
            None => {
                commands.entity(player).insert(secs);
            }
        }
    }
}
