//! What items do in a fight (`arena_shared::items`), on the server. Three passes a tick, after
//! the attacks and abilities:
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

use std::f32::consts::TAU;

use super::*;

use arena_shared::classes::{AbilityKind, Chill, Trick};
use arena_shared::items::{self, Item, Satchel, wisp_offset};
use arena_shared::map::Map;
use arena_shared::sim;

use crate::brain::Mob;
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
/// `at`, in `hue`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Delayed {
    pub(crate) by: By,
    pub(crate) at: Vec2,
    pub(crate) radius: f32,
    pub(crate) damage: i32,
    pub(crate) chill: Chill,
    pub(crate) fire_at: u32,
    pub(crate) hue: FxColor,
}

/// Server-only: blasts set to go off.
#[derive(Resource, Default)]
pub(crate) struct Pending(pub(crate) Vec<Delayed>);

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

/// Deals `damage` (and `chill`) to `by`'s foes within `radius` of `at`, shown as a blast in `hue`.
/// How many it hit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blast(commands: &mut Commands, log: &mut HitLog, now: u32, targets: &mut Targets, by: By, at: Vec2, radius: f32, damage: i32, chill: Chill, hue: FxColor) -> usize {
    spawn_fx(commands, by.side.0, Fx { kind: FxKind::Blast(hue), at, to: at, radius, from: now, until: now + 10 });
    let blow = Blow::ability(damage, chill);
    hit_foes(commands, log, now, targets, (by.id, by.class, by.name()), (by.side, by.source), blow, |_, p| p.distance(at) <= radius + PLAYER_RADIUS)
}

/// The foe of `side` nearest `at` within `reach`, alive, other than those `skip` says: who and where.
fn nearest_foe(targets: &Targets, side: Side, at: Vec2, reach: f32, skip: impl Fn(PeerId) -> bool) -> Option<(PeerId, Vec2)> {
    targets
        .iter()
        .filter(|(_, id, (room, team), pos, health, ..)| health.0 > 0 && !skip(id.0) && foes(side, (**room, **team)) && pos.0.distance(at) <= reach)
        .map(|(_, id, _, pos, ..)| (id.0, pos.0))
        .min_by(|a, b| a.1.distance_squared(at).total_cmp(&b.1.distance_squared(at)))
}

/// Heals the fighter `id` by `amount`, up to its most: shown as a heal.
fn heal(targets: &mut Targets, maxes: &Query<&MaxHealth>, id: PeerId, amount: i32) {
    if let Some((entity, _, _, _, mut health, _, mut hits)) = targets.iter_mut().find(|(_, p, ..)| p.0 == id) {
        let most = maxes.get(entity).map_or(health.0, |m| m.0);
        let healed = (health.0 + amount).min(most);
        if health.0 > 0 && healed > health.0 {
            hits.push(healed - health.0, HitKind::Heal);
            health.0 = healed;
        }
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
        for (item, level) in satchel.items() {
            match item {
                Item::WispLantern => {
                    let (count, damage, orbit, again) = items::wisp_lantern(level);
                    for wisp in 0..count {
                        let at = pos.0 + wisp_offset(now as f32, wisp, count, orbit);
                        let mut struck = Vec::new();
                        let ready = |target: PeerId, state: &ItemState| !state.wisp_hits.iter().any(|&(w, p, until)| w == wisp && p == target && now < until);
                        let blow = Blow::ability(damage, Chill::default());
                        let by = by(item);
                        hit_foes(&mut commands, &mut log, now, &mut targets, (by.id, by.class, item.name()), (side, by.source), blow, |target, p| {
                            let touched = p.distance(at) <= items::WISP_REACH + PLAYER_RADIUS && ready(target, &state);
                            if touched {
                                struck.push(target);
                            }
                            touched
                        });
                        state.wisp_hits.retain(|&(_, _, until)| now < until);
                        state.wisp_hits.extend(struck.into_iter().map(|target| (wisp, target, now + again)));
                    }
                }
                Item::PowderKeg if now >= state.keg_at => {
                    let (every, damage, radius, fuse, reach) = items::powder_keg(level);
                    let Some((_, at)) = nearest_foe(&targets, side, pos.0, reach, |_| false) else { continue };
                    state.keg_at = now + every;
                    spawn_fx(&mut commands, side.0, Fx { kind: FxKind::Lob, at: pos.0, to: at, radius, from: now, until: now + fuse });
                    pending.0.push(Delayed { by: by(item), at, radius, damage, chill: Chill::default(), fire_at: now + fuse, hue: FxColor::Fire });
                }
                Item::HoarfrostBoots if now >= state.trail.0 && pos.0.distance(state.trail.1) >= 0.8 => {
                    let (slow, damage, lasts, every) = items::hoarfrost_boots(level);
                    state.trail = (now + every, pos.0);
                    let radius = items::FROST_PATCH_RADIUS;
                    zones.0.push(Zone { by: by(item), at: pos.0, radius, damage, slow, pull: 0.0, until: now + lasts });
                    spawn_fx(&mut commands, side.0, Fx { kind: FxKind::Zone(FxColor::Frost), at: pos.0, to: pos.0, radius, from: now, until: now + lasts });
                }
                Item::GravecallerRift if pulse => {
                    let damage = items::gravecaller_rift(level);
                    let mine: Vec<sim::Rift> = rifts.0.iter().filter(|(r, rift)| *r == side.0 && rift.owner == id.0).map(|(_, rift)| *rift).collect();
                    if mine.is_empty() {
                        continue;
                    }
                    let by = by(item);
                    let blow = Blow::ability(damage, Chill::default());
                    hit_foes(&mut commands, &mut log, now, &mut targets, (by.id, by.class, item.name()), (side, by.source), blow, |_, p| {
                        mine.iter().any(|rift| rift.holds(p, now))
                    });
                }
                _ => {}
            }
        }
    }
    // Zones: pull every tick, hurt and slow on the pulse.
    zones.0.retain(|zone| zone.until > now);
    for zone in zones.0.clone() {
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
            let blow = Blow::ability(zone.damage, chill);
            let by = zone.by;
            hit_foes(&mut commands, &mut log, now, &mut targets, (by.id, by.class, by.name()), (by.side, by.source), blow, |_, p| {
                p.distance(zone.at) <= zone.radius + PLAYER_RADIUS
            });
        }
    }
    // What was set to go off.
    let due: Vec<Delayed> = pending.0.iter().filter(|d| d.fire_at <= now).copied().collect();
    pending.0.retain(|d| d.fire_at > now);
    for d in due {
        blast(&mut commands, &mut log, now, &mut targets, d.by, d.at, d.radius, d.damage, d.chill, d.hue);
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
        if dead || toll <= 0 {
            continue;
        }
        let Ok((_, victim, ..)) = targets.get(entity) else { continue };
        let victim = victim.0;
        spawn_fx(&mut commands, doom.side.0, Fx { kind: FxKind::Blast(FxColor::Void), at: pos.0, to: pos.0, radius: 1.2, from: now, until: now + 12 });
        let by = By { id: doom.by, class: doom.class, side: doom.side, source: Source::Item(Item::DoomBell) };
        hit_foes(&mut commands, &mut log, now, &mut targets, (by.id, by.class, by.name()), (by.side, by.source), Blow::ability(toll, Chill::default()), |p, _| p == victim);
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
    maxes: Query<'w, 's, &'static MaxHealth>,
    holders: Query<'w, 's, (&'static PlayerId, &'static Satchel, &'static mut ItemState, &'static mut Frenzy, &'static Team)>,
    mobs: Query<'w, 's, (&'static Mob, Option<&'static Elite>, &'static ClassId, Option<&'static Might>)>,
    doomed: Query<'w, 's, &'static mut Doomed>,
}

/// How many rounds of hits setting off hits (bursts killing into bursts) one tick follows.
const ROUNDS: usize = 6;

/// Reacts to this tick's hits and abilities (see the module's docs).
pub(super) fn react(mut r: Reacting) {
    let now = r.timeline.tick().0;
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

fn on_cast(r: &mut Reacting, now: u32, cast: Used) {
    let Some((_, satchel, mut state, ..)) = r.holders.iter_mut().find(|(p, ..)| p.0 == cast.caster) else { return };
    let satchel = *satchel;
    let by = |item| By { id: cast.caster, class: cast.class, side: cast.side, source: Source::Item(item) };
    for (item, level) in satchel.items() {
        match item {
            Item::EchoStone => {
                let (damage, radius, delay) = items::echo_stone(level);
                spawn_fx(&mut r.commands, cast.side.0, Fx { kind: FxKind::Warn(FxColor::Spirit), at: cast.at, to: cast.at, radius, from: now, until: now + delay });
                r.pending.0.push(Delayed { by: by(item), at: cast.at, radius, damage, chill: Chill::default(), fire_at: now + delay, hue: FxColor::Spirit });
            }
            Item::VoidOrb => {
                let (radius, pull, damage, lasts) = items::void_orb(level);
                let map = r.lobby.map(&cast.side.0);
                // At the cursor, within reason, on open ground.
                let at = cast.at + cast.aim.clamp_length_max(9.0);
                let at = map.nearest_walkable(Map::tile_of(at), 3).map_or(cast.at, Map::center);
                r.zones.0.push(Zone { by: by(item), at, radius, damage, slow: 0.0, pull, until: now + lasts });
                spawn_fx(&mut r.commands, cast.side.0, Fx { kind: FxKind::Zone(FxColor::Void), at, to: at, radius, from: now, until: now + lasts });
            }
            Item::MirrorShard if now >= state.mirror_at => {
                let (lasts, damage, radius, recharge) = items::mirror_shard(level);
                state.mirror_at = now + recharge;
                let id = r.lobby.next_local();
                let decoy = Decoy { owner: cast.caster, until: now + lasts, damage, radius };
                r.commands.spawn((
                    Name::from("Decoy"),
                    decoy,
                    fighter(id, cast.class, cast.side.0, cast.side.1, cast.at),
                    PlayerInput::default(),
                    Replicate::to_clients(NetworkTarget::All),
                    InterpolationTarget::to_clients(NetworkTarget::All),
                ));
                spawn_fx(&mut r.commands, cast.side.0, Fx { kind: FxKind::Blast(FxColor::Spirit), at: cast.at, to: cast.at, radius: 1.0, from: now, until: now + 8 });
            }
            Item::HeartOfWinter if matches!(cast.class.def().ability(cast.slot).map(|a| a.kind), Some(AbilityKind::Nova(_))) => {
                let (radius, slow, damage, lasts) = items::heart_of_winter(level);
                r.zones.0.push(Zone { by: by(item), at: cast.at, radius, damage, slow, pull: 0.0, until: now + lasts });
                spawn_fx(&mut r.commands, cast.side.0, Fx { kind: FxKind::Zone(FxColor::Frost), at: cast.at, to: cast.at, radius, from: now, until: now + lasts });
            }
            _ => {}
        }
    }
}

fn on_hit(r: &mut Reacting, now: u32, hit: Landed) {
    let attack = hit.source == Source::Attack;
    // Monsters' own: an elite's twist, a bloater's last burst, a split.
    monster_hit(r, now, &hit);
    // The attacker's items: what goes off on their hits, and on their kills.
    if let Some((_, satchel, ..)) = r.holders.iter().find(|(p, ..)| p.0 == hit.attacker) {
        let satchel = *satchel;
        let by = |item| By { id: hit.attacker, class: hit.class, side: hit.side, source: Source::Item(item) };
        for (item, level) in satchel.items() {
            match item {
                Item::EmberHeart if hit.killed => {
                    let (damage, radius) = items::ember_heart(level);
                    blast(&mut r.commands, &mut r.log, now, &mut r.targets, by(item), hit.at, radius, damage, Chill::default(), FxColor::Fire);
                }
                Item::BerserkerTotem if hit.killed => {
                    let (each, most, fade) = items::berserker_totem(level);
                    if let Some((.., mut frenzy, _)) = r.holders.iter_mut().find(|(p, ..)| p.0 == hit.attacker) {
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
                        spawn_fx(&mut r.commands, hit.side.0, Fx { kind: FxKind::Arc, at: from, to: at, radius: 0.0, from: now, until: now + 8 });
                        let by = by(item);
                        hit_foes(&mut r.commands, &mut r.log, now, &mut r.targets, (by.id, by.class, item.name()), (by.side, by.source), Blow::ability(damage, Chill::default()), |p, _| p == next);
                        struck.push(next);
                        from = at;
                    }
                }
                Item::RimeSigil | Item::ThunderCrown => {
                    let count = r.holders.iter_mut().find(|(p, ..)| p.0 == hit.attacker).map_or(0, |(_, _, mut state, ..)| {
                        if item == Item::RimeSigil {
                            state.hits += 1;
                        }
                        state.hits
                    });
                    if item == Item::RimeSigil {
                        let (every, freeze) = items::rime_sigil(level);
                        if count % every == 0 && !hit.killed {
                            let by = by(item);
                            hit_foes(&mut r.commands, &mut r.log, now, &mut r.targets, (by.id, by.class, item.name()), (by.side, by.source), Blow::ability(0, Chill { root_ticks: freeze, ..default() }), |p, _| p == hit.victim_id);
                        }
                    } else {
                        // Counted with the sigil's count if both are carried (it's the same hits).
                        let count = if satchel.level(Item::RimeSigil) > 0 { count } else {
                            r.holders.iter_mut().find(|(p, ..)| p.0 == hit.attacker).map_or(0, |(_, _, mut state, ..)| {
                                state.hits += 1;
                                state.hits
                            })
                        };
                        let (every, damage, radius, delay) = items::thunder_crown(level);
                        if count % every == 0 {
                            spawn_fx(&mut r.commands, hit.side.0, Fx { kind: FxKind::Bolt, at: hit.at, to: hit.at, radius, from: now, until: now + delay });
                            r.pending.0.push(Delayed { by: by(item), at: hit.at, radius, damage, chill: Chill::default(), fire_at: now + delay, hue: FxColor::Storm });
                        }
                    }
                }
                Item::Bloodfang => heal(&mut r.targets, &r.maxes, hit.attacker, items::bloodfang(level)),
                Item::DoomBell if !hit.killed => {
                    let (share, lasts) = items::doom_bell(level);
                    match r.doomed.get_mut(hit.victim) {
                        Ok(mut doom) if doom.by == hit.attacker => doom.stored += hit.amount,
                        Ok(_) => {}
                        Err(_) => {
                            let doom = Doomed { by: hit.attacker, class: hit.class, side: hit.side, at: now + lasts, stored: hit.amount, share };
                            r.commands.entity(hit.victim).try_insert(doom);
                            spawn_fx(&mut r.commands, hit.side.0, Fx { kind: FxKind::Warn(FxColor::Void), at: hit.at, to: hit.at, radius: 0.9, from: now, until: now + lasts });
                        }
                    }
                }
                Item::MidasCoin if !hit.killed => {
                    let (below, gold) = items::midas_coin(level);
                    let boss = r.mobs.get(hit.victim).is_ok_and(|(mob, ..)| mob.boss);
                    let most = r.maxes.get(hit.victim).map_or(1, |m| m.0);
                    let Ok((entity, id, _, _, mut health, _, mut hits)) = r.targets.get_mut(hit.victim) else { continue };
                    if boss || health.0 <= 0 || health.0 as f32 > below * most as f32 || id.0 == hit.attacker {
                        continue;
                    }
                    let amount = health.0;
                    health.0 = 0;
                    hits.push(amount, HitKind::Crit);
                    r.commands.entity(entity).insert(Dead { respawn_at: now + RESPAWN_TICKS, placed: false, killer: hit.attacker });
                    r.lobby.give_gold(hit.side.0.key, gold);
                    spawn_fx(&mut r.commands, hit.side.0, Fx { kind: FxKind::Coins, at: hit.at, to: hit.at, radius: 1.0, from: now, until: now + 20 });
                    r.log.0.push(Landed { amount, killed: true, source: Source::Item(item), shot: None, ..hit });
                }
                Item::RicochetCharm => {
                    let Some(shot) = hit.shot else { continue };
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
                    let Some(shot) = hit.shot.filter(|shot| !shot.shard) else { continue };
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
    // The victim's items: what goes off on being hit.
    if hit.attacker == hit.victim_id {
        return;
    }
    let Some((_, satchel, ..)) = r.holders.iter().find(|(p, ..)| p.0 == hit.victim_id) else { return };
    let satchel = *satchel;
    let Ok((_, _, (room, team), ..)) = r.targets.get(hit.victim) else { return };
    let side = (*room, *team);
    let victim_class = r.mobs.get(hit.victim).map(|(_, _, c, _)| *c).ok();
    let class = victim_class.unwrap_or(hit.class);
    let by = |item| By { id: hit.victim_id, class, side, source: Source::Item(item) };
    for (item, level) in satchel.items() {
        match item {
            Item::BrambleMantle if !hit.killed => {
                let ready = r.holders.iter_mut().find(|(p, ..)| p.0 == hit.victim_id).is_some_and(|(_, _, mut state, ..)| {
                    let ready = now >= state.lash_at;
                    if ready {
                        state.lash_at = now + items::bramble_mantle(level).2;
                    }
                    ready
                });
                if ready {
                    let (damage, radius, _) = items::bramble_mantle(level);
                    blast(&mut r.commands, &mut r.log, now, &mut r.targets, by(item), hit.at, radius, damage, Chill::default(), FxColor::Thorn);
                }
            }
            Item::PhoenixFeather if hit.killed => {
                let (share, damage, radius, recharge) = items::phoenix_feather(level);
                let ready = r.holders.iter_mut().find(|(p, ..)| p.0 == hit.victim_id).is_some_and(|(_, _, mut state, ..)| {
                    let ready = now >= state.phoenix_at;
                    if ready {
                        state.phoenix_at = now + recharge;
                    }
                    ready
                });
                if !ready {
                    continue;
                }
                let most = r.maxes.get(hit.victim).map_or(1, |m| m.0);
                if let Ok((entity, _, _, _, mut health, _, mut hits)) = r.targets.get_mut(hit.victim) {
                    health.0 = (most as f32 * share).round() as i32;
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
/// burst, a volatile's blast, a splitting one's brood.
fn monster_hit(r: &mut Reacting, now: u32, hit: &Landed) {
    let attacker = r.targets.iter().find(|(_, id, ..)| id.0 == hit.attacker).map(|(entity, ..)| entity);
    let elite = attacker.and_then(|entity| r.mobs.get(entity).ok()).and_then(|(_, elite, ..)| elite.copied());
    if let Some(elite) = elite {
        match elite.0 {
            Affix::Vampiric => heal(&mut r.targets, &r.maxes, hit.attacker, hit.amount),
            Affix::Frostbound if !hit.killed && hit.source != Source::Trick => {
                let by = By { id: hit.attacker, class: hit.class, side: hit.side, source: Source::Trick };
                hit_foes(&mut r.commands, &mut r.log, now, &mut r.targets, (by.id, by.class, "frost"), (by.side, by.source), Blow::ability(0, Chill { root_ticks: 16, ..default() }), |p, _| p == hit.victim_id);
            }
            _ => {}
        }
    }
    if !hit.killed {
        return;
    }
    let Ok((mob, elite, class, might)) = r.mobs.get(hit.victim) else { return };
    let (class, might, elite) = (*class, might_of(might), elite.map(|e| e.0));
    let Ok((_, _, (room, team), ..)) = r.targets.get(hit.victim) else { return };
    let side = (*room, *team);
    let by = By { id: hit.victim_id, class, side, source: Source::Trick };
    if !mob.burst
        && let Some(Trick::Burst { damage, radius, .. }) = class.def().mob.as_ref().and_then(|m| m.tricks.iter().find(|t| matches!(t, Trick::Burst { .. })))
    {
        blast(&mut r.commands, &mut r.log, now, &mut r.targets, by, hit.at, *radius, (*damage as f32 * might).round() as i32, Chill::default(), FxColor::Poison);
    }
    let volatile_wave = r.lobby.run(side.0.key).and_then(|run| run.omen) == Some(arena_shared::rooms::Omen::Volatile);
    match elite {
        _ if volatile_wave && elite != Some(Affix::Volatile) => {
            let (fuse, radius) = (20, 2.0);
            spawn_fx(&mut r.commands, side.0, Fx { kind: FxKind::Warn(FxColor::Fire), at: hit.at, to: hit.at, radius, from: now, until: now + fuse });
            r.pending.0.push(Delayed { by, at: hit.at, radius, damage: (14.0 * might).round() as i32, chill: Chill::default(), fire_at: now + fuse, hue: FxColor::Fire });
        }
        Some(Affix::Volatile) => {
            let fuse = 26;
            let radius = 2.6;
            spawn_fx(&mut r.commands, side.0, Fx { kind: FxKind::Warn(FxColor::Fire), at: hit.at, to: hit.at, radius, from: now, until: now + fuse });
            r.pending.0.push(Delayed { by, at: hit.at, radius, damage: (24.0 * might).round() as i32, chill: Chill::default(), fire_at: now + fuse, hue: FxColor::Fire });
        }
        Some(Affix::Splitting) => {
            if let Some(brood) = ClassId::by_key("gnawer") {
                for n in 0..3 {
                    let at = hit.at + Vec2::from_angle(n as f32 * TAU / 3.0) * 0.8;
                    crate::waves::spawn_mob(&mut r.commands, &mut r.lobby, side.0, brood, at, None, might, None);
                }
            }
        }
        _ => {}
    }
}
