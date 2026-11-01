//! Practice dummies: placed by a player, standing or walking a patrol, and restored after
//! they're hit.

use super::*;

use arena_shared::map::Map;

use crate::rooms::InRoom;

/// Server-only: a target dummy, for practice: a fighter of `class` standing at a spot, never
/// moving or attacking, hit like anyone else, and back to full health instead of dying. `home`
/// is where it was placed, where it comes back to, and `feet` how high it stood there.
#[derive(Component)]
pub(crate) struct Dummy {
    pub(super) home: Vec2,
    pub(super) feet: f32,
}

/// A target dummy of `class` standing `at` (a walkable spot) on `map` in `room`, as `id` (none
/// of the clients'), on top of whatever is there (a tower's, a crate's). Replicated straight
/// away: it already stands where it'll stay.
pub(crate) fn spawn_dummy(commands: &mut Commands, map: &Map, id: PeerId, class: ClassId, room: InRoom, at: Vec2) -> Entity {
    let feet = map.surface_under(at, f32::INFINITY);
    commands
        .spawn((
            Name::from("Dummy"),
            Dummy { home: at, feet },
            fighter(id, class, room, Team(arena_shared::rooms::NO_TEAM), at),
            replicated_npc(),
        ))
        .insert(Motion { height: feet, ..default() })
        .id()
}

/// Fighters nothing walks (dummies, the shopkeeper: `move_players` walks everyone else) stand on
/// what's under them as they're moved about (a patrol, a shove), up and down the ground with
/// them, so they're hit and drawn where they stand, not down at the gameplay plane.
pub(super) fn stand_unwalked(lobby: Res<crate::rooms::Lobby>, mut bodies: Query<(&Pos, &InRoom, &mut Motion), (Without<PlayerInput>, InPlay)>) {
    for (pos, room, mut motion) in &mut bodies {
        let height = lobby.map(room).floor_at(pos.0, motion.height);
        motion.set_if_neq(Motion { height, ..*motion });
    }
}

/// Server-only: a practice dummy that walks back and forth between `ends`, toward `ends[toward]`,
/// at its class's walking speed (slowed, rooted and hasted like anyone).
#[derive(Component)]
pub(crate) struct Patrol {
    pub(super) ends: [Vec2; 2],
    pub(super) toward: usize,
}

impl Patrol {
    /// How far a walking dummy goes each way from where it was placed, at most.
    const REACH: f32 = 3.0;

    /// The longest clear straight line through `at` on `map`, along x or y (`REACH` each way,
    /// shortened at walls), if it's long enough to be worth walking.
    pub(crate) fn through(map: &Map, at: Vec2) -> Option<Patrol> {
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
pub(super) fn walk_dummies(timeline: Res<LocalTimeline>, mut dummies: Query<(&mut Pos, &ClassId, &mut Patrol, &Chilled, &Hasted), InPlay>) {
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

/// A dummy that would die is back on its spot (where it was placed), fresh as a respawned player (full health, its
/// slows, roots and haste gone); its hits still show.
pub(super) fn restore_dummies(
    mut commands: Commands,
    mut dummies: Query<(Entity, &Dummy, &MaxHealth, &mut Pos, &mut Health, (&mut Chilled, &mut Hasted), &mut AttackState, (&mut Cooldowns, &mut Dashing, &mut Motion)), With<Dead>>,
) {
    for (entity, dummy, max, mut pos, mut health, (mut chilled, mut hasted), mut attack, (mut cooldowns, mut dashing, mut motion)) in &mut dummies {
        pos.set_if_neq(Pos(dummy.home));
        motion.set_if_neq(Motion { height: dummy.feet, ..default() });
        health.0 = max.0;
        chilled.set_if_neq(Chilled::default());
        hasted.set_if_neq(Hasted::default());
        attack.set_if_neq(AttackState::default());
        cooldowns.set_if_neq(Cooldowns::default());
        dashing.set_if_neq(Dashing::default());
        commands.entity(entity).remove::<Dead>();
    }
}
