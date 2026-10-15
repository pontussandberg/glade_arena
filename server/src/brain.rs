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
use arena_shared::classes::{Brain, Chill, Trick};
use arena_shared::map::FlowField;
use arena_shared::rooms::{RoomKey, ZOMBIES};

use crate::fx::spawn_fx;
use crate::items::{By, blast};
use crate::rooms::{InRoom, Lobby};

/// Server-only: a monster of the waves.
#[derive(Component)]
pub(crate) struct Mob {
    /// Gold for everyone in the run when it dies.
    pub(crate) bounty: u32,
    pub(crate) boss: bool,
    /// It has burst already (`Trick::Burst`): not again as it dies.
    pub(crate) burst: bool,
    /// When each of its tricks may go again.
    pub(crate) ready_at: Vec<u32>,
    /// Which way round its prey it closes in from, so a pack surrounds rather than stacks.
    pub(crate) flank: f32,
    pub(crate) enraged: bool,
    /// Who called it up, if it was summoned.
    pub(crate) raised_by: Option<Entity>,
}

impl Mob {
    pub(crate) fn new(class: ClassId, now: u32, bounty: u32, raised_by: Option<Entity>) -> Mob {
        let def = class.def().mob.as_ref();
        let tricks = def.map_or(&[][..], |m| &m.tricks[..]);
        // Each trick's first go comes a while in, not all at once.
        let ready_at = tricks.iter().map(|trick| now + trick_every(trick).map_or(0, |every| fastrand::u32(every / 3..=every))).collect();
        Mob { bounty, boss: def.is_some_and(|m| m.boss), burst: false, ready_at, flank: fastrand::f32() * TAU, enraged: false, raised_by }
    }
}

/// How often a trick that goes off now and then does.
fn trick_every(trick: &Trick) -> Option<u32> {
    match trick {
        Trick::Summon { every_ticks, .. } | Trick::Mend { every_ticks, .. } | Trick::Ring { every_ticks, .. } | Trick::Fan { every_ticks, .. } => Some(*every_ticks),
        _ => None,
    }
}

/// Server-only: each room's flow field toward its prey, and when it was made.
#[derive(Resource, Default)]
pub(crate) struct Fields(HashMap<RoomKey, (u32, FlowField)>);

/// How often a room's flow field is made again.
const FIELD_EVERY_TICKS: u32 = 12;

/// What a monster's trick does to others: done by `do_deeds`.
pub(crate) enum Deed {
    Burst { by: By, entity: Entity, at: Vec2, radius: f32, damage: i32 },
    Mend { room: InRoom, at: Vec2, radius: f32, amount: i32 },
    Summon { room: InRoom, class: ClassId, count: u32, at: Vec2, might: f32, raiser: Entity },
    Shots { side: Side, shots: Vec<Projectile>, might: f32 },
}

/// Server-only: deeds to do this tick.
#[derive(Resource, Default)]
pub(crate) struct Deeds(Vec<Deed>);

type Prey<'w, 's> = Query<'w, 's, (&'static Pos, &'static InRoom, &'static Team), (With<PlayerId>, Without<Zombie>, Without<Untouchable>, InPlay)>;

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
        let stale = fields.0.get(&room.key).is_none_or(|(at, _)| now >= at + FIELD_EVERY_TICKS);
        if stale {
            let sources: Vec<Vec2> = prey.iter().filter(|(_, r, _)| *r == room).map(|(p, ..)| p.0).collect();
            fields.0.insert(room.key, (now, lobby.map(room).flow_field(sources)));
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
            fields.0.get(&room.key).and_then(|(_, field)| map.downhill(field, pos.0))
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
            Brain::Kite { keep } if dist < keep * 0.55 => {
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
                Trick::Burst { within, damage, radius } if dist < *within && !mob.burst => {
                    mob.burst = true;
                    let by = By { id: id.0, class: *class, side, source: Source::Trick };
                    deeds.0.push(Deed::Burst { by, entity, at: pos.0, radius: *radius, damage: (*damage as f32 * might).round() as i32 });
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
                    fired = true;
                    let Some(shot) = def.attack.kind.shot() else { continue };
                    let (from, step) = match trick {
                        Trick::Fan { spread, .. } => (to.to_angle() - spread / 2.0, spread / (*count - 1).max(1) as f32),
                        _ => (mob.flank, TAU / *count as f32),
                    };
                    let shots = (0..*count)
                        .map(|n| {
                            let dir = Vec2::from_angle(from + step * n as f32);
                            Projectile { owner: id.0, class: *class, origin: pos.0 + dir * (PLAYER_RADIUS + shot.radius), dir, spawn_tick: now, ability: None, crit: false }
                        })
                        .collect();
                    deeds.0.push(Deed::Shots { side, shots, might });
                }
                Trick::Enrage { below } => {
                    if !mob.enraged && (health.0 as f32) < below * max.0 as f32 {
                        mob.enraged = true;
                        hasted.0 = Span { from: now + 1, until: u32::MAX };
                        commands.entity(entity).insert(Might(might * 1.3));
                        spawn_fx(&mut commands, *room, Fx { kind: FxKind::Blast(Hue::Blood), at: pos.0, to: pos.0, radius: 3.0, from: now, until: now + 16 });
                    }
                }
            }
            if fired && let (Some(every), Some(at)) = (every, mob.ready_at.get_mut(i)) {
                *at = now + every;
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
    mut targets: Targets,
    maxes: Query<&MaxHealth>,
) {
    let now = timeline.tick().0;
    for deed in std::mem::take(&mut deeds.0) {
        match deed {
            Deed::Burst { by, entity, at, radius, damage } => {
                blast(&mut commands, &mut log, now, &mut targets, by, at, radius, damage, Chill::default(), Hue::Poison);
                // It's spent: it bursts itself.
                if let Ok((_, id, _, _, mut health, _, mut hits)) = targets.get_mut(entity)
                    && health.0 > 0
                {
                    hits.push(health.0, HitKind::Damage);
                    health.0 = 0;
                    commands.entity(entity).insert(Dead { respawn_at: now + RESPAWN_TICKS, placed: false, killer: id.0 });
                }
            }
            Deed::Mend { room, at, radius, amount } => {
                spawn_fx(&mut commands, room, Fx { kind: FxKind::Blast(Hue::Poison), at, to: at, radius, from: now, until: now + 14 });
                for (entity, _, (r, team), pos, mut health, _, mut hits) in &mut targets {
                    if *r != room || team.0 != ZOMBIES || health.0 <= 0 || pos.0.distance(at) > radius {
                        continue;
                    }
                    let most = maxes.get(entity).map_or(health.0, |m| m.0);
                    let healed = (health.0 + amount).min(most);
                    if healed > health.0 {
                        hits.push(healed - health.0, HitKind::Heal);
                        health.0 = healed;
                    }
                }
            }
            Deed::Summon { room, class, count, at, might, raiser } => {
                let map = lobby.map(&room);
                for n in 0..count {
                    let spot = at + Vec2::from_angle(TAU * n as f32 / count as f32 + fastrand::f32()) * 1.6;
                    let spot = if map.walkable_at(spot) && map.line_walkable(at, spot) { spot } else { at };
                    crate::waves::spawn_mob(&mut commands, &mut lobby, room, class, spot, None, might, Some(raiser));
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
