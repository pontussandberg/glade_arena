//! Zombies, roaming lobby matches until they're won: every so often one rises at the spawn point
//! farthest from everyone, shambles toward the nearest fighter and claws at it once it's in
//! reach. They're fighters like any other (hit, slowed, frozen and killed like anyone) on their
//! own side (`rooms::ZOMBIES`), only driven by the server instead of a client: it sets their
//! intent (`PlayerInput`) as a player's commands would. A dead one is gone for good. Killing one
//! scores nothing toward the match, but sends its killer into a frenzy (`Frenzy`).

use std::collections::HashMap;

use super::*;

use arena_shared::classes::AttackKind;
use arena_shared::rooms::{RoomKey, ZOMBIES};

use crate::rooms::{InRoom, Lobby};

/// Server-only: a zombie.
#[derive(Component)]
pub(crate) struct Zombie;

/// Server-only: walks at this share of its speed (practice's slow zombies).
#[derive(Component, Clone, Copy)]
pub(crate) struct Sluggish(pub(crate) f32);

/// How fast practice's slow zombies walk, of a zombie's speed.
pub(crate) const PRACTICE_ZOMBIE_SPEED: f32 = 0.5;

/// A zombie standing `at` in `room`, coming for whoever's nearest.
pub(crate) fn spawn_zombie(commands: &mut Commands, id: PeerId, room: InRoom, at: Vec2) -> Entity {
    commands.spawn((Name::from("Zombie"), Zombie, fighter(id, zombie_class(), room, Team(ZOMBIES), at), PlayerInput::default())).id()
}

/// How long into a match the first zombie rises, and how often another may after that.
const FIRST_ZOMBIE_TICKS: u32 = 8 * TICK_HZ as u32;
const ZOMBIE_EVERY_TICKS: u32 = 7 * TICK_HZ as u32;
/// How long a dead zombie lies (hidden, as the dead are) before it's gone, so every client sees
/// it fall first.
const BURY_TICKS: u32 = TICK_HZ as u32;

/// How many zombies may roam a room with this many fighters in its arena: a few, and one more
/// for each fighter past that, up to a horde.
fn most_zombies(fighters: usize) -> usize {
    (fighters + 1).clamp(3, 8)
}

fn zombie_class() -> ClassId {
    ClassId::by_key("zombie").expect("the zombie's class file")
}

/// A zombie rises in each room on the hunt (`Lobby::hunting`) when it's due, unless there are
/// as many roaming it as there may be, or no one in its arena. It's placed like a fighter
/// joining (`place_players`): at the spawn point farthest from everyone.
pub(super) fn raise_zombies(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut lobby: ResMut<Lobby>,
    mut due: Local<HashMap<RoomKey, u32>>,
    fighters: Query<&InRoom, (With<PlayerId>, With<ControlledBy>)>,
    zombies: Query<&InRoom, With<Zombie>>,
) {
    let now = timeline.tick().0;
    let hunting: Vec<InRoom> = lobby.hunting().collect();
    // A match that starts again waits for its first zombie again.
    due.retain(|key, _| hunting.iter().any(|room| room.key == *key));
    for room in hunting {
        let at = due.entry(room.key).or_insert(now + FIRST_ZOMBIE_TICKS);
        if now < *at {
            continue;
        }
        *at = now + ZOMBIE_EVERY_TICKS;
        let in_arena = fighters.iter().filter(|r| **r == room).count();
        let roaming = zombies.iter().filter(|r| **r == room).count();
        if in_arena == 0 || roaming >= most_zombies(in_arena) {
            continue;
        }
        let id = lobby.next_local();
        let zombie = spawn_zombie(&mut commands, id, room, Vec2::ZERO);
        commands.entity(zombie).insert(NeedsSpawnPoint);
        debug!(room = room.key.0, roaming = roaming + 1, "a zombie rises");
    }
}

/// Each zombie goes for the nearest fighter in its room that's in play: walks toward it, or once
/// it's well within its claws' reach, swipes at it (the swipe's windup roots it, as anyone's
/// does, so stepping away from it dodges). With no one to go for, it stands.
pub(super) fn chase(
    // The waves' monsters think for themselves (`brain.rs`).
    mut zombies: Query<(&Pos, &ClassId, &InRoom, &mut PlayerInput), (With<Zombie>, Without<Mob>, InPlay)>,
    // Fighters, not practice's target dummies.
    prey: Query<(&Pos, &InRoom), (With<PlayerId>, Without<Zombie>, Without<Untouchable>, Without<crate::dummies::Dummy>, InPlay)>,
) {
    for (pos, class, room, mut input) in &mut zombies {
        let nearest = prey
            .iter()
            .filter(|(_, r)| *r == room)
            .map(|(p, _)| p.0)
            .min_by(|a, b| a.distance_squared(pos.0).total_cmp(&b.distance_squared(pos.0)));
        let Some(target) = nearest else {
            input.move_to = None;
            continue;
        };
        // Well within, so a target standing still is hit, and one stepping back may be too.
        let reach = class.def().attack.hit_span(false).map_or(0.0, |(_, far)| far) * 0.8;
        if pos.distance(target) <= reach {
            input.move_to = None;
            input.aim = target - pos.0;
            input.fire = true;
        } else {
            input.move_to = Some(target);
        }
    }
}

/// A zombie killed (its last hit by a player) counts toward its killer's zombie kills, and
/// sends them into a frenzy as strong as those kills before it make it: walking and attacking
/// faster, fading away (slower for a fighter in melee). A new one replaces what's left of the last.
pub(super) fn feed_frenzy(
    timeline: Res<LocalTimeline>,
    mut lobby: ResMut<Lobby>,
    killed: Query<(&InRoom, &Dead), (With<Zombie>, Added<Dead>)>,
    mut killers: Query<(&PlayerId, &ClassId, &InRoom, &mut Frenzy), Without<Zombie>>,
) {
    let now = timeline.tick().0;
    for (room, dead) in &killed {
        let Some(before) = lobby.count_zombie_kill(room.key, dead.killer) else { continue };
        // Against the waves, kills are counted, but frenzies come from items and shrines.
        if lobby.pve(room.key) {
            continue;
        }
        // Its killer may have left the arena since (its shot still flying): counted all the same.
        let Some((_, class, _, mut frenzy)) = killers.iter_mut().find(|(id, _, r, _)| id.0 == dead.killer && *r == room) else { continue };
        let melee = matches!(class.def().attack.kind, AttackKind::Melee { .. });
        *frenzy = Frenzy::after_kill(now, before, melee);
    }
}

/// Dead zombies are gone once they've lain a moment, and every zombie is gone from a room no
/// longer on the hunt (its match won or over, the room closed).
pub(super) fn bury_zombies(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    zombies: Query<(Entity, &InRoom, Option<&Dead>), With<Zombie>>,
) {
    let now = timeline.tick().0;
    for (zombie, room, dead) in &zombies {
        // It died `RESPAWN_TICKS` before it would respawn.
        let buried = dead.is_some_and(|dead| now + RESPAWN_TICKS >= dead.respawn_at + BURY_TICKS);
        if buried || !lobby.keeps_monsters(room.key) {
            commands.entity(zombie).try_despawn();
        }
    }
}
