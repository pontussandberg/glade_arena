//! Shots: spawned when thrown, flying on each tick, and hitting whoever they reach first.

use super::*;

use arena_shared::items::Item;
use arena_shared::map::ShotEnd;
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

/// Server-only: the monsters a piercing shot (an Impaler holder's javelin) has gone through, so
/// it hits each once.
#[derive(Component, Default)]
pub(crate) struct Pierced(Vec<PeerId>);

/// Server-only: up to what tick a shot's flight has been judged for hits, so each tick judges
/// just the stretch flown since the last (`resolve_projectile_hits`).
#[derive(Component)]
pub(crate) struct Judged(u32);

/// Server-only: done at this tick (hit, or ended by the map), kept one tick before it's
/// despawned -- a shot spawned and despawned within one tick folds into nothing on the wire, and
/// its strike would never show on any client. It hits no one and goes nowhere meanwhile.
#[derive(Component)]
pub(crate) struct Spent(u32);

/// Despawns a done shot, or, done the very tick it spawned, keeps it one tick as `Spent`.
fn despawn_spent(commands: &mut Commands, entity: Entity, projectile: &Projectile, now: u32) {
    if now == projectile.spawn_tick {
        commands.entity(entity).try_insert(Spent(now));
    } else {
        commands.entity(entity).try_despawn();
    }
}

/// Server-only: ended this tick, by the map or its range. Its last stretch of flight is still
/// judged for hits (`resolve_projectile_hits`, which chains after); not stopped by anyone on it,
/// it bursts where it ended, if it's one that bursts, and goes.
#[derive(Component)]
pub(crate) struct Ending;

/// Shots fly on, and end at their range or against what's in their way (`Ending`: the hits on
/// their last stretch, and the burst where they end, are `resolve_projectile_hits`', right
/// after, so nothing standing just before the wall is passed over).
pub(super) fn move_projectiles(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    mut projectiles: Query<(Entity, &mut Pos, &Projectile, &InRoom), (Without<PlayerId>, Without<Spent>)>,
) {
    let tick = timeline.tick().0;
    for (entity, mut pos, projectile, room) in &mut projectiles {
        pos.0 = sim::projectile_pos(projectile, tick as f32);
        if sim::projectile_expired(lobby.map(room), pos.0, projectile, tick) {
            commands.entity(entity).try_insert(Ending);
        }
    }
}

/// An auto-attack's burst (`AttackDef::burst`) where its shot ended (`end`: shown on what it came
/// down on, or the side of what it flew into): `amount` (chilled as its shot chills) to each of
/// its shooter's foes round it but the one its shot struck (`spares`).
fn burst_at(
    commands: &mut Commands,
    log: &mut HitLog,
    now: u32,
    targets: &mut Targets,
    (projectile, amount): (&Projectile, i32),
    (burst, end, spares): (arena_shared::classes::BurstDef, ShotEnd, Option<PeerId>),
    side: Side,
) {
    let at = end.at;
    crate::fx::spawn_fx(commands, side.0, Fx::at(FxKind::Blast(burst.color), at, burst.radius, now, 10).ended(end).by(projectile.owner));
    let blow = Blow::attack(amount, sim::projectile_chill(projectile), false, projectile.crit);
    hit_foes(commands, log, now, targets, (projectile.owner, projectile.class, "auto-attack"), (side, Source::Splash), blow, |id, (p, _, size)| {
        Some(id) != spares && p.distance(at) <= burst.radius + PLAYER_RADIUS * size
    });
}

/// Projectile hits on the shooter's foes (allies' pass through), judged against where everyone
/// is now, like every hit, but along everything the shot flew since it was last judged (its
/// whole flight so far, the first time): no one fits between one tick's spot and the next, and
/// point blank counts. Those on the stretch are hit in the order flown: a shot that ends in one
/// ends in the first body on its way. Clients see the result through replicated `Health` (and
/// `Chilled`). A shot ends at the first it hits, but an Impaler holder's attack javelins fly on
/// through monsters (`items::impaler` of them at most), hitting each once.
#[allow(clippy::type_complexity)]
pub(super) fn resolve_projectile_hits(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut projectiles: Query<(Entity, &Projectile, &InRoom, &Team, Option<&Might>, Option<&Spares>, (Option<&Bounces>, Option<&mut Pierced>, Option<&mut Judged>, Option<&Spent>, Has<Ending>))>,
    mut targets: Targets,
    holders: Query<(&PlayerId, &arena_shared::items::Satchel)>,
    lobby: Res<Lobby>,
    mut log: ResMut<HitLog>,
) {
    let now = timeline.tick();
    // Frostbolts' bursts (`AttackDef::burst`), dealt once every shot's hit is: where, by whom,
    // how hard, and whom they spare (the one struck).
    let mut bursts = Vec::new();
    for (projectile_entity, projectile, room, team, might, spares, (bounces, mut pierced, mut judged, spent, ending)) in &mut projectiles {
        // Done already, lingering a tick for the wire: its time come, it goes.
        if let Some(spent) = spent {
            if now.0 > spent.0 {
                commands.entity(projectile_entity).try_despawn();
            }
            continue;
        }
        let impaler = holders.iter().find(|(id, _)| id.0 == projectile.owner).map_or(0, |(_, satchel)| satchel.level(Item::Impaler));
        let through_at_most = if projectile.ability.is_none() && impaler > 0 { arena_shared::items::impaler(impaler) } else { 0 };
        let mut through: Vec<PeerId> = pierced.as_ref().map_or_else(Vec::new, |p| p.0.clone());
        let flew = (judged.as_ref().map_or(projectile.spawn_tick as f32 - 1.0, |j| j.0 as f32), now.0 as f32);
        match judged.as_mut() {
            Some(judged) => judged.0 = now.0,
            None => {
                commands.entity(projectile_entity).try_insert(Judged(now.0));
            }
        }
        let mut struck: Vec<(Entity, f32)> = targets
            .iter()
            .filter_map(|(target, target_id, (target_room, target_team), target_pos, health, _, _, _, (sheltered, decoy, motion, target_class, elite))| {
                if target_id.0 == projectile.owner
                    || sheltered
                    || owns(projectile.owner, decoy)
                    || spares.is_some_and(|s| s.0 == target_id.0)
                    || through.contains(&target_id.0)
                    || health.0 == 0
                    || !foes((*room, *team), (*target_room, *target_team))
                {
                    return None;
                }
                sim::projectile_hits(projectile, flew, (target_pos.0, motion.height, target_class.def().hit_size(elite))).map(|met_at| (target, met_at))
            })
            .collect();
        struck.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut stopped = false;
        for (struck, met) in struck {
            let Ok((target, target_id, (_, target_team), target_pos, mut health, mut chilled, mut recent, _, (_, _, motion, ..))) = targets.get_mut(struck) else { continue };
            let goes_on = target_team.0 == arena_shared::rooms::ZOMBIES && (through.len() as u32) < through_at_most;
            let first = through.is_empty();
            if goes_on {
                through.push(target_id.0);
            } else {
                despawn_spent(&mut commands, projectile_entity, projectile, now.0);
                stopped = true;
            }
            // Priced where the body was met, not at the stretch's end: `far_scale` earns only
            // the flight really flown.
            let amount = mighty(sim::projectile_damage(projectile, met.round() as u32), might_of(might));
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
                // On the one struck: where its feet are.
                let end = ShotEnd { at: target_pos.0, height: lobby.map(room).surface_under(target_pos.0, motion.height + 1e-3), wall: None };
                bursts.push((burst, end, Some(target_id.0), *projectile, mighty(amount, burst.share), (*room, *team)));
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
        // Ended by the map (or its range) with no one on its last stretch to stop it: it bursts
        // there, if it's one that bursts, and goes.
        if ending && !stopped {
            despawn_spent(&mut commands, projectile_entity, projectile, now.0);
            if let Some(burst) = projectile.class.def().attack.burst.filter(|_| projectile.ability.is_none() && bounces.is_none()) {
                let amount = mighty(mighty(sim::projectile_damage(projectile, now.0), might_of(might)), burst.share);
                let end = sim::projectile_end(lobby.map(room), projectile, now.0);
                bursts.push((burst, end, None, *projectile, amount, (*room, *team)));
            }
        }
    }
    for (burst, end, struck, projectile, amount, side) in bursts {
        burst_at(&mut commands, &mut log, now.0, &mut targets, (&projectile, amount), (burst, end, struck), side);
    }
}

