//! The waves mode: a room's party against endless waves of monsters (`Run`). A run opens with
//! time at the shop; then wave after wave comes out of the map's lairs, each bigger than the
//! last, newer and nastier kinds joining in as they go, a boss leading every `BOSS_EVERY`th, a
//! few elites (`Elite`) among the rest, and chests turning up out in the wilds. Clearing a wave
//! pays everyone, heals and brings back the fallen, and opens the shop again until the next.
//!
//! Every kill pays its bounty to the whole party, so no one steals a kill. A fallen player comes
//! back at once while the party has lives; with none left, they wait for the wave to be cleared,
//! and if the whole party is down, the run is over. A boss killed is a life more.
//!
//! How hard a wave is, and what it's worth, is all in the tuning below.

use std::f32::consts::TAU;

use super::*;

use arena_shared::config::TICK_HZ;
use arena_shared::items::Satchel;
use arena_shared::rooms::{Omen, Phase, RoomKey, ZOMBIES};

use crate::brain::Mob;
use crate::fx::spawn_fx;
use crate::rooms::{InRoom, Lobby};

// Tuning.

/// Gold each player starts a run with, and each wave cleared pays (`WAVE_PAY` and more for each
/// wave after the first).
pub(crate) const START_GOLD: u32 = 300;
const WAVE_PAY: u32 = 60;
const WAVE_PAY_GROWTH: u32 = 15;
/// What a chest holds, for everyone, and more each wave.
const CHEST_GOLD: u32 = 40;
const CHEST_GOLD_GROWTH: u32 = 10;
/// How long the shop is open before the first wave, and between waves; and how soon the next
/// wave comes once everyone's ready.
const FIRST_SHOP_TICKS: u32 = 25 * TICK_HZ as u32;
const SHOP_TICKS: u32 = 30 * TICK_HZ as u32;
const READY_TICKS: u32 = 3 * TICK_HZ as u32;
/// Lives a run starts with; a boss killed gives one more.
const START_LIVES: u32 = 3;
/// How long a player coming back can't be hurt (nor is gone for), so they're not caught at once,
/// and how far round them monsters are flung back as they do.
const SHELTER_TICKS: u32 = 3 * TICK_HZ as u32;
const REBIRTH_REACH: f32 = 6.0;
/// How much of their health players in the camp get back a second, in a run, and how often.
const CAMP_MENDS: f32 = 0.04;
const MEND_EVERY_TICKS: u32 = TICK_HZ as u32 / 2;
/// How long a fallen player with a life to spend waits.
const COMEBACK_TICKS: u32 = 5 * TICK_HZ as u32;
/// Every how many waves a boss leads one.
pub(crate) const BOSS_EVERY: u32 = 5;
/// How long a lost run's end shows before everyone's back in the lobby.
const OVER_SHOWS: std::time::Duration = std::time::Duration::from_secs(8);
/// How near the party a lair may be to send out monsters, and how far (if any are between).
const LAIR_NEAREST: f32 = 18.0;
const LAIR_FARTHEST: f32 = 42.0;

/// A wave's budget: how much of it there is to spawn, by what its monsters cost.
fn budget(wave: u32, party: usize) -> f32 {
    let w = wave as f32;
    (3.0 + 1.8 * w + 0.07 * w * w) * (1.0 + 0.55 * (party.max(1) - 1) as f32)
}

/// How much tougher than its class a monster is on `wave`, with `party` players.
fn toughness(wave: u32, party: usize) -> f32 {
    let w = (wave - 1) as f32;
    (1.0 + 0.09 * w + 0.002 * w * w) * (1.0 + 0.3 * (party.max(1) - 1) as f32)
}

/// How much harder than its class a monster hits on `wave`: softer at first, then harder.
fn might(wave: u32) -> f32 {
    0.75 + 0.04 * (wave - 1) as f32
}

/// How much tougher a boss is: by the party, and by how many times round the bosses have come.
fn boss_toughness(wave: u32, party: usize) -> f32 {
    let round = (wave / BOSS_EVERY - 1) / bosses().len().max(1) as u32;
    (1.0 + 0.45 * (party.max(1) - 1) as f32) * (1.0 + 0.8 * round as f32) * (1.0 + 0.03 * wave.saturating_sub(BOSS_EVERY) as f32)
}

/// The odds that a monster on `wave` comes as an elite.
fn elite_odds(wave: u32) -> f32 {
    ((wave as f32 - 5.0) * 0.02).clamp(0.0, 0.3)
}

/// How often a pack comes out, and how many may be about at once.
fn spawn_every(wave: u32) -> u32 {
    48u32.saturating_sub(wave).max(14)
}
fn most_about(wave: u32, party: usize) -> usize {
    (14 + wave as usize + 4 * (party.max(1) - 1)).min(44)
}

/// What an elite is, against its kind.
const ELITE_HP: f32 = 2.5;
const ELITE_MIGHT: f32 = 1.3;
const ELITE_BOUNTY: u32 = 3;

/// The bosses, in the order they come.
fn bosses() -> Vec<ClassId> {
    let mut bosses: Vec<ClassId> = ClassId::all().filter(|c| c.def().mob.as_ref().is_some_and(|m| m.boss)).collect();
    bosses.sort_by_key(|c| c.def().mob.as_ref().map_or(0, |m| m.from_wave));
    bosses
}

/// The boss leading `wave`, if it's a boss wave.
pub(crate) fn boss_of(wave: u32) -> Option<ClassId> {
    let bosses = bosses();
    (wave % BOSS_EVERY == 0 && !bosses.is_empty()).then(|| bosses[((wave / BOSS_EVERY - 1) as usize) % bosses.len()])
}

/// A room's run, on the server.
#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) wave: u32,
    pub(crate) phase: RunPhase,
    pub(crate) lives: u32,
    /// What's still to spawn this wave, and when the next pack may.
    budget: f32,
    next_pack_at: u32,
    /// The boss of this wave is out (or it has none).
    boss_out: bool,
    /// Who's ready for the next wave (`Member::guest_id`).
    pub(crate) ready: Vec<u32>,
    /// Who's down until the wave's cleared (`Member::guest_id`).
    pub(crate) waiting: Vec<u32>,
    /// Monsters about, as last counted.
    pub(crate) alive: u32,
    /// Chests lying out this wave.
    chests: Vec<Entity>,
    /// The seconds the shop's countdown last showed.
    pub(crate) shown_secs: u32,
    /// What's strange about this wave (rolled as the shop opens before it).
    pub(crate) omen: Option<Omen>,
    /// Lives bought from the merchant this run.
    pub(crate) lives_bought: u32,
}

/// From which wave omens may come, and how likely one is.
const OMENS_FROM: u32 = 4;
const OMEN_ODDS: f32 = 0.5;
/// The odds that a monster dying on a Restless wave rises again.
const RISE_ODDS: f32 = 0.3;

/// Whatever's strange about `wave`, if anything: never the first few, nor a boss's.
fn roll_omen(wave: u32) -> Option<Omen> {
    (wave >= OMENS_FROM && boss_of(wave).is_none() && fastrand::f32() < OMEN_ODDS).then(|| Omen::ALL[fastrand::usize(..Omen::ALL.len())])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunPhase {
    /// Shopping until this tick.
    Shop { until: u32 },
    Fight,
    /// Everyone fell on this wave.
    Over { reached: u32 },
}

impl Run {
    pub(crate) fn new(now: u32) -> Run {
        Run {
            wave: 1,
            phase: RunPhase::Shop { until: now + FIRST_SHOP_TICKS },
            lives: START_LIVES,
            budget: 0.0,
            next_pack_at: 0,
            boss_out: false,
            ready: Vec::new(),
            waiting: Vec::new(),
            alive: 0,
            chests: Vec::new(),
            shown_secs: 0,
            omen: None,
            lives_bought: 0,
        }
    }

    /// Monsters are about (or may come): the run's on and not lost.
    pub(crate) fn on(&self) -> bool {
        !matches!(self.phase, RunPhase::Over { .. })
    }

    pub(crate) fn phase_view(&self, now: u32) -> Phase {
        match self.phase {
            RunPhase::Shop { until } => Phase::Shop { secs_left: until.saturating_sub(now).div_ceil(TICK_HZ as u32) },
            RunPhase::Fight => Phase::Fight,
            RunPhase::Over { reached } => Phase::Over { reached },
        }
    }

    /// Monsters still to come this wave, and about, as players see them (by what's left to spend).
    pub(crate) fn enemies_left(&self) -> u32 {
        self.alive + (self.budget.max(0.0) / 1.2).ceil() as u32 + u32::from(!self.boss_out)
    }

    /// The chests lying out, for whoever ends the run (or the room) to clear away.
    pub(crate) fn take_chests(&mut self) -> Vec<Entity> {
        std::mem::take(&mut self.chests)
    }

    /// The gold a player joining now starts with: what they'd have earned by now, more or less.
    pub(crate) fn catch_up_gold(&self) -> u32 {
        START_GOLD + (1..self.wave).map(|w| WAVE_PAY + WAVE_PAY_GROWTH * w + 5 * budget(w, 1) as u32).sum::<u32>()
    }
}

/// A monster of `class` rising at `at` in `room`: as tough as `hp_scale` makes it, hitting as hard
/// as `might`, an elite with `elite`, called up by `raised_by` if it was summoned.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_mob(commands: &mut Commands, lobby: &mut Lobby, room: InRoom, class: ClassId, at: Vec2, hp_scale: Option<f32>, might: f32, raised_by: Option<Entity>) -> Entity {
    spawn_mob_as(commands, lobby, room, class, at, hp_scale.unwrap_or(1.0), might, raised_by, None)
}

#[allow(clippy::too_many_arguments)]
fn spawn_mob_as(commands: &mut Commands, lobby: &mut Lobby, room: InRoom, class: ClassId, at: Vec2, hp_scale: f32, might: f32, raised_by: Option<Entity>, elite: Option<Affix>) -> Entity {
    let def = class.def();
    let hp_scale = hp_scale * if elite.is_some() { ELITE_HP } else { 1.0 };
    let might = might * if elite.is_some() { ELITE_MIGHT } else { 1.0 };
    let hp = (def.max_hp as f32 * hp_scale).round().max(1.0) as i32;
    let bounty = def.mob.as_ref().map_or(0, |m| m.bounty) * if elite.is_some() { ELITE_BOUNTY } else { 1 };
    let id = lobby.next_local();
    let now = lobby.now();
    let mut mob = commands.spawn((
        Name::from(def.name.clone()),
        Zombie,
        Mob::new(class, now, bounty, raised_by),
        fighter(id, class, room, Team(ZOMBIES), at),
        Might(might),
        PlayerInput::default(),
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::All),
    ));
    // As tough as the wave makes it, over its class's.
    mob.insert((Health(hp), MaxHealth(hp)));
    if let Some(affix) = elite {
        mob.insert(Elite(affix));
    }
    // Swift, or every one on a Frenzy wave (bosses aside).
    let frenzy = lobby.run(room.key).and_then(|run| run.omen) == Some(Omen::Frenzy) && !def.mob.as_ref().is_some_and(|m| m.boss);
    if elite == Some(Affix::Swift) || frenzy {
        mob.insert(Hasted(Span { from: now + 1, until: u32::MAX }));
    }
    let entity = mob.id();
    spawn_fx(commands, room, Fx { kind: FxKind::Rise, at, to: at, radius: 0.8 * def.size(), from: now, until: now + 16 });
    entity
}

/// The monsters that may come in `wave` (not bosses, not what only comes summoned), with how
/// likely each is: the newest kinds most.
fn roster(wave: u32) -> Vec<(ClassId, f32)> {
    ClassId::all()
        .filter_map(|c| {
            let mob = c.def().mob.as_ref()?;
            if mob.boss || mob.from_wave > wave {
                return None;
            }
            let fresh = wave - mob.from_wave < 3;
            Some((c, if fresh { 1.5 } else { 1.0 }))
        })
        .collect()
}

fn pick(roster: &[(ClassId, f32)], budget: f32) -> Option<ClassId> {
    let fits: Vec<&(ClassId, f32)> = roster.iter().filter(|(c, _)| c.def().mob.as_ref().is_some_and(|m| m.cost * m.pack as f32 <= budget + 0.5)).collect();
    let fits = if fits.is_empty() { roster.iter().collect() } else { fits };
    let total: f32 = fits.iter().map(|(_, w)| w).sum();
    let mut roll = fastrand::f32() * total;
    for (class, weight) in &fits {
        roll -= weight;
        if roll <= 0.0 {
            return Some(*class);
        }
    }
    fits.last().map(|(c, _)| *c)
}

/// A lair to come out of: one well away from the party but not across the map, if there is one;
/// else the farthest.
fn pick_lair(lairs: &[Vec2], party: &[Vec2]) -> Option<Vec2> {
    let nearest = |lair: Vec2| party.iter().map(|p| p.distance(lair)).fold(f32::MAX, f32::min);
    let fine: Vec<Vec2> = lairs.iter().copied().filter(|&l| (LAIR_NEAREST..LAIR_FARTHEST).contains(&nearest(l))).collect();
    if !fine.is_empty() {
        return Some(fine[fastrand::usize(..fine.len())]);
    }
    lairs.iter().copied().max_by(|a, b| nearest(*a).total_cmp(&nearest(*b)))
}

/// Runs each room's run: the shop's countdown, the wave's spawning, a wave cleared, the run lost.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn run_waves(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    time: Res<Time<Real>>,
    mut lobby: ResMut<Lobby>,
    mut party: Query<(&ControlledBy, &InRoom, &Pos, &mut Health, &MaxHealth, Option<&mut Dead>), With<Satchel>>,
    mobs: Query<&InRoom, (With<Mob>, Without<Dead>)>,
) {
    let now = timeline.tick().0;
    lobby.set_now(now);
    for (key, room) in lobby.runs() {
        let map = lobby.map(&room);
        let members = lobby.party(key);
        let fighters: Vec<(Entity, Vec2, bool)> = party
            .iter()
            .filter(|(_, r, ..)| **r == room)
            .map(|(c, _, pos, _, _, dead)| (c.owner, pos.0, dead.is_some()))
            .collect();
        let size = members.len().max(1);
        let alive = mobs.iter().filter(|r| **r == room).count();
        let Some(run) = lobby.run_mut(key) else { continue };
        run.alive = alive as u32;
        match run.phase {
            RunPhase::Shop { until } => {
                // Everyone in the arena ready: soon.
                let in_arena: Vec<u32> = members.iter().filter(|(link, _)| fighters.iter().any(|(f, ..)| f == link)).map(|(_, id)| *id).collect();
                let all_ready = !in_arena.is_empty() && in_arena.iter().all(|id| run.ready.contains(id));
                if all_ready && until > now + READY_TICKS {
                    run.phase = RunPhase::Shop { until: now + READY_TICKS };
                }
                let secs = run.phase_view(now);
                if let Phase::Shop { secs_left } = secs
                    && secs_left != run.shown_secs
                {
                    run.shown_secs = secs_left;
                    lobby.touch(key);
                }
                let Some(run) = lobby.run_mut(key) else { continue };
                if now < match run.phase {
                    RunPhase::Shop { until } => until,
                    _ => 0,
                } {
                    continue;
                }
                // The wave comes.
                run.phase = RunPhase::Fight;
                run.ready.clear();
                run.budget = budget(run.wave, size) * if run.omen == Some(Omen::Horde) { 1.5 } else { 1.0 };
                run.next_pack_at = now;
                run.boss_out = boss_of(run.wave).is_none();
                let wave = run.wave;
                info!(room = key.0, wave, party = size, "a wave comes");
                // Chests out in the wilds, far from the party.
                let spots: Vec<Vec2> = map.chests.iter().copied().filter(|c| fighters.iter().all(|(_, p, _)| p.distance(*c) > 20.0)).collect();
                let mut chests = Vec::new();
                for _ in 0..(1 + wave / 4).min(3) {
                    if spots.is_empty() {
                        break;
                    }
                    let at = spots[fastrand::usize(..spots.len())];
                    if chests.iter().any(|(_, a)| *a == at) {
                        continue;
                    }
                    let pickup = Pickup { kind: PickupKind::Chest, at, back_at: None, taken_by: None };
                    let chest = commands.spawn((Name::from("Chest"), pickup, room, room.rooms(), Replicate::to_clients(NetworkTarget::All))).id();
                    chests.push((chest, at));
                }
                if let Some(run) = lobby.run_mut(key) {
                    run.chests = chests.into_iter().map(|(c, _)| c).collect();
                }
            }
            RunPhase::Fight => {
                let wave = run.wave;
                let party_at: Vec<Vec2> = fighters.iter().filter(|(.., dead)| !dead).map(|(_, p, _)| *p).collect();
                // Lost: everyone in the arena down, with no lives to spend.
                if !fighters.is_empty() && fighters.iter().all(|(.., dead)| *dead) && run.lives == 0 {
                    run.phase = RunPhase::Over { reached: wave };
                    for chest in run.take_chests() {
                        commands.entity(chest).try_despawn();
                    }
                    info!(room = key.0, wave, "the run is lost");
                    lobby.lose_run(key, time.elapsed() + OVER_SHOWS);
                    continue;
                }
                if !run.boss_out
                    && let Some(boss) = boss_of(wave)
                    && let Some(at) = pick_lair(&map.lairs, &party_at)
                {
                    run.boss_out = true;
                    lobby.touch(key);
                    let hp = boss_toughness(wave, size);
                    spawn_mob_as(&mut commands, &mut lobby, room, boss, at, hp, might(wave), None, None);
                    info!(room = key.0, wave, boss = boss.def().name, "a boss comes");
                    continue;
                }
                let run = lobby.run_mut(key).expect("checked");
                if run.budget > 0.0 && now >= run.next_pack_at && alive < most_about(wave, size) {
                    run.next_pack_at = now + spawn_every(wave);
                    let roster = roster(wave);
                    let Some(class) = pick(&roster, run.budget) else { continue };
                    let mob = class.def().mob.as_ref().expect("on the roster");
                    run.budget -= mob.cost * mob.pack as f32;
                    let Some(lair) = pick_lair(&map.lairs, &party_at) else { continue };
                    lobby.touch(key);
                    for n in 0..mob.pack {
                        let at = lair + Vec2::from_angle(TAU * n as f32 / mob.pack as f32) * if mob.pack > 1 { 1.2 } else { 0.0 };
                        let at = if map.walkable_at(at) { at } else { lair };
                        let blood_moon = lobby.run(key).and_then(|r| r.omen) == Some(Omen::BloodMoon);
                        let odds = if blood_moon { (elite_odds(wave) * 3.0).max(0.25) } else { elite_odds(wave) };
                        let elite = (fastrand::f32() < odds).then(|| Affix::ALL[fastrand::usize(..Affix::ALL.len())]);
                        spawn_mob_as(&mut commands, &mut lobby, room, class, at, toughness(wave, size), might(wave), None, elite);
                    }
                    continue;
                }
                if run.budget > 0.0 || alive > 0 || !run.boss_out {
                    continue;
                }
                // Cleared: pay, heal, bring back the fallen, and shop.
                let pay = WAVE_PAY + WAVE_PAY_GROWTH * wave;
                run.wave += 1;
                run.phase = RunPhase::Shop { until: now + SHOP_TICKS };
                run.omen = roll_omen(run.wave);
                run.waiting.clear();
                let chests = std::mem::take(&mut run.chests);
                for chest in chests {
                    commands.entity(chest).try_despawn();
                }
                info!(room = key.0, wave, pay, "wave cleared");
                lobby.give_gold(key, pay);
                lobby.note_best(key, wave);
                for (_, r, _, mut health, max, dead) in &mut party {
                    if *r != room {
                        continue;
                    }
                    match dead {
                        Some(mut dead) => {
                            dead.respawn_at = now + 1;
                            dead.placed = false;
                        }
                        None => health.0 = max.0,
                    }
                }
            }
            RunPhase::Over { .. } => {}
        }
    }
}

/// A player falling in a run: back soon if the party has a life to spend, else not until the
/// wave's cleared.
pub(super) fn fall(timeline: Res<LocalTimeline>, mut lobby: ResMut<Lobby>, mut fallen: Query<(&ControlledBy, &InRoom, &mut Dead), Added<Dead>>) {
    let now = timeline.tick().0;
    for (controlled_by, room, mut dead) in &mut fallen {
        let guest = lobby.guest_id(controlled_by.owner);
        let Some(run) = lobby.run_mut(room.key).filter(|run| run.phase == RunPhase::Fight) else { continue };
        if run.lives > 0 {
            run.lives -= 1;
            dead.respawn_at = now + COMEBACK_TICKS;
        } else {
            // Until the wave's cleared (`run_waves` brings them back).
            dead.respawn_at = u32::MAX / 2;
            run.waiting.extend(guest);
        }
    }
}

/// Server-only: a player just back in a run, out of harm's way until `until` (`Untouchable`).
#[derive(Component)]
pub(crate) struct Sheltered {
    until: u32,
}

/// Players coming back in a run are sheltered a moment, and burst out of the ground: monsters
/// near are flung back out of reach, so a comeback among them isn't over at once. The shelter
/// ends in time.
pub(super) fn shelter(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    mut back: RemovedComponents<Dead>,
    players: Query<(&InRoom, &Pos), With<ControlledBy>>,
    mut monsters: Query<(&InRoom, &mut Pos), (With<Mob>, Without<ControlledBy>)>,
    sheltered: Query<(Entity, &Sheltered)>,
) {
    let now = timeline.tick().0;
    for player in back.read() {
        let Ok((room, at)) = players.get(player) else { continue };
        if !lobby.run(room.key).is_some_and(Run::on) {
            continue;
        }
        commands.entity(player).try_insert((Untouchable, Sheltered { until: now + SHELTER_TICKS }));
        spawn_fx(&mut commands, *room, Fx { kind: FxKind::Blast(FxColor::Spirit), at: at.0, to: at.0, radius: REBIRTH_REACH, from: now, until: now + 14 });
        let map = lobby.map(room);
        for (monster_room, mut pos) in &mut monsters {
            let away = pos.0 - at.0;
            if monster_room != room || away.length() >= REBIRTH_REACH {
                continue;
            }
            // Out along the way it was from us, as far as the ground goes.
            let dir = away.try_normalize().unwrap_or(Vec2::X);
            let to = (1..=(REBIRTH_REACH * 4.0) as i32)
                .map(|step| at.0 + dir * step as f32 * 0.25)
                .filter(|p| p.distance(at.0) > away.length())
                .take_while(|p| map.line_walkable(pos.0, *p))
                .last();
            if let Some(to) = to {
                pos.0 = to;
            }
        }
    }
    for (player, shelter) in &sheltered {
        if now >= shelter.until {
            commands.entity(player).try_remove::<(Untouchable, Sheltered)>();
        }
    }
}

/// A monster killed pays its bounty to the party (twice, on a Gilded wave); a boss, a life more.
/// On a Restless wave, the dead may rise again (once).
pub(super) fn pay_bounties(timeline: Res<LocalTimeline>, mut commands: Commands, mut lobby: ResMut<Lobby>, killed: Query<(Entity, &Mob, &InRoom, &Pos, &ClassId, Option<&Might>), Added<Dead>>) {
    let now = timeline.tick().0;
    for (entity, mob, room, pos, class, might) in &killed {
        let omen = lobby.run(room.key).and_then(|run| run.omen);
        if mob.bounty > 0 {
            lobby.give_gold(room.key, mob.bounty * if omen == Some(Omen::Gilded) { 2 } else { 1 });
        }
        // Risen ones (raised by another) don't rise again.
        if omen == Some(Omen::Restless)
            && !mob.boss
            && mob.raised_by.is_none()
            && fastrand::f32() < RISE_ODDS
            && let Some(zombie) = ClassId::by_key("zombie")
        {
            let wave = lobby.run(room.key).map_or(1, |run| run.wave);
            let size = lobby.party(room.key).len();
            spawn_mob(&mut commands, &mut lobby, *room, zombie, pos.0, Some(toughness(wave, size)), crate::combat::might_of(might), Some(entity));
        }
        if mob.boss {
            if let Some(run) = lobby.run_mut(room.key) {
                run.lives += 1;
            }
            info!(room = room.key.0, boss = class.def().name, "a boss falls");
            spawn_fx(&mut commands, *room, Fx { kind: FxKind::Coins, at: pos.0, to: pos.0, radius: 3.0, from: now, until: now + 40 });
        }
    }
}

/// The camp's fire mends the party: players in it, in a run, get some health back now and then.
pub(super) fn mend_at_camp(timeline: Res<LocalTimeline>, lobby: Res<Lobby>, mut party: Query<(&InRoom, &Pos, &mut Health, &MaxHealth, &mut RecentHits), (With<ControlledBy>, InPlay)>) {
    let now = timeline.tick().0;
    if now % MEND_EVERY_TICKS != 0 {
        return;
    }
    for (room, pos, mut health, max, mut hits) in &mut party {
        let Some((middle, half)) = lobby.map(room).camp else { continue };
        let inside = (pos.0 - middle).abs().max_element() < half;
        if !inside || health.0 >= max.0 || !lobby.run(room.key).is_some_and(Run::on) {
            continue;
        }
        let mended = ((max.0 as f32 * CAMP_MENDS * MEND_EVERY_TICKS as f32 / TICK_HZ as f32).ceil() as i32).min(max.0 - health.0);
        health.0 += mended;
        hits.push(mended, HitKind::Heal);
    }
}

/// A chest taken pays everyone in the run, more on later waves.
pub(crate) fn chest_gold(lobby: &Lobby, key: RoomKey) -> u32 {
    let wave = lobby.run(key).map_or(1, |run| run.wave);
    CHEST_GOLD + CHEST_GOLD_GROWTH * wave
}

/// Each player's fighter carries their satchel as the server keeps it (`Lobby`).
pub(super) fn sync_satchels(lobby: Res<Lobby>, mut fighters: Query<(&ControlledBy, &mut Satchel)>) {
    for (controlled_by, mut satchel) in &mut fighters {
        if let Some(kept) = lobby.satchel(controlled_by.owner) {
            satchel.set_if_neq(kept);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waves_grow_and_bosses_take_turns() {
        assert!(budget(10, 1) > budget(1, 1) * 3.0);
        assert!(budget(5, 4) > budget(5, 1) * 2.0);
        assert!(toughness(20, 1) > 2.0 && toughness(1, 1) == 1.0);
        assert_eq!(boss_of(4), None);
        let [first, second, third] = [5, 10, 15].map(|w| boss_of(w).expect("a boss wave"));
        assert!(first != second && second != third);
        assert_eq!(boss_of(20), Some(first), "the bosses come round again");
        assert!(boss_toughness(20, 1) > boss_toughness(5, 1));
        assert_eq!(elite_odds(3), 0.0);
        assert!(roster(1).iter().all(|(c, _)| c.def().mob.as_ref().unwrap().from_wave <= 1) && !roster(1).is_empty());
        assert!(roster(12).len() > roster(3).len());
    }

    #[test]
    fn lairs_well_away_from_the_party() {
        let lairs = [Vec2::new(5.0, 0.0), Vec2::new(30.0, 0.0), Vec2::new(200.0, 0.0)];
        assert_eq!(pick_lair(&lairs, &[Vec2::ZERO]), Some(Vec2::new(30.0, 0.0)));
        assert_eq!(pick_lair(&lairs[..1], &[Vec2::ZERO]), Some(Vec2::new(5.0, 0.0)));
    }
}
