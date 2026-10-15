//! What items do in a fight (`arena_shared::items`), on the server. Two passes a tick, after the
//! attacks and abilities:
//!
//! - `tick_items`: what goes on by itself: wisps circling, kegs lobbed, frost left underfoot, rifts
//!   burning, zones (frost patches, blizzards, vortices) pulsing, things set to go off going off
//!   (`Pending`), decoys shattering, doom bells tolling.
//! - `react`: every hit landed this tick (`HitLog`), in order, and every ability used
//!   (`CastLog`): arcs, bursts, freezes, heals, executions, marks, bounces and splits off hits;
//!   echoes, vortices, decoys and blizzards off abilities; thorns and the phoenix off being hit;
//!   and monsters' twists (an elite's, a bloater's). What these hit is logged too, and reacted
//!   to in turn (a burst killing sets off the next burst), a few rounds deep at most.
//!
//! Effects never crit, and an item's own hits only set off what reacts to kills, so nothing
//! feeds itself forever.

use std::collections::HashMap;

use super::*;

use arena_shared::classes::{AbilityKind, Chill, Trick};
use arena_shared::items::{self, Item, Satchel, wisp_offset};
use arena_shared::map::Map;
use arena_shared::sim;

use crate::brain::{self, Mob};
use crate::fx::spawn_fx;
use crate::rooms::{InRoom, Lobby};

/// Server-only: what a fighter's items keep track of between ticks.
#[derive(Component, Default)]
pub(crate) struct ItemState {
    /// Auto-attack hits landed, for the items that go off every so many.
    hits: u32,
    /// When the thorns may lash again, a keg may be lobbed, the phoenix rise, a decoy be left.
    lash_at: u32,
    keg_at: u32,
    phoenix_at: u32,
    mirror_at: u32,
    /// When the next frost patch may be left, and where the last was.
    trail: (u32, Vec2),
    /// Each wisp's last touch on a fighter: (wisp, whom, when it may touch them again).
    wisp_hits: Vec<(u32, PeerId, u32)>,
}

/// Whether something that recharges is ready at `now` (ready again at `*at`): if so, it's used,
/// and ready again `recharge` later.
fn ready(at: &mut u32, now: u32, recharge: u32) -> bool {
    let ready = now >= *at;
    if ready {
        *at = now + recharge;
    }
    ready
}

/// Server-only: a decoy of `owner`, left by a Mirror Shard: a fighter like them that monsters go
/// for, shattering for `damage` around it when its time is up or it's killed.
#[derive(Component)]
pub(crate) struct Decoy {
    owner: PeerId,
    until: u32,
    damage: i32,
    radius: f32,
}

/// Server-only: marked by a Doom Bell's holder (`by`): what they've dealt it since, tolled again
/// as a share of it (`share`) at `at`.
#[derive(Component)]
pub(crate) struct Doomed {
    by: PeerId,
    class: ClassId,
    side: Side,
    at: u32,
    stored: i32,
    share: f32,
}

/// Server-only: how many more times a shot bounces (a Ricochet Charm's), and whether it's one
/// split off a javelin (which doesn't split again).
#[derive(Component, Clone, Copy)]
pub(crate) struct Bounces(pub(crate) u8);

#[derive(Component)]
pub(crate) struct Shard;

/// Whose blow something is: who, of which class, on which side, and what dealt it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct By {
    pub(crate) id: PeerId,
    pub(crate) class: ClassId,
    pub(crate) side: Side,
    pub(crate) source: Source,
}

impl By {
    fn name(&self) -> &'static str {
        match self.source {
            Source::Item(item) => item.name(),
            Source::Trick => "a trick",
            Source::Attack => "auto-attack",
            Source::Ability(_) => "ability",
        }
    }
}

/// A blast set to go off at `fire_at`: `damage` (and `chill`) to `by`'s foes within `radius` of
/// `at`, in `color`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Delayed {
    pub(crate) by: By,
    pub(crate) at: Vec2,
    pub(crate) radius: f32,
    pub(crate) damage: i32,
    pub(crate) chill: Chill,
    pub(crate) fire_at: u32,
    pub(crate) color: FxColor,
}

/// Server-only: blasts set to go off.
#[derive(Resource, Default)]
pub(crate) struct Pending(pub(crate) Vec<Delayed>);

/// Sets `blast` to go off, `look` showing where and when until it does.
pub(crate) fn arm(commands: &mut Commands, pending: &mut Pending, now: u32, blast: Delayed, look: FxKind) {
    spawn_fx(commands, blast.by.side.0, Fx::at(look, blast.at, blast.radius, now, blast.fire_at.saturating_sub(now)));
    pending.0.push(blast);
}

/// Ground that hurts `by`'s foes in it every `ZONE_PULSE_TICKS` (`damage`, slowing them `slow`),
/// pulling them toward its middle (`pull`, meters per second), until `until`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Zone {
    by: By,
    at: Vec2,
    radius: f32,
    damage: i32,
    slow: f32,
    pull: f32,
    until: u32,
}

/// Server-only: zones on the ground.
#[derive(Resource, Default)]
pub(crate) struct Zones(Vec<Zone>);

/// `blow` from `by` to its foes `hits` says are hit (`hit_foes`). How many it hit.
fn strike(commands: &mut Commands, log: &mut HitLog, now: u32, targets: &mut Targets, by: By, blow: Blow, hits: impl FnMut(PeerId, Vec2) -> bool) -> usize {
    hit_foes(commands, log, now, targets, (by.id, by.class, by.name()), (by.side, by.source), blow, hits)
}

/// Deals `damage` (and `chill`) to `by`'s foes within `radius` of `at`, shown as a blast in `color`.
/// How many it hit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blast(commands: &mut Commands, log: &mut HitLog, now: u32, targets: &mut Targets, by: By, at: Vec2, radius: f32, damage: i32, chill: Chill, color: FxColor) -> usize {
    spawn_fx(commands, by.side.0, Fx::at(FxKind::Blast(color), at, radius, now, 10));
    strike(commands, log, now, targets, by, Blow::ability(damage, chill), |_, p| p.distance(at) <= radius + PLAYER_RADIUS)
}

/// The foe of `side` nearest `at` within `reach`, alive, other than those `skip` says: who and where.
fn nearest_foe(targets: &Targets, side: Side, at: Vec2, reach: f32, skip: impl Fn(PeerId) -> bool) -> Option<(PeerId, Vec2)> {
    targets
        .iter()
        .filter(|(_, id, (room, team), pos, health, .., sheltered)| health.0 > 0 && !sheltered && !skip(id.0) && foes(side, (**room, **team)) && pos.0.distance(at) <= reach)
        .map(|(_, id, _, pos, ..)| (id.0, pos.0))
        .min_by(|a, b| a.1.distance_squared(at).total_cmp(&b.1.distance_squared(at)))
}

/// Heals the fighter `id` by `amount`, up to its most: shown as a heal.
fn heal(r: &mut Reacting, id: PeerId, amount: i32) {
    if let Some(&entity) = r.ids.get(&id)
        && let Ok((.., mut health, _, mut hits, max, _)) = r.targets.get_mut(entity)
    {
        mend(&mut health, max, &mut hits, amount);
    }
}

/// The items' own doings, each tick (see the module's docs).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn tick_items(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    rifts: Res<Rifts>,
    mut log: ResMut<HitLog>,
    mut pending: ResMut<Pending>,
    mut zones: ResMut<Zones>,
    mut holders: Query<(&PlayerId, &ClassId, &Pos, (&InRoom, &Team), &Satchel, &mut ItemState), InPlay>,
    mut targets: Targets,
    decoys: Query<(Entity, &Decoy, &ClassId, &Pos, (&InRoom, &Team), Has<Dead>)>,
    mut doomed: Query<(Entity, &mut Doomed, &Pos, Has<Dead>)>,
) {
    let now = timeline.tick().0;
    let pulse = now % items::ZONE_PULSE_TICKS == 0;
    for (id, class, pos, side, satchel, mut state) in &mut holders {
        let side = (*side.0, *side.1);
        let by = |item| By { id: id.0, class: *class, side, source: Source::Item(item) };
        state.wisp_hits.retain(|&(_, _, until)| now < until);
        for (item, level) in satchel.items() {
            match item {
                Item::WispLantern => {
                    let (count, damage, orbit, again) = items::wisp_lantern(level);
                    for wisp in 0..count {
                        let at = pos.0 + wisp_offset(now as f32, wisp, count, orbit);
                        let mut struck = Vec::new();
                        let resting = |target: PeerId, state: &ItemState| state.wisp_hits.iter().any(|&(w, p, _)| w == wisp && p == target);
                        strike(&mut commands, &mut log, now, &mut targets, by(item), Blow::ability(damage, Chill::default()), |target, p| {
                            let touched = p.distance(at) <= items::WISP_REACH + PLAYER_RADIUS && !resting(target, &state);
                            if touched {
                                struck.push(target);
                            }
                            touched
                        });
                        state.wisp_hits.extend(struck.into_iter().map(|target| (wisp, target, now + again)));
                    }
                }
                Item::PowderKeg if now >= state.keg_at => {
                    let (every, damage, radius, fuse, reach) = items::powder_keg(level);
                    let Some((_, at)) = nearest_foe(&targets, side, pos.0, reach, |_| false) else { continue };
                    state.keg_at = now + every;
                    spawn_fx(&mut commands, side.0, Fx::between(FxKind::Lob(FxColor::Fire), pos.0, at, radius, now, fuse));
                    pending.0.push(Delayed { by: by(item), at, radius, damage, chill: Chill::default(), fire_at: now + fuse, color: FxColor::Fire });
                }
                Item::HoarfrostBoots if now >= state.trail.0 && pos.0.distance(state.trail.1) >= 0.8 => {
                    let (slow, lasts, every) = items::hoarfrost_boots(level);
                    state.trail = (now + every, pos.0);
                    let radius = items::FROST_PATCH_RADIUS;
                    zones.0.push(Zone { by: by(item), at: pos.0, radius, damage: 0, slow, pull: 0.0, until: now + lasts });
                    spawn_fx(&mut commands, side.0, Fx::at(FxKind::Rime, pos.0, radius, now, lasts));
                }
                Item::GravecallerRift if pulse => {
                    let mine: Vec<sim::Rift> = rifts.0.iter().filter(|(r, rift)| *r == side.0 && rift.owner == id.0).map(|(_, rift)| *rift).collect();
                    if !mine.is_empty() {
                        let blow = Blow::ability(items::gravecaller_rift(level), Chill::default());
                        strike(&mut commands, &mut log, now, &mut targets, by(item), blow, |_, p| mine.iter().any(|rift| rift.holds(p, now)));
                    }
                }
                _ => {}
            }
        }
    }
    // Zones: pull every tick, hurt and slow on the pulse.
    zones.0.retain(|zone| zone.until > now);
    for zone in &zones.0 {
        if zone.pull > 0.0 {
            for (entity, _, (room, team), pos, health, ..) in targets.iter() {
                if health.0 > 0 && foes(zone.by.side, (*room, *team)) && pos.0.distance(zone.at) <= zone.radius + PLAYER_RADIUS {
                    let next = pos.0.move_towards(zone.at, zone.pull * TICK_DT);
                    if lobby.map(room).line_walkable(pos.0, next) {
                        commands.entity(entity).insert(Pos(next));
                    }
                }
            }
        }
        if pulse {
            let chill = if zone.slow > 0.0 { Chill { slow: zone.slow, slow_ticks: items::ZONE_PULSE_TICKS + 4, root_ticks: 0 } } else { Chill::default() };
            let (at, reach) = (zone.at, zone.radius + PLAYER_RADIUS);
            strike(&mut commands, &mut log, now, &mut targets, zone.by, Blow::ability(zone.damage, chill), |_, p| p.distance(at) <= reach);
        }
    }
    // What was set to go off.
    let due: Vec<Delayed> = pending.0.iter().filter(|d| d.fire_at <= now).copied().collect();
    pending.0.retain(|d| d.fire_at > now);
    for d in due {
        blast(&mut commands, &mut log, now, &mut targets, d.by, d.at, d.radius, d.damage, d.chill, d.color);
    }
    // Decoys shatter when their time's up, or they're killed.
    for (entity, decoy, class, pos, side, dead) in &decoys {
        if now >= decoy.until || dead {
            let by = By { id: decoy.owner, class: *class, side: (*side.0, *side.1), source: Source::Item(Item::MirrorShard) };
            blast(&mut commands, &mut log, now, &mut targets, by, pos.0, decoy.radius, decoy.damage, Chill::default(), FxColor::Spirit);
            commands.entity(entity).try_despawn();
        }
    }
    // Doom bells toll.
    for (entity, doom, pos, dead) in &mut doomed {
        if now < doom.at && !dead {
            continue;
        }
        commands.entity(entity).remove::<Doomed>();
        let toll = (doom.stored as f32 * doom.share).round() as i32;
        let Ok((_, victim, ..)) = targets.get(entity) else { continue };
        if dead || toll <= 0 {
            continue;
        }
        let victim = victim.0;
        spawn_fx(&mut commands, doom.side.0, Fx::at(FxKind::Blast(FxColor::Void), pos.0, 1.2, now, 12));
        let by = By { id: doom.by, class: doom.class, side: doom.side, source: Source::Item(Item::DoomBell) };
        strike(&mut commands, &mut log, now, &mut targets, by, Blow::ability(toll, Chill::default()), |p, _| p == victim);
    }
}

/// Everything `react` needs besides the hits.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct Reacting<'w, 's> {
    commands: Commands<'w, 's>,
    timeline: Res<'w, LocalTimeline>,
    lobby: ResMut<'w, Lobby>,
    log: ResMut<'w, HitLog>,
    cast_log: ResMut<'w, CastLog>,
    pending: ResMut<'w, Pending>,
    zones: ResMut<'w, Zones>,
    targets: Targets<'w, 's>,
    holders: Query<'w, 's, (Entity, &'static PlayerId, &'static Satchel, &'static mut ItemState, &'static mut Frenzy)>,
    mobs: Query<'w, 's, (&'static Mob, Option<&'static Elite>, &'static ClassId, Option<&'static Might>)>,
    doomed: Query<'w, 's, &'static mut Doomed>,
    fighters: Query<'w, 's, (Entity, &'static PlayerId)>,
    zombies: Query<'w, 's, &'static ClassId, With<Zombie>>,
    /// Every fighter by its id, made once a tick: hits name who dealt them by id.
    ids: Local<'s, HashMap<PeerId, Entity>>,
}

/// How many rounds of hits setting off hits (bursts killing into bursts) one tick follows.
const ROUNDS: usize = 6;

/// Reacts to this tick's hits and abilities (see the module's docs).
pub(super) fn react(mut r: Reacting) {
    let now = r.timeline.tick().0;
    let ids: HashMap<PeerId, Entity> = r.fighters.iter().map(|(entity, id)| (id.0, entity)).collect();
    *r.ids = ids;
    let casts = std::mem::take(&mut r.cast_log.0);
    for cast in casts {
        on_cast(&mut r, now, cast);
    }
    for _ in 0..ROUNDS {
        let hits = std::mem::take(&mut r.log.0);
        if hits.is_empty() {
            break;
        }
        for hit in hits {
            on_hit(&mut r, now, hit);
        }
    }
    // Whatever's left past the last round goes unanswered.
    r.log.0.clear();
}

/// The entity and satchel of the fighter `id`, if it carries items.
fn holder(r: &Reacting, id: PeerId) -> Option<(Entity, Satchel)> {
    let entity = *r.ids.get(&id)?;
    r.holders.get(entity).ok().map(|(_, _, satchel, ..)| (entity, *satchel))
}

fn on_cast(r: &mut Reacting, now: u32, cast: Used) {
    let Some((holder, satchel)) = holder(r, cast.caster) else { return };
    let by = |item| By { id: cast.caster, class: cast.class, side: cast.side, source: Source::Item(item) };
    for (item, level) in satchel.items() {
        match item {
            Item::EchoStone => {
                let (damage, radius, delay) = items::echo_stone(level);
                let echo = Delayed { by: by(item), at: cast.at, radius, damage, chill: Chill::default(), fire_at: now + delay, color: FxColor::Spirit };
                arm(&mut r.commands, &mut r.pending, now, echo, FxKind::Warn(FxColor::Spirit));
            }
            Item::VoidOrb => {
                let (radius, pull, damage, lasts) = items::void_orb(level);
                let map = r.lobby.map(&cast.side.0);
                // At the cursor, within reason, on open ground.
                let at = cast.at + cast.aim.clamp_length_max(9.0);
                let at = map.nearest_walkable(Map::tile_of(at), 3).map_or(cast.at, Map::center);
                r.zones.0.push(Zone { by: by(item), at, radius, damage, slow: 0.0, pull, until: now + lasts });
                spawn_fx(&mut r.commands, cast.side.0, Fx::at(FxKind::Zone(FxColor::Void), at, radius, now, lasts));
            }
            Item::MirrorShard => {
                let (lasts, damage, radius, recharge) = items::mirror_shard(level);
                let Ok((.., mut state, _)) = r.holders.get_mut(holder) else { continue };
                if !ready(&mut state.mirror_at, now, recharge) {
                    continue;
                }
                let id = r.lobby.next_local();
                let decoy = Decoy { owner: cast.caster, until: now + lasts, damage, radius };
                r.commands.spawn((
                    Name::from("Decoy"),
                    decoy,
                    fighter(id, cast.class, cast.side.0, cast.side.1, cast.at),
                    PlayerInput::default(),
                    replicated_npc(),
                ));
                spawn_fx(&mut r.commands, cast.side.0, Fx::at(FxKind::Blast(FxColor::Spirit), cast.at, 1.0, now, 8));
            }
            Item::HeartOfWinter if matches!(cast.class.def().ability(cast.slot).map(|a| a.kind), Some(AbilityKind::Nova(_))) => {
                let (radius, slow, damage, lasts) = items::heart_of_winter(level);
                r.zones.0.push(Zone { by: by(item), at: cast.at, radius, damage, slow, pull: 0.0, until: now + lasts });
                spawn_fx(&mut r.commands, cast.side.0, Fx::at(FxKind::Zone(FxColor::Frost), cast.at, radius, now, lasts));
            }
            _ => {}
        }
    }
}

fn on_hit(r: &mut Reacting, now: u32, hit: Landed) {
    // Monsters' own: an elite's twist, a bloater's last burst, a split.
    monster_hit(r, now, &hit);
    knock_back(r, &hit);
    if let Some(holder) = holder(r, hit.attacker) {
        attacker_items(r, now, &hit, holder);
    }
    if hit.attacker != hit.victim_id
        && let Some(holder) = holder(r, hit.victim_id)
    {
        victim_items(r, now, &hit, holder);
    }
}

/// A player's auto-attack knocks a monster (a zombie, or one of the waves') back a little, away
/// from them (along a shot's flight), a bigger one less; bosses stand firm.
fn knock_back(r: &mut Reacting, hit: &Landed) {
    if hit.source != Source::Attack || hit.killed || !matches!(hit.attacker, PeerId::Netcode(_)) {
        return;
    }
    let Ok(class) = r.zombies.get(hit.victim) else { return };
    if class.def().is_boss() {
        return;
    }
    let from = r.ids.get(&hit.attacker).and_then(|e| r.targets.get(*e).ok()).map(|(.., pos, _, _, _, _, _)| pos.0);
    let dir = match hit.shot {
        Some(shot) => shot.projectile.dir,
        None => from.map_or(Vec2::ZERO, |from| (hit.at - from).normalize_or_zero()),
    };
    if dir != Vec2::ZERO {
        r.commands.entity(hit.victim).try_insert(Shoved { dir, left: KNOCKBACK / class.def().size().max(0.5) });
    }
}

/// The attacker's items: what goes off on their hits, and on their kills.
fn attacker_items(r: &mut Reacting, now: u32, hit: &Landed, (holder, satchel): (Entity, Satchel)) {
    let attack = hit.source == Source::Attack;
    let by = |item| By { id: hit.attacker, class: hit.class, side: hit.side, source: Source::Item(item) };
    // The auto-attack hits landed, for the items that go off every so many.
    let counted = attack && satchel.items().any(|(item, _)| matches!(item, Item::RimeSigil | Item::ThunderCrown));
    let hits = match r.holders.get_mut(holder) {
        Ok((.., mut state, _)) if counted => {
            state.hits += 1;
            state.hits
        }
        _ => 0,
    };
    for (item, level) in satchel.items() {
        match item {
            Item::EmberHeart if hit.killed => {
                let (damage, radius) = items::ember_heart(level);
                blast(&mut r.commands, &mut r.log, now, &mut r.targets, by(item), hit.at, radius, damage, Chill::default(), FxColor::Fire);
            }
            Item::BerserkerTotem if hit.killed => {
                let (each, most, fade) = items::berserker_totem(level);
                if let Ok((.., mut frenzy)) = r.holders.get_mut(holder) {
                    let strength = (frenzy.at(now as f32) + each).min(most);
                    *frenzy = Frenzy { from: now, until: now + fade, strength };
                }
            }
            _ if !attack => {}
            Item::StormCoil => {
                let (jumps, damage, reach) = items::storm_coil(level);
                let mut struck = vec![hit.victim_id];
                let mut from = hit.at;
                for _ in 0..jumps {
                    let Some((next, at)) = nearest_foe(&r.targets, hit.side, from, reach, |p| struck.contains(&p)) else { break };
                    spawn_fx(&mut r.commands, hit.side.0, Fx::between(FxKind::Arc, from, at, 0.0, now, 8));
                    strike(&mut r.commands, &mut r.log, now, &mut r.targets, by(item), Blow::ability(damage, Chill::default()), |p, _| p == next);
                    struck.push(next);
                    from = at;
                }
            }
            Item::RimeSigil => {
                let (every, freeze) = items::rime_sigil(level);
                if hits % every == 0 && !hit.killed {
                    let blow = Blow::ability(0, Chill { root_ticks: freeze, ..default() });
                    strike(&mut r.commands, &mut r.log, now, &mut r.targets, by(item), blow, |p, _| p == hit.victim_id);
                }
            }
            Item::ThunderCrown => {
                let (every, damage, radius, delay) = items::thunder_crown(level);
                if hits % every == 0 {
                    let bolt = Delayed { by: by(item), at: hit.at, radius, damage, chill: Chill::default(), fire_at: now + delay, color: FxColor::Storm };
                    arm(&mut r.commands, &mut r.pending, now, bolt, FxKind::Bolt);
                }
            }
            Item::Bloodfang => heal(r, hit.attacker, items::bloodfang(level)),
            Item::DoomBell if !hit.killed => {
                let (share, lasts) = items::doom_bell(level);
                match r.doomed.get_mut(hit.victim) {
                    Ok(mut doom) if doom.by == hit.attacker => doom.stored += hit.amount,
                    Ok(_) => {}
                    Err(_) => {
                        let doom = Doomed { by: hit.attacker, class: hit.class, side: hit.side, at: now + lasts, stored: hit.amount, share };
                        r.commands.entity(hit.victim).try_insert(doom);
                        spawn_fx(&mut r.commands, hit.side.0, Fx::at(FxKind::Warn(FxColor::Void), hit.at, 0.9, now, lasts));
                    }
                }
            }
            Item::MidasCoin if !hit.killed => {
                let (below, gold) = items::midas_coin(level);
                let boss = r.mobs.get(hit.victim).is_ok_and(|(_, _, class, _)| class.def().is_boss());
                let Ok((entity, id, _, pos, mut health, mut chilled, mut hits, max, _)) = r.targets.get_mut(hit.victim) else { continue };
                if boss || health.0 <= 0 || health.0 as f32 > below * max.0 as f32 || id.0 == hit.attacker {
                    continue;
                }
                // Turned to gold: a blow of all it has left.
                let (id, at, rest) = (id.0, pos.0, health.0);
                let by = (hit.attacker, hit.class, item.name());
                damage(&mut r.commands, &mut r.log, now, (entity, id, at), (&mut health, &mut chilled, &mut hits), Blow::ability(rest, Chill::default()), by, (hit.side, Source::Item(item), None));
                r.lobby.give_gold(hit.side.0.key, gold);
                spawn_fx(&mut r.commands, hit.side.0, Fx::at(FxKind::Coins, at, 1.0, now, 20));
            }
            Item::RicochetCharm => {
                let Some(shot) = hit.shot.filter(|shot| !shot.pierced) else { continue };
                let (bounces, reach, keeps) = items::ricochet_charm(level);
                let left = shot.bounces.unwrap_or(bounces);
                if left == 0 {
                    continue;
                }
                let Some((_, at)) = nearest_foe(&r.targets, hit.side, hit.at, reach, |p| p == hit.victim_id) else { continue };
                let Some(dir) = (at - hit.at).try_normalize() else { continue };
                let projectile = Projectile { origin: hit.at, dir, spawn_tick: now, crit: false, ..shot.projectile };
                let spawned = spawn_projectile(&mut r.commands, projectile, hit.side, shot.might * keeps);
                r.commands.entity(spawned).insert((Bounces(left - 1), Spares(hit.victim_id)));
            }
            Item::HydraSpear => {
                let Some(shot) = hit.shot.filter(|shot| !shot.shard && !shot.pierced) else { continue };
                let (count, keeps) = items::hydra_spear(level);
                for n in 0..count {
                    let spread = 0.9 * (n as f32 / (count - 1).max(1) as f32 - 0.5);
                    let dir = Vec2::from_angle(spread).rotate(shot.projectile.dir);
                    let projectile = Projectile { origin: hit.at, dir, spawn_tick: now, crit: false, ..shot.projectile };
                    let spawned = spawn_projectile(&mut r.commands, projectile, hit.side, shot.might * keeps);
                    r.commands.entity(spawned).insert((Shard, Bounces(0), Spares(hit.victim_id)));
                }
            }
            _ => {}
        }
    }
}

/// The victim's items: what goes off on being hit.
fn victim_items(r: &mut Reacting, now: u32, hit: &Landed, (holder, satchel): (Entity, Satchel)) {
    let Ok((_, _, (room, team), ..)) = r.targets.get(hit.victim) else { return };
    let side = (*room, *team);
    let class = r.mobs.get(hit.victim).map_or(hit.class, |(_, _, class, _)| *class);
    let by = |item| By { id: hit.victim_id, class, side, source: Source::Item(item) };
    for (item, level) in satchel.items() {
        match item {
            Item::BrambleMantle if !hit.killed => {
                let (damage, radius, recharge) = items::bramble_mantle(level);
                let Ok((.., mut state, _)) = r.holders.get_mut(holder) else { continue };
                if ready(&mut state.lash_at, now, recharge) {
                    blast(&mut r.commands, &mut r.log, now, &mut r.targets, by(item), hit.at, radius, damage, Chill::default(), FxColor::Thorn);
                }
            }
            Item::PhoenixFeather if hit.killed => {
                let (share, damage, radius, recharge) = items::phoenix_feather(level);
                let Ok((.., mut state, _)) = r.holders.get_mut(holder) else { continue };
                if !ready(&mut state.phoenix_at, now, recharge) {
                    continue;
                }
                if let Ok((entity, .., mut health, _, mut hits, max, _)) = r.targets.get_mut(hit.victim) {
                    health.0 = (max.0 as f32 * share).round() as i32;
                    hits.push(health.0, HitKind::Heal);
                    r.commands.entity(entity).remove::<Dead>();
                }
                blast(&mut r.commands, &mut r.log, now, &mut r.targets, by(item), hit.at, radius, damage, Chill::default(), FxColor::Fire);
            }
            _ => {}
        }
    }
}

/// What monsters do when they hit, or die: a vampire's drink, a frostbound's freeze; a bloater's
/// burst, a volatile's blast (or anyone's, on a Volatile wave), a splitting one's brood.
fn monster_hit(r: &mut Reacting, now: u32, hit: &Landed) {
    // Only a monster's hits have a twist (players' are `Netcode`).
    let elite = r.ids.get(&hit.attacker).and_then(|entity| r.mobs.get(*entity).ok()?.1.copied());
    match elite.map(|e| e.0) {
        Some(Affix::Vampiric) => heal(r, hit.attacker, hit.amount),
        Some(Affix::Frostbound) if !hit.killed && hit.source != Source::Trick => {
            let by = By { id: hit.attacker, class: hit.class, side: hit.side, source: Source::Trick };
            let blow = Blow::ability(0, Chill { root_ticks: brain::FROSTBOUND_ROOT_TICKS, ..default() });
            strike(&mut r.commands, &mut r.log, now, &mut r.targets, by, blow, |p, _| p == hit.victim_id);
        }
        _ => {}
    }
    if !hit.killed {
        return;
    }
    let Ok((mob, elite, class, might)) = r.mobs.get(hit.victim) else { return };
    let (class, might, elite, burst) = (*class, might_of(might), elite.map(|e| e.0), mob.burst);
    let Ok((_, _, (room, team), ..)) = r.targets.get(hit.victim) else { return };
    let side = (*room, *team);
    let by = By { id: hit.victim_id, class, side, source: Source::Trick };
    if !burst
        && let Some(Trick::Burst { damage, radius, .. }) = class.def().mob.as_ref().and_then(|m| m.tricks.iter().find(|t| matches!(t, Trick::Burst { .. })))
    {
        blast(&mut r.commands, &mut r.log, now, &mut r.targets, by, hit.at, *radius, mighty(*damage, might), Chill::default(), FxColor::Poison);
    }
    let volatile_wave = r.lobby.run(side.0.key).and_then(|run| run.omen).is_some_and(|omen| omen.bursts());
    let boom = match elite {
        Some(Affix::Volatile) => Some(brain::VOLATILE_BLAST),
        _ if volatile_wave => Some(brain::VOLATILE_WAVE_BLAST),
        _ => None,
    };
    if let Some(brain::Boom { fuse, radius, damage }) = boom {
        let blast = Delayed { by, at: hit.at, radius, damage: mighty(damage, might), chill: Chill::default(), fire_at: now + fuse, color: FxColor::Fire };
        arm(&mut r.commands, &mut r.pending, now, blast, FxKind::Warn(FxColor::Fire));
    }
    if elite == Some(Affix::Splitting)
        && let Some(brood) = ClassId::by_key(brain::SPLITS_INTO)
    {
        let map = r.lobby.map(&side.0);
        for at in crate::waves::ring_spots(map, hit.at, brain::SPLITS_INTO_COUNT, 0.8).collect::<Vec<_>>() {
            crate::waves::spawn_mob(&mut r.commands, &mut r.lobby, now, side.0, brood, at, 1.0, might, None);
        }
    }
}
