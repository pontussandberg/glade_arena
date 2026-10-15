//! Shots: spawned when thrown, flying on each tick, and hitting whoever they reach first.

use super::*;

use arena_shared::items::Item;
use arena_shared::sim;

use crate::rooms::{InRoom, Lobby};

/// Projectiles are only replicated: clients draw them from where and when they were thrown. Its
/// hits deal `might` times what its class says.
pub(super) fn spawn_projectile(commands: &mut Commands, projectile: Projectile, (room, team): Side, might: f32) -> Entity {
    commands.spawn((projectile.bundle(), room, room.rooms(), team, Might(might), Replicate::to_clients(NetworkTarget::All))).id()
}

/// Server-only: a shot that flies on from a fighter it hit (bounced, split) passes through that
/// one, so it isn't hit again the moment the shot appears.
#[derive(Component)]
pub(crate) struct Spares(pub(crate) PeerId);

/// Server-only: the monsters a piercing shot (`AttackDef::pierces`) has gone through, so it hits
/// each once.
#[derive(Component, Default)]
pub(crate) struct Pierced(Vec<PeerId>);

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
/// A shot ends at the first it hits, but one that pierces (`AttackDef::pierces`, an auto-attack's)
/// flies on through monsters, hitting each once.
#[allow(clippy::type_complexity)]
pub(super) fn resolve_projectile_hits(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut projectiles: Query<(Entity, &Pos, &Projectile, &InRoom, &Team, Option<&Might>, Option<&Spares>, (Option<&Bounces>, Option<&mut Pierced>))>,
    mut targets: Targets,
    mut log: ResMut<HitLog>,
) {
    let now = timeline.tick();
    // Frostbolts' bursts (`AttackDef::burst`), dealt once every shot's hit is: where, by whom,
    // how hard, and whom they spare (the one struck).
    let mut bursts = Vec::new();
    for (projectile_entity, projectile_pos, projectile, room, team, might, spares, (bounces, mut pierced)) in &mut projectiles {
        let pierces = projectile.ability.is_none() && projectile.class.def().attack.pierces;
        let mut through: Vec<PeerId> = pierced.as_ref().map_or_else(Vec::new, |p| p.0.clone());
        for (target, target_id, (target_room, target_team), target_pos, mut health, mut chilled, mut recent, _, sheltered) in &mut targets {
            if target_id.0 == projectile.owner
                || sheltered
                || spares.is_some_and(|s| s.0 == target_id.0)
                || through.contains(&target_id.0)
                || health.0 == 0
                || !foes((*room, *team), (*target_room, *target_team))
                || !sim::projectile_hits(projectile_pos.0, projectile, target_pos.0) {
                continue;
            }
            let goes_on = pierces && target_team.0 == arena_shared::rooms::ZOMBIES;
            let first = through.is_empty();
            if goes_on {
                through.push(target_id.0);
            } else {
                commands.entity(projectile_entity).try_despawn();
            }
            let amount = mighty(sim::projectile_damage(projectile, now.0), might_of(might));
            let chill = sim::projectile_chill(projectile);
            // Only an auto-attack's own javelin or bolt (a Hydra Spear's barrage too: thrown like
            // it) is the attack's, setting off on-hit items; a bounce is the Ricochet Charm's.
            let (source, with) = match projectile.ability {
                Some(slot) => (Source::Ability(slot), projectile.class.def().ability(slot).map_or("ability", |a| a.name.as_str())),
                None if bounces.is_some() => (Source::Item(Item::RicochetCharm), Item::RicochetCharm.name()),
                None => (Source::Attack, "auto-attack"),
            };
            let blow = if source == Source::Attack { Blow::attack(amount, chill, false, projectile.crit) } else { Blow::ability(amount, chill) };
            let def = projectile.class.def();
            let hit = (&mut *health, &mut *chilled, &mut *recent);
            let at = (target, target_id.0, target_pos.0);
            let shot = ShotHit { projectile: *projectile, might: might_of(might), bounces: bounces.map(|b| b.0), pierced: !first };
            damage(&mut commands, &mut log, now.0, at, hit, blow, (projectile.owner, projectile.class, with), ((*room, *team), source, Some(shot)));
            if let Some(burst) = def.attack.burst.filter(|_| source == Source::Attack) {
                bursts.push((burst, target_pos.0, target_id.0, *projectile, mighty(amount, burst.share), chill, (*room, *team)));
            }
            if !goes_on {
                break;
            }
        }
        // Who it's gone through, for the ticks it flies on.
        match pierced.as_mut() {
            Some(pierced) if pierced.0.len() != through.len() => pierced.0 = through,
            None if !through.is_empty() => {
                commands.entity(projectile_entity).try_insert(Pierced(through));
            }
            _ => {}
        }
    }
    for (burst, at, struck, projectile, amount, chill, side) in bursts {
        crate::fx::spawn_fx(&mut commands, side.0, Fx::at(FxKind::Blast(burst.color), at, burst.radius, now.0, 10));
        let reach = burst.radius + PLAYER_RADIUS;
        let blow = Blow::attack(amount, chill, false, projectile.crit);
        hit_foes(&mut commands, &mut log, now.0, &mut targets, (projectile.owner, projectile.class, "auto-attack"), (side, Source::Splash), blow, |id, p| id != struck && p.distance(at) <= reach);
    }
}
