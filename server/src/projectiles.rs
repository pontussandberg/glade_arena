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
    jumpers: Query<&arena_shared::protocol::Jumping>,
    lobby: Res<Lobby>,
    mut log: ResMut<HitLog>,
) {
    let now = timeline.tick();
    // Frostbolts' bursts (`AttackDef::burst`), dealt once every shot's hit is: where, by whom,
    // how hard, and whom they spare (the one struck). And storm bolts' chains (`AttackDef::chain`)
    // likewise: from the one struck, on to the nearest.
    let mut bursts = Vec::new();
    let mut chains = Vec::new();
    for (projectile_entity, projectile_pos, projectile, room, team, might, spares, (bounces, mut pierced)) in &mut projectiles {
        let pierces = projectile.ability.is_none() && projectile.class.def().attack.pierces;
        let mut through: Vec<PeerId> = pierced.as_ref().map_or_else(Vec::new, |p| p.0.clone());
        for (target, target_id, (target_room, target_team), target_pos, mut health, mut chilled, mut recent, _, (sheltered, decoy)) in &mut targets {
            if target_id.0 == projectile.owner
                || sheltered
                || owns(projectile.owner, decoy)
                || spares.is_some_and(|s| s.0 == target_id.0)
                || through.contains(&target_id.0)
                || health.0 == 0
                || !foes((*room, *team), (*target_room, *target_team))
                || !sim::projectile_hits(projectile_pos.0, projectile, now.0, target_pos.0, lobby.map(room).height_at(target_pos.0) + feet_of(&jumpers, target, now.0)) {
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
            if let Some(chain) = def.attack.chain.filter(|_| source == Source::Attack) {
                chains.push((chain, target_pos.0, target_id.0, *projectile, mighty(amount, chain.share), chill, (*room, *team)));
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
    // Each arc leaps to the nearest foe within reach of the last one struck that hasn't been
    // struck by this bolt, and on from there, so a shot into a pack runs through it.
    for (chain, mut from, struck, projectile, amount, chill, side) in chains {
        let mut hit = vec![struck];
        let reach = chain.reach + PLAYER_RADIUS;
        for _ in 0..chain.hops {
            let next = targets
                .iter()
                .filter(|(_, id, (room, team), pos, health, _, _, _, (sheltered, decoy))| {
                    !hit.contains(&id.0) && id.0 != projectile.owner && health.0 > 0 && !*sheltered && !owns(projectile.owner, *decoy) && foes(side, (**room, **team)) && pos.0.distance(from) <= reach
                })
                .min_by(|a, b| a.3.0.distance(from).total_cmp(&b.3.0.distance(from)))
                .map(|(entity, id, _, pos, ..)| (entity, id.0, pos.0));
            let Some((entity, id, at)) = next else { break };
            crate::fx::spawn_fx(&mut commands, side.0, Fx::between(FxKind::Arc, from, at, 0.0, now.0, 6));
            let Ok((.., mut health, mut chilled, mut recent, _, _)) = targets.get_mut(entity) else { break };
            let blow = Blow::attack(amount, chill, false, projectile.crit);
            damage(&mut commands, &mut log, now.0, (entity, id, at), (&mut health, &mut chilled, &mut recent), blow, (projectile.owner, projectile.class, "auto-attack"), (side, Source::Splash, None));
            hit.push(id);
            from = at;
        }
    }
}

/// How far off the ground under them `target`'s feet are at `tick`: a jump's height, if it's one
/// who can jump and is.
fn feet_of(jumpers: &Query<&arena_shared::protocol::Jumping>, target: Entity, tick: u32) -> f32 {
    jumpers.get(target).ok().and_then(|jumping| jumping.0).map_or(0.0, |jump| jump.height_at(tick as f32))
}
