//! How the waves' monsters fight (`MobDef`): each tick, each one picks the nearest prey (a player,
//! or a decoy), gets about the way its `brain` says (straight at it, or keeping its distance), and
//! attacks once it's in reach, as a player's commands would (`PlayerInput`). Around walls it
//! follows its room's flow field (`Fields`): one search for every monster in the room, instead
//! of one each. Its `tricks` go off when they're due: what they do to others is noted as `Deed`s
//! and done right after (`do_deeds`), once the monsters have all had their think.

use std::collections::HashMap;
use std::f32::consts::TAU;

use super::*;

use arena_shared::abilities::AbilitySlot;
use arena_shared::classes::{Brain, Chill, Shell, Trick};
use arena_shared::map::{FlowField, Map};
use arena_shared::rooms::{RoomKey, ZOMBIES};

use crate::fx::spawn_fx;
use crate::items::{By, Delayed, Pending, arm, blast};
use crate::rooms::{InRoom, Lobby};

// Elites' twists (`Affix`), and a Volatile wave's.

/// How long a frostbound elite's hits freeze.
pub(crate) const FROSTBOUND_ROOT_TICKS: u32 = 16;

/// A blast where a monster died: `fuse` ticks later, `damage` (at wave 1's might) within `radius`.
pub(crate) struct Boom {
    pub(crate) fuse: u32,
    pub(crate) radius: f32,
    pub(crate) damage: i32,
}

/// A volatile elite's, and any monster's on a Volatile wave.
pub(crate) const VOLATILE_BLAST: Boom = Boom { fuse: 26, radius: 2.6, damage: 24 };
pub(crate) const VOLATILE_WAVE_BLAST: Boom = Boom { fuse: 20, radius: 2.0, damage: 14 };

/// What a splitting elite splits into as it dies, and how many.
pub(crate) const SPLITS_INTO: &str = "gnawer";
pub(crate) const SPLITS_INTO_COUNT: u32 = 3;

/// Server-only: a monster of the waves.
#[derive(Component)]
pub(crate) struct Mob {
    /// Gold for everyone in the run when it dies.
    pub(crate) bounty: u32,
    /// It has burst already (`Trick::Burst`): not again as it dies.
    pub(crate) burst: bool,
    /// When each of its tricks may go again.
    pub(crate) ready_at: Vec<u32>,
    /// Which way round its prey it closes in from, so a pack surrounds rather than stacks.
    pub(crate) flank: f32,
    pub(crate) enraged: bool,
    /// Who called it up, if it was summoned.
    pub(crate) raised_by: Option<Entity>,
    /// A kiter backs off until the first tick, and may again from the second: in short bursts,
    /// standing its ground in between, so it can be caught.
    retreat: (u32, u32),
}

impl Mob {
    pub(crate) fn new(class: ClassId, now: u32, bounty: u32, raised_by: Option<Entity>) -> Mob {
        let def = class.def().mob.as_ref();
        let tricks = def.map_or(&[][..], |m| &m.tricks[..]);
        // Each trick's first go comes a while in, not all at once.
        let ready_at = tricks.iter().map(|trick| now + trick_every(trick).map_or(0, |every| fastrand::u32(every / 3..=every))).collect();
        Mob { bounty, burst: false, ready_at, flank: fastrand::f32() * TAU, enraged: false, raised_by, retreat: (0, 0) }
    }
}

/// How often a trick that goes off now and then does.
fn trick_every(trick: &Trick) -> Option<u32> {
    match trick {
        Trick::Summon { every_ticks, .. }
        | Trick::Mend { every_ticks, .. }
        | Trick::Ring { every_ticks, .. }
        | Trick::Fan { every_ticks, .. }
        | Trick::Mortar { every_ticks, .. }
        | Trick::Blink { every_ticks, .. } => Some(*every_ticks),
        _ => None,
    }
}

/// Server-only: each room's flow field toward its prey: when it was made, and the tiles the prey
/// stood on then (made again only once they've moved).
#[derive(Resource, Default)]
pub(crate) struct Fields(HashMap<RoomKey, (u32, Vec<IVec2>, FlowField)>);

/// How far a mortar throws.
const MORTAR_REACH: f32 = 14.0;

/// Where a volley of `count` shells comes down: the first right on `target`, the rest round it
/// `scatter` out (from `angle`), evenly as far as open ground allows, each on a spot of its own: one
/// with nowhere to land isn't thrown, never two on one spot.
fn shell_spots(map: &Map, target: Vec2, count: u32, scatter: f32, angle: f32) -> Vec<Vec2> {
    let rest = count.saturating_sub(1);
    let tries = rest.max(1) * 2;
    let round: Vec<Vec2> = (0..tries).map(|n| target + Vec2::from_angle(angle + TAU * n as f32 / tries as f32) * scatter).filter(|spot| map.walkable_at(*spot)).collect();
    let step = (round.len() as f32 / rest.max(1) as f32).max(1.0);
    std::iter::once(target).chain((0..rest).filter_map(|n| round.get((n as f32 * step) as usize).copied())).collect()
}

/// What a blinking monster leaves behind (`Trick::Blink::leaves`): how far its burst reaches,
/// and how soon it goes off.
const BLINK_LEAVES_RADIUS: f32 = 2.4;
const BLINK_LEAVES_FUSE: u32 = 20;

/// How long a kiter backs off at a time, and how often it may.
const RETREAT_TICKS: u32 = 48;
const RETREAT_EVERY_TICKS: u32 = 160;

/// How often a room's flow field is made again.
const FIELD_EVERY_TICKS: u32 = 12;

/// What a monster's trick does to others: done by `do_deeds`.
pub(crate) enum Deed {
    Burst { by: By, entity: Entity, at: Vec2, radius: f32, damage: i32, color: FxColor },
    Mend { room: InRoom, at: Vec2, radius: f32, amount: i32 },
    Summon { room: InRoom, class: ClassId, count: u32, at: Vec2, might: f32, raiser: Entity },
    Shots { side: Side, shots: Vec<Projectile>, might: f32 },
    Mortar { by: By, from: Vec2, at: Vec2, radius: f32, damage: i32, fuse: u32, shell: Shell },
    /// Gone from `from`, there at `to`, leaving `leaves` damage behind (`Trick::Blink`).
    Blink { by: By, entity: Entity, from: Vec2, to: Vec2, leaves: i32 },
}

/// Server-only: deeds to do this tick.
#[derive(Resource, Default)]
pub(crate) struct Deeds(Vec<Deed>);

type Prey<'w, 's> = Query<'w, 's, (&'static Pos, &'static InRoom, &'static Team), (With<PlayerId>, Without<Zombie>, Without<Untouchable>, Without<Sheltered>, InPlay)>;

/// Each monster's think: whom it goes for, where it walks, whether it attacks, and its tricks.
#[allow(clippy::type_complexity)]
pub(super) fn think(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    mut fields: ResMut<Fields>,
    mut deeds: ResMut<Deeds>,
    mut mobs: Query<
        (Entity, &PlayerId, &ClassId, &Pos, (&InRoom, &Team), &mut PlayerInput, &Cooldowns, (&Health, &MaxHealth), &mut Mob, Option<&Might>, &mut Hasted),
        InPlay,
    >,
    prey: Prey,
) {
    let now = timeline.tick().0;
    // The rooms that have monsters in play, and their fields, fresh enough.
    let mut rooms: Vec<InRoom> = mobs.iter().map(|(_, _, _, _, (room, _), ..)| *room).collect();
    rooms.sort_by_key(|r| r.key);
    rooms.dedup();
    fields.0.retain(|key, _| rooms.iter().any(|r| r.key == *key));
    for room in &rooms {
        if fields.0.get(&room.key).is_some_and(|(at, ..)| now < at + FIELD_EVERY_TICKS) {
            continue;
        }
        let mut tiles: Vec<IVec2> = prey.iter().filter(|(_, r, _)| *r == room).map(|(p, ..)| Map::tile_of(p.0)).collect();
        tiles.sort_by_key(|t| (t.x, t.y));
        tiles.dedup();
        match fields.0.get_mut(&room.key) {
            Some((at, was, _)) if *was == tiles => *at = now,
            _ => {
                let field = lobby.map(room).flow_field(tiles.iter().map(|t| Map::center(*t)));
                fields.0.insert(room.key, (now, tiles, field));
            }
        }
    }
    // Who's summoned whom, for the summoners' caps.
    let mut raised_count: HashMap<Entity, u32> = HashMap::new();
    for (.., mob, _, _) in &mobs {
        if let Some(raiser) = mob.raised_by {
            *raised_count.entry(raiser).or_default() += 1;
        }
    }
    for (entity, id, class, pos, (room, team), mut input, cooldowns, (health, max), mut mob, might, mut hasted) in &mut mobs {
        let def = class.def();
        let Some(mob_def) = def.mob.as_ref() else { continue };
        let map = lobby.map(room);
        let might = crate::combat::might_of(might);
        let side = (*room, *team);
        let target = prey
            .iter()
            .filter(|(_, r, t)| *r == room && t.0 != ZOMBIES)
            .map(|(p, ..)| p.0)
            .min_by(|a, b| a.distance_squared(pos.0).total_cmp(&b.distance_squared(pos.0)));
        let Some(target) = target else {
            input.move_to = None;
            continue;
        };
        let to = target - pos.0;
        let dist = to.length();
        let sees = map.shot_clear(pos.0, target);
        // Where it can hit from: a little inside its reach, so it lands on prey that stands still.
        let reach = match def.attack.kind.shot() {
            Some(shot) => shot.edge_reach() * 0.85,
            None => def.attack.hit_span(false).map_or(1.0, |(_, far)| far) * 0.8,
        };
        let walk_to = |goal: Vec2| -> Option<Vec2> {
            if map.line_walkable(pos.0, goal) {
                return Some(goal);
            }
            fields.0.get(&room.key).and_then(|(.., field)| map.downhill(field, pos.0))
        };
        // Close in from its own side of the prey.
        let flanked = target + Vec2::from_angle(mob.flank) * (reach * 0.5).min(1.2);
        match mob_def.brain {
            Brain::Chase if dist <= reach && sees => {
                input.move_to = None;
                input.aim = to;
                input.fire = true;
            }
            Brain::Chase => input.move_to = walk_to(if dist < 5.0 { flanked } else { target }),
            Brain::Kite { keep } if dist < keep * 0.55 && (now < mob.retreat.0 || now >= mob.retreat.1) => {
                if now >= mob.retreat.0 {
                    mob.retreat = (now + RETREAT_TICKS, now + RETREAT_EVERY_TICKS);
                }
                // Too close: back off, sideways along a wall if it must.
                let away = (-to).normalize_or_zero();
                let back = [away, away.perp(), -away.perp()].into_iter().map(|d| pos.0 + d * 3.0).find(|p| map.walkable_at(*p) && map.line_walkable(pos.0, *p));
                input.move_to = back;
                if back.is_none() && sees {
                    input.aim = to;
                    input.fire = true;
                }
            }
            Brain::Kite { keep } if dist <= reach.max(keep) && sees => {
                input.move_to = None;
                input.aim = to;
                input.fire = dist <= reach;
                if !input.fire {
                    input.move_to = walk_to(target);
                }
            }
            Brain::Kite { .. } => input.move_to = walk_to(target),
        }
        for (i, trick) in mob_def.tricks.iter().enumerate() {
            if now < mob.ready_at.get(i).copied().unwrap_or(0) {
                continue;
            }
            let every = trick_every(trick);
            let mut fired = false;
            match trick {
                Trick::Lunge { slot, within } => {
                    let slot = AbilitySlot(*slot);
                    let ready = def.ability(slot).is_some_and(|a| now >= cooldowns.usable_at(slot, a));
                    if ready && sees && dist > 2.0 && dist < *within {
                        // Through its prey, not just up to it.
                        input.order_cast(slot, to + to.normalize_or_zero() * 1.5);
                    }
                }
                Trick::Slam { slot, within } => {
                    let slot = AbilitySlot(*slot);
                    let ready = def.ability(slot).is_some_and(|a| now >= cooldowns.usable_at(slot, a));
                    if ready && dist < *within {
                        input.order_cast(slot, to);
                    }
                }
                Trick::Burst { within, damage, radius, color } if dist < *within && !mob.burst => {
                    mob.burst = true;
                    let by = By { id: id.0, class: *class, side, source: Source::Trick };
                    deeds.0.push(Deed::Burst { by, entity, at: pos.0, radius: *radius, damage: mighty(*damage, might), color: *color });
                }
                Trick::Burst { .. } => {}
                Trick::Summon { class: kind, count, max, .. } => {
                    fired = true;
                    let up = raised_count.get(&entity).copied().unwrap_or(0);
                    if up < *max
                        && let Some(kind) = ClassId::by_key(kind)
                    {
                        let count = (*count).min(max - up);
                        deeds.0.push(Deed::Summon { room: *room, class: kind, count, at: pos.0, might, raiser: entity });
                    }
                }
                Trick::Mend { radius, amount, .. } => {
                    fired = true;
                    deeds.0.push(Deed::Mend { room: *room, at: pos.0, radius: *radius, amount: *amount });
                }
                Trick::Ring { count, .. } | Trick::Fan { count, .. } => {
                    let Some(shot) = def.attack.kind.shot().filter(|_| *count > 0) else { continue };
                    fired = true;
                    let (from, step) = match trick {
                        Trick::Fan { spread, .. } => (to.to_angle() - spread / 2.0, spread / count.saturating_sub(1).max(1) as f32),
                        _ => (mob.flank, TAU / *count as f32),
                    };
                    let shots = (0..*count)
                        .map(|n| {
                            let dir = Vec2::from_angle(from + step * n as f32);
                            Projectile { owner: id.0, class: *class, origin: pos.0 + dir * (PLAYER_RADIUS + shot.radius), dir, spawn_tick: now, ability: None, crit: false, extra: false, climb: 0.0 }
                        })
                        .collect();
                    deeds.0.push(Deed::Shots { side, shots, might });
                }
                Trick::Mortar { damage, radius, fuse_ticks, count, scatter, shell, .. } => {
                    // Only at prey it can reach with a throw.
                    if dist < MORTAR_REACH {
                        fired = true;
                        let by = By { id: id.0, class: *class, side, source: Source::Trick };
                        for at in shell_spots(map, target, *count, *scatter, mob.flank) {
                            deeds.0.push(Deed::Mortar { by, from: pos.0, at, radius: *radius, damage: mighty(*damage, might), fuse: *fuse_ticks, shell: *shell });
                        }
                    }
                }
                Trick::Blink { within, distance, leaves, .. } if dist < *within => {
                    // As far away from its prey as there's open ground for: straight away first,
                    // then turning further round, the whole way if it must, then a shorter hop.
                    // Never through a wall, nor into somewhere it can't be got at.
                    let away = (-to).normalize_or(Vec2::X);
                    let turns = (0..12).map(|n| (n as f32 / 2.0).ceil() * if n % 2 == 0 { 1.0 } else { -1.0 } * TAU / 12.0);
                    let spot = [*distance, *distance * 0.6]
                        .into_iter()
                        .flat_map(|reach| turns.clone().map(move |turn| (reach, turn)))
                        .map(|(reach, turn)| pos.0 + away.rotate(Vec2::from_angle(turn)) * reach)
                        .find(|spot| map.walkable_at(*spot) && map.line_walkable(pos.0, *spot) && spot.distance(target) > *within);
                    if let Some(to) = spot {
                        fired = true;
                        input.move_to = None;
                        input.fire = false;
                        let by = By { id: id.0, class: *class, side, source: Source::Trick };
                        deeds.0.push(Deed::Blink { by, entity, from: pos.0, to, leaves: mighty(*leaves, might) });
                    }
                }
                Trick::Blink { .. } => {}
                Trick::Enrage { below } => {
                    if !mob.enraged && (health.0 as f32) < below * max.0 as f32 {
                        mob.enraged = true;
                        hasted.0 = Span { from: now + 1, until: u32::MAX };
                        commands.entity(entity).insert(Might(might * 1.3));
                        spawn_fx(&mut commands, *room, Fx::at(FxKind::Blast(FxColor::Blood), pos.0, 3.0, now, 16));
                    }
                }
            }
            if fired && let (Some(every), Some(at)) = (every, mob.ready_at.get_mut(i)) {
                *at = now + every;
            }
            // Gone from here: its other tricks wait for it to be there (they'd go off from here).
            if fired && matches!(trick, Trick::Blink { .. }) {
                break;
            }
        }
    }
}

/// What the monsters' tricks did to others this tick (`Deed`).
#[allow(clippy::too_many_arguments)]
pub(super) fn do_deeds(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut lobby: ResMut<Lobby>,
    mut deeds: ResMut<Deeds>,
    mut log: ResMut<HitLog>,
    mut pending: ResMut<Pending>,
    mut targets: Targets,
) {
    let now = timeline.tick().0;
    for deed in std::mem::take(&mut deeds.0) {
        match deed {
            Deed::Burst { by, entity, at, radius, damage: burst, color } => {
                blast(&mut commands, &mut log, now, &mut targets, by, at, radius, burst, Chill::default(), color);
                // It's spent: it bursts itself, a blow of all it has left.
                if let Ok((_, id, _, pos, mut health, mut chilled, mut hits, ..)) = targets.get_mut(entity) {
                    let (id, at, rest) = (id.0, pos.0, health.0);
                    let blow = Blow::ability(rest, Chill::default());
                    damage(&mut commands, &mut log, now, (entity, id, at), (&mut health, &mut chilled, &mut hits), blow, (id, by.class, "a burst"), (by.side, Source::Trick, None));
                }
            }
            Deed::Mend { room, at, radius, amount } => {
                spawn_fx(&mut commands, room, Fx::at(FxKind::Mend, at, radius, now, 24));
                for (_, _, (r, team), pos, mut health, _, mut hits, max, _) in &mut targets {
                    if *r == room && team.0 == ZOMBIES && pos.0.distance(at) <= radius {
                        mend(&mut health, max, &mut hits, amount);
                    }
                }
            }
            Deed::Summon { room, class, count, at, might, raiser } => {
                for spot in crate::waves::ring_spots(lobby.map(&room), at, count, 1.6).collect::<Vec<_>>() {
                    crate::waves::spawn_mob(&mut commands, &mut lobby, now, room, class, spot, 1.0, might, Some(raiser));
                }
            }
            Deed::Mortar { by, from, at, radius, damage, fuse, shell } => {
                // What's coming shows where it'll land, until it does.
                let (warning, look) = match shell {
                    Shell::Bomb => {
                        spawn_fx(&mut commands, by.side.0, Fx::between(FxKind::Lob(FxColor::Poison), from, at, radius, now, fuse));
                        (FxKind::Warn(FxColor::Poison), FxKind::Blast(FxColor::Poison))
                    }
                    Shell::Lightning => (FxKind::Warn(FxColor::Storm), FxKind::Blast(FxColor::Storm)),
                    Shell::Meteor => (FxKind::Meteor, FxKind::Blast(FxColor::Fire)),
                };
                let shell = Delayed { by, at, radius, damage, chill: Chill::default(), fire_at: now + fuse, look };
                arm(&mut commands, &mut pending, now, shell, warning);
            }
            Deed::Blink { by, entity, from, to, leaves } => {
                commands.entity(entity).insert(Pos(to));
                spawn_fx(&mut commands, by.side.0, Fx::between(FxKind::Blink, from, to, 1.0, now, 12));
                if leaves > 0 {
                    let left = Delayed { by, at: from, radius: BLINK_LEAVES_RADIUS, damage: leaves, chill: Chill::default(), fire_at: now + BLINK_LEAVES_FUSE, look: FxKind::Blast(FxColor::Void) };
                    arm(&mut commands, &mut pending, now, left, FxKind::Warn(FxColor::Void));
                }
            }
            Deed::Shots { side, shots, might } => {
                for shot in shots {
                    spawn_projectile(&mut commands, shot, side, might);
                }
            }
        }
    }
}

/// How far apart (center to center, for monsters of their figure's size) monsters keep, and the
/// most a crowded one is nudged a tick.
const ELBOW_ROOM: f32 = 0.85;
const NUDGE: f32 = 0.06;

/// Monsters don't stand in each other: crowding ones are nudged apart (where the ground lets
/// them), so a pack spreads round its prey instead of piling into one body.
pub(super) fn spread_out(lobby: Res<Lobby>, mut mobs: Query<(&mut Pos, &InRoom, &ClassId, &Dashing), (With<Mob>, InPlay)>) {
    // By room: each is only ever crowded by those in its own.
    let mut by_room: HashMap<RoomKey, Vec<(usize, Vec2, f32)>> = HashMap::new();
    for (i, (pos, room, class, _)) in mobs.iter().enumerate() {
        by_room.entry(room.key).or_default().push((i, pos.0, class.def().size()));
    }
    for (i, (mut pos, room, class, dashing)) in mobs.iter_mut().enumerate() {
        if dashing.0.is_some() {
            continue;
        }
        let size = class.def().size();
        let mut push = Vec2::ZERO;
        for &(j, other, other_size) in by_room.get(&room.key).map_or(&[][..], |v| &v[..]) {
            if i == j {
                continue;
            }
            let apart = pos.0 - other;
            let room_for = ELBOW_ROOM * (size + other_size) / 2.0;
            if apart.length_squared() >= room_for * room_for {
                continue;
            }
            // Exactly on top: apart some way of their own.
            let away = apart.try_normalize().unwrap_or(Vec2::from_angle(i as f32));
            push += away * (room_for - apart.length());
        }
        let push = push.clamp_length_max(NUDGE);
        if push != Vec2::ZERO {
            let next = pos.0 + push;
            if lobby.map(room).line_walkable(pos.0, next) {
                pos.0 = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use arena_shared::map::MapId;

    /// A volley never lands two shells on one spot, against a wall or out in the open.
    #[test]
    fn shells_each_land_on_a_spot_of_their_own() {
        let map = MapId::Wilds.map();
        let open = map.shop.unwrap() + Vec2::new(0.0, -4.0);
        // Hard by a wall: a walkable tile with one next to it that isn't.
        let by_wall = map.tiles().map(|(t, _)| Map::center(t)).find(|p| map.walkable_at(*p) && !map.walkable_at(*p + Vec2::X)).unwrap();
        for target in [open, by_wall] {
            let spots = shell_spots(map, target, 5, 3.0, 0.4);
            assert_eq!(spots[0], target);
            assert!(spots.iter().all(|s| map.walkable_at(*s)), "a shell off open ground: {spots:?}");
            for (i, a) in spots.iter().enumerate() {
                assert!(spots[i + 1..].iter().all(|b| a.distance(*b) > 0.5), "two shells on one spot: {spots:?}");
            }
        }
        assert_eq!(shell_spots(map, open, 5, 3.0, 0.4).len(), 5, "out in the open, all of them");
        assert_eq!(shell_spots(map, open, 1, 3.0, 0.4), vec![open]);
    }
}
