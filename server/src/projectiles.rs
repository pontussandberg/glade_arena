//! Shots: spawned when thrown, flying on each tick, and hitting whoever they reach first.

use super::*;

/// Projectiles are only replicated: clients draw them from where and when they were thrown.
pub(super) fn spawn_projectile(commands: &mut Commands, projectile: Projectile, (room, team): Side) {
    commands.spawn((projectile.bundle(), room, room.rooms(), team, Replicate::to_clients(NetworkTarget::All)));
}

pub(super) fn move_projectiles(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    mut projectiles: Query<(Entity, &mut Pos, &Projectile, &InRoom)>,
) {
    let tick = timeline.tick().0;
    for (entity, mut pos, projectile, room) in &mut projectiles {
        pos.0 = sim::projectile_pos(projectile, tick as f32);
        if sim::projectile_expired(lobby.map(room), pos.0, projectile, tick) {
            commands.entity(entity).despawn();
        }
    }
}

/// Projectile hits on the shooter's foes (allies' pass through), judged against where everyone
/// is now, like every hit. Clients see the result through replicated `Health` (and `Chilled`).
pub(super) fn resolve_projectile_hits(
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
                can_crit: projectile.ability.is_none(),
            };
            let def = projectile.class.def();
            let with = projectile.ability.and_then(|slot| def.ability(slot)).map_or("auto-attack", |ability| ability.name.as_str());
            let hit = (&mut *health, &mut *chilled, &mut *recent);
            damage(&mut commands, now.0, (target, target_id.0), hit, blow, (projectile.owner, projectile.class, with));
            break;
        }
    }
}
