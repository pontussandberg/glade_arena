//! The waves mode: a room's party against endless waves of monsters (`Run`). A run opens with
//! time at the shop; then wave after wave comes out of the map's lairs, each bigger than the
//! last, newer and nastier kinds joining in as they go, a boss leading every `BOSS_EVERY`th, a
//! few elites (`Elite`) among the rest, and chests turning up out in the wilds. Clearing a wave
//! pays everyone, heals and brings back the fallen, and opens the shop again until the next.
//!
//! Every kill pays its bounty to the whole party, so no one steals a kill. Each player has one
//! life a wave: a fallen one is down (`Downed`) until an ally stands over them long enough to pick
//! them up (`revive`), or the wave is cleared, or a boss falls. If the whole party is down, the
//! run is over: alone, one life is all there is.
//!
//! How hard a wave is, and what it's worth, is all in the tuning below, and it's the same every
//! run: which monsters come, which of them are elites and as what, and the wave's omen are all
//! the wave's own (its `Run::rng` is seeded by its number; omens come in a fixed turn,
//! `Omen::of_wave`). Only where they come out, and where chests turn up, changes.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::LazyLock;

use super::*;

use arena_shared::config::TICK_HZ;
use arena_shared::items::Satchel;
use arena_shared::map::Map;
use arena_shared::rooms::{BOSS_EVERY, Omen, Phase, RoomKey, ZOMBIES};

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
/// How long a player coming back can't be hurt (nor is gone for), so they're not caught at once,
/// and how far round them monsters are flung back as they do.
const SHELTER_TICKS: u32 = 3 * TICK_HZ as u32;
const REBIRTH_REACH: f32 = 6.0;
/// When a downed player comes back by themselves: not until the wave's cleared (or a boss falls).
const UNTIL_CLEARED: u32 = u32::MAX / 2;
/// How much of their health a player picked up by an ally comes back with.
const REVIVE_HEALTH: f32 = 0.4;
/// How near the party a lair may be to send out monsters, and how far (if any are between).
const LAIR_NEAREST: f32 = 18.0;
const LAIR_FARTHEST: f32 = 42.0;
/// How far from the party chests turn up, at least.
const CHESTS_AWAY: f32 = 20.0;

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
    let round = (wave / BOSS_EVERY - 1) / BOSSES.len().max(1) as u32;
    (1.0 + 0.45 * (party.max(1) - 1) as f32) * (1.0 + 0.8 * round as f32) * (1.0 + 0.03 * wave.saturating_sub(BOSS_EVERY) as f32)
}

/// The odds that a monster on `wave` comes as an elite.
fn elite_odds(wave: u32) -> f32 {
    ((wave as f32 - 5.0) * 0.02).clamp(0.0, 0.3)
}

/// How often a pack comes out, and how many may be about at once.
fn spawn_every(wave: u32) -> u32 {
    if HORDE.is_some() {
        return 2;
    }
    48u32.saturating_sub(wave).max(14)
}
fn most_about(wave: u32, party: usize) -> usize {
    HORDE.unwrap_or((14 + wave as usize + 4 * (party.max(1) - 1)).min(44))
}

/// `ARENA_HORDE=N` (for measuring what a crowd costs): every wave keeps `N` monsters about, a
/// pack coming out every other tick till it has them, and never runs out of them; and the party
/// never falls (`combat::damage`).
pub(crate) static HORDE: std::sync::LazyLock<Option<usize>> = std::sync::LazyLock::new(|| std::env::var("ARENA_HORDE").ok().and_then(|n| n.parse().ok()));

/// What an elite is, against its kind: (toughness, might, bounty).
const ELITE: (f32, f32, u32) = (2.5, 1.3, 3);

/// On a Restless wave, every how many monsters to die one rises again, and as what.
const RISES_EVERY: u32 = 3;
const RISES_AS: &str = "zombie";

/// The bosses, in the order they come.
static BOSSES: LazyLock<Vec<ClassId>> = LazyLock::new(|| {
    let mut bosses: Vec<ClassId> = ClassId::all().filter(|c| c.def().is_boss()).collect();
    bosses.sort_by_key(|c| c.def().mob.as_ref().map_or(0, |m| m.from_wave));
    bosses
});

/// The boss leading `wave`, if it's a boss wave.
pub(crate) fn boss_of(wave: u32) -> Option<ClassId> {
    (wave % BOSS_EVERY == 0 && !BOSSES.is_empty()).then(|| BOSSES[((wave / BOSS_EVERY - 1) as usize) % BOSSES.len()])
}


/// A room's run, on the server.
#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) wave: u32,
    pub(crate) phase: RunPhase,
    /// What's still to spawn this wave, and when the next pack may.
    budget: f32,
    next_pack_at: u32,
    /// The boss of this wave is out (or it has none).
    boss_out: bool,
    /// Who's ready for the next wave (`Member::guest_id`).
    pub(crate) ready: Vec<u32>,
    /// Who's down (`Member::guest_id`), until picked up or the wave's cleared.
    pub(crate) waiting: Vec<u32>,
    /// A boss fell: the downed get up.
    raise_fallen: bool,
    /// Monsters about, as last counted.
    alive: u32,
    /// Chests lying out this wave.
    chests: Vec<Entity>,
    /// What's strange about this wave (`Omen::of_wave`, shown as the shop opens before it).
    pub(crate) omen: Option<Omen>,
    /// This wave's own chances: which monsters come, and which are elites (`wave_chances`, as it
    /// comes).
    rng: fastrand::Rng,
    /// Monsters that could rise again that have died this wave (on a Restless wave every
    /// `RISES_EVERY`th does).
    deaths: u32,
    /// A wave has come: the run is under way, and no one joins it any more.
    pub(crate) fought: bool,
    /// Everyone was ready: the wave comes at this tick (sooner than the shop's own end), unless
    /// someone isn't any more.
    ready_by: Option<u32>,
    /// The wave it started at: only a run from the first makes the leaderboards.
    pub(crate) from: u32,
    /// Who fought it (guest ids, names and classes), as the last wave came: who it's on the
    /// leaderboards as.
    pub(crate) party: Vec<(u32, String, ClassId)>,
    /// It's on the leaderboards already (it ended).
    pub(crate) scored: bool,
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
    /// A run starting at `now`, its first wave `wave` (1 but for trying later ones out).
    pub(crate) fn new(now: u32, wave: u32) -> Run {
        Run {
            wave,
            phase: RunPhase::Shop { until: now + FIRST_SHOP_TICKS },
            budget: 0.0,
            next_pack_at: 0,
            boss_out: false,
            ready: Vec::new(),
            waiting: Vec::new(),
            raise_fallen: false,
            alive: 0,
            chests: Vec::new(),
            omen: Omen::of_wave(wave),
            // Set as each wave comes.
            rng: wave_chances(wave),
            deaths: 0,
            fought: false,
            ready_by: None,
            from: wave,
            party: Vec::new(),
            scored: false,
        }
    }

    /// When the shop (open until `until`) closes: then, or sooner if everyone's ready.
    fn shop_ends(&self, until: u32) -> u32 {
        self.ready_by.map_or(until, |soon| soon.min(until))
    }

    /// Monsters are about (or may come): the run's on and not lost.
    pub(crate) fn on(&self) -> bool {
        !matches!(self.phase, RunPhase::Over { .. })
    }

    pub(crate) fn phase_view(&self, now: u32) -> Phase {
        match self.phase {
            RunPhase::Shop { until } => Phase::Shop { secs_left: self.shop_ends(until).saturating_sub(now).div_ceil(TICK_HZ as u32) },
            RunPhase::Fight => Phase::Fight,
            RunPhase::Over { reached } => Phase::Over { reached },
        }
    }

    /// Monsters still to come this wave, and about, as players see them (by what's left to spend).
    pub(crate) fn enemies_left(&self) -> u32 {
        self.alive + (self.budget.max(0.0) / 1.2).ceil() as u32 + u32::from(!self.boss_out)
    }

    /// The chests lying out are gone.
    pub(crate) fn clear_chests(&mut self, commands: &mut Commands) {
        for chest in self.chests.drain(..) {
            commands.entity(chest).try_despawn();
        }
    }

    /// The gold a player starts the run with: `START_GOLD`, and for a run started at a later wave
    /// (to try it out, "Waves 10"), about what they'd have earned by then.
    pub(crate) fn starting_gold(&self) -> u32 {
        START_GOLD + (1..self.wave).map(|w| WAVE_PAY + WAVE_PAY_GROWTH * w + 5 * budget(w, 1) as u32).sum::<u32>()
    }
}

/// `count` spots in a ring `radius` round `at`, each on open ground it can be walked to from
/// `at` (else `at` itself): where a pack, a brood or the summoned come up.
pub(crate) fn ring_spots(map: &Map, at: Vec2, count: u32, radius: f32) -> impl Iterator<Item = Vec2> + '_ {
    let turn = fastrand::f32() * TAU;
    (0..count).map(move |n| {
        let spot = at + Vec2::from_angle(turn + TAU * n as f32 / count.max(1) as f32) * radius;
        if map.walkable_at(spot) && map.line_walkable(at, spot) { spot } else { at }
    })
}

/// A monster of `class` rising at `at` in `room` at `now`: `hp_scale` times as tough as its
/// class, hitting `might` times as hard, called up by `raised_by` if it was summoned (or rose
/// again).
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_mob(commands: &mut Commands, lobby: &mut Lobby, now: u32, room: InRoom, class: ClassId, at: Vec2, hp_scale: f32, might: f32, raised_by: Option<Entity>) -> Entity {
    spawn_mob_as(commands, lobby, now, room, class, at, hp_scale, might, raised_by, None)
}

/// `spawn_mob`, an elite with `elite`.
#[allow(clippy::too_many_arguments)]
fn spawn_mob_as(commands: &mut Commands, lobby: &mut Lobby, now: u32, room: InRoom, class: ClassId, at: Vec2, hp_scale: f32, might: f32, raised_by: Option<Entity>, elite: Option<Affix>) -> Entity {
    let def = class.def();
    let (elite_hp, elite_might, elite_bounty) = if elite.is_some() { ELITE } else { (1.0, 1.0, 1) };
    let hp = (def.max_hp as f32 * hp_scale * elite_hp).round().max(1.0) as i32;
    let bounty = def.mob.as_ref().map_or(0, |m| m.bounty) * elite_bounty;
    let id = lobby.next_local();
    let mut mob = commands.spawn((
        Name::from(def.name.clone()),
        Zombie,
        Mob::new(class, now, bounty, raised_by),
        fighter(id, class, room, Team(ZOMBIES), at),
        Might(might * elite_might),
        PlayerInput::default(),
        replicated_npc(),
    ));
    // As tough as the wave makes it, over its class's.
    mob.insert((Health(hp), MaxHealth(hp)));
    if let Some(affix) = elite {
        mob.insert(Elite(affix));
    }
    // Swift, or every one on a Frenzy wave (bosses aside).
    let frenzy = lobby.run(room.key).and_then(|run| run.omen).is_some_and(Omen::hastes) && !def.is_boss();
    if elite == Some(Affix::Swift) || frenzy {
        mob.insert(Hasted(Span { from: now + 1, until: u32::MAX }));
    }
    let entity = mob.id();
    spawn_fx(commands, room, Fx::at(FxKind::Rise, at, 0.8 * def.size(), now, 16));
    entity
}

/// `wave`'s own chances: seeded by its number, so it comes out the same whenever it comes.
fn wave_chances(wave: u32) -> fastrand::Rng {
    fastrand::Rng::with_seed(wave as u64)
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

fn pick(rng: &mut fastrand::Rng, roster: &[(ClassId, f32)], budget: f32) -> Option<ClassId> {
    let fits: Vec<&(ClassId, f32)> = roster.iter().filter(|(c, _)| c.def().mob.as_ref().is_some_and(|m| m.cost * m.pack as f32 <= budget + 0.5)).collect();
    let fits = if fits.is_empty() { roster.iter().collect() } else { fits };
    let total: f32 = fits.iter().map(|(_, w)| w).sum();
    let mut roll = rng.f32() * total;
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

/// Chests for `wave`, out at `map`'s chest spots far from the party (`party_at`).
fn scatter_chests(commands: &mut Commands, room: InRoom, map: &Map, party_at: &[Vec2], wave: u32) -> Vec<Entity> {
    let mut spots: Vec<Vec2> = map.chests.iter().copied().filter(|c| party_at.iter().all(|p| p.distance(*c) > CHESTS_AWAY)).collect();
    fastrand::shuffle(&mut spots);
    let count = (1 + wave / 4).min(3) as usize;
    spots.into_iter().take(count).map(|at| spawn_pickup(commands, room, at, PickupKind::Chest)).collect()
}

type Party<'w, 's> = Query<'w, 's, (&'static ControlledBy, &'static InRoom, &'static Pos, &'static mut Health, &'static MaxHealth, Option<&'static mut Dead>), With<Satchel>>;

/// Runs each room's run: the shop's countdown, the wave's spawning, a wave cleared, the run lost.
/// Its members are sent the run again whenever what they see of it changed.
pub(super) fn run_waves(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut lobby: ResMut<Lobby>,
    mut party: Party,
    mobs: Query<&InRoom, (With<Mob>, Without<Dead>)>,
) {
    let now = timeline.tick().0;
    let mut alive: HashMap<RoomKey, u32> = HashMap::new();
    for room in &mobs {
        *alive.entry(room.key).or_default() += 1;
    }
    for (key, room) in lobby.runs() {
        let alive = alive.get(&key).copied().unwrap_or(0);
        step_run(&mut commands, &mut lobby, &mut party, now, room, alive);
        lobby.refresh_run(key, now);
    }
}

/// One tick of `room`'s run (`run_waves`), `alive` monsters about.
fn step_run(commands: &mut Commands, lobby: &mut Lobby, party: &mut Party, now: u32, room: InRoom, alive: u32) {
    let key = room.key;
    let map = lobby.map(&room);
    let members = lobby.party(key);
    // Scaled to who's fighting, not who's in the room.
    let size = lobby.fighting(key).max(1);
    let fighters: Vec<(Entity, Vec2, bool)> = party.iter().filter(|(_, r, ..)| **r == room).map(|(c, _, pos, _, _, dead)| (c.owner, pos.0, dead.is_some())).collect();
    let party_at: Vec<Vec2> = fighters.iter().filter(|(.., dead)| !dead).map(|(_, p, _)| *p).collect();
    let Some(run) = lobby.run_mut(key) else { return };
    run.alive = alive;
    let wave = run.wave;
    match run.phase {
        RunPhase::Shop { until } => {
            // Everyone in the arena ready: soon.
            let in_arena = members.iter().filter(|(link, _)| fighters.iter().any(|(f, ..)| f == link));
            let all_ready = in_arena.clone().next().is_some() && in_arena.into_iter().all(|(_, id)| run.ready.contains(id));
            // Soon once everyone's ready; the shop's own time again if someone isn't any more.
            run.ready_by = match (all_ready, run.ready_by) {
                (true, None) => Some(now + READY_TICKS),
                (true, soon) => soon,
                (false, _) => None,
            };
            if now < run.shop_ends(until) {
                return;
            }
            run.ready_by = None;
            // The wave comes, and chests with it.
            run.phase = RunPhase::Fight;
            run.ready.clear();
            run.budget = budget(wave, size) * run.omen.map_or(1.0, Omen::budget_scale) * if HORDE.is_some() { 1000.0 } else { 1.0 };
            run.rng = wave_chances(wave);
            run.deaths = 0;
            // Under way: closed to newcomers now (the lobby list says so, below).
            let closing = !std::mem::replace(&mut run.fought, true);
            run.next_pack_at = now;
            run.boss_out = boss_of(wave).is_none();
            run.chests = scatter_chests(commands, room, map, &party_at, wave);
            info!(room = key.0, wave, party = size, "a wave comes");
            if closing {
                lobby.close_run();
            }
            // Who's fighting it, for the leaderboards.
            let roster = lobby.roster(key);
            if let Some(run) = lobby.run_mut(key) {
                run.party = roster;
            }
        }
        RunPhase::Fight => {
            // Measuring a crowd (`HORDE`): the party stays on its feet, healed whole every tick,
            // so the fight goes on.
            if HORDE.is_some() {
                for (_, r, _, mut health, max, dead) in party.iter_mut() {
                    if *r == room && dead.is_none() {
                        health.0 = max.0;
                    }
                }
            }
            // A boss fell: the downed get up (back at the camp).
            if std::mem::take(&mut run.raise_fallen) {
                run.waiting.clear();
                bring_back(party, room, now);
            }
            // Lost: everyone in the arena down.
            let out = party.iter().filter(|(_, r, ..)| **r == room).all(|(.., dead)| dead.is_some_and(|dead| dead.respawn_at >= UNTIL_CLEARED));
            if !fighters.is_empty() && out {
                run.phase = RunPhase::Over { reached: wave };
                run.clear_chests(commands);
                info!(room = key.0, wave, "the run is lost");
                lobby.lose_run(key);
                return;
            }
            if !run.boss_out
                && let Some(boss) = boss_of(wave)
                && let Some(at) = pick_lair(&map.lairs, &party_at)
            {
                run.boss_out = true;
                spawn_mob(commands, lobby, now, room, boss, at, boss_toughness(wave, size), might(wave), None);
                info!(room = key.0, wave, boss = boss.def().name, "a boss comes");
                return;
            }
            if run.budget > 0.0 && now >= run.next_pack_at && (alive as usize) < most_about(wave, size) {
                run.next_pack_at = now + spawn_every(wave);
                let (Some(class), Some(lair)) = (pick(&mut run.rng, &roster(wave), run.budget), pick_lair(&map.lairs, &party_at)) else { return };
                let mob = class.def().mob.as_ref().expect("on the roster");
                run.budget -= mob.cost * mob.pack as f32;
                let odds = run.omen.map_or(elite_odds(wave), |omen| omen.elite_odds(elite_odds(wave)));
                let radius = if mob.pack > 1 { 1.2 } else { 0.0 };
                // Which are elites, and as what, by the wave's own chances.
                let pack: Vec<(Vec2, Option<Affix>)> =
                    ring_spots(map, lair, mob.pack, radius).map(|at| (at, (run.rng.f32() < odds).then(|| Affix::ALL[run.rng.usize(..Affix::ALL.len())]))).collect();
                for (at, elite) in pack {
                    spawn_mob_as(commands, lobby, now, room, class, at, toughness(wave, size), might(wave), None, elite);
                }
                return;
            }
            if run.budget > 0.0 || alive > 0 || !run.boss_out {
                return;
            }
            // Cleared: pay, heal, bring back the fallen, and shop.
            let pay = WAVE_PAY + WAVE_PAY_GROWTH * wave;
            run.wave += 1;
            run.phase = RunPhase::Shop { until: now + SHOP_TICKS };
            run.omen = Omen::of_wave(run.wave);
            run.waiting.clear();
            run.clear_chests(commands);
            info!(room = key.0, wave, pay, "wave cleared");
            lobby.give_gold(key, pay);
            lobby.note_best(key, wave);
            // The bastion's next court may open with the coming wave: thunder tears its gate
            // open while everyone shops.
            if let Some(gate) = crate::rooms::stage_map(lobby, commands, key, wave + 1) {
                spawn_fx(commands, room, Fx::at(FxKind::Blast(FxColor::Storm), gate, 3.5, now, 24));
            }
            bring_back(party, room, now);
            for (_, r, _, mut health, max, dead) in party.iter_mut() {
                if *r == room && dead.is_none() {
                    health.0 = max.0;
                }
            }
        }
        RunPhase::Over { .. } => {}
    }
}

/// Everyone down in `room` comes back (at the camp, fresh) at once.
fn bring_back(party: &mut Party, room: InRoom, now: u32) {
    for (_, r, .., dead) in party.iter_mut() {
        if let Some(mut dead) = dead.filter(|_| *r == room) {
            // Time to be moved to the camp while still down (`respawn`), then up there.
            dead.respawn_at = now + crate::spawning::PLACE_BEFORE_RESPAWN_TICKS + 2;
            dead.placed = false;
        }
    }
}

/// A player falling in a run is down where they fell: until an ally picks them up (`revive`), or
/// the wave's cleared (`run_waves` brings them back).
pub(super) fn fall(mut commands: Commands, mut lobby: ResMut<Lobby>, mut fallen: Query<(Entity, &ControlledBy, &InRoom, &mut Dead), Added<Dead>>) {
    for (player, controlled_by, room, mut dead) in &mut fallen {
        let guest = lobby.guest_id(controlled_by.owner);
        let Some(run) = lobby.run_mut(room.key).filter(|run| run.phase == RunPhase::Fight) else { continue };
        dead.respawn_at = UNTIL_CLEARED;
        run.waiting.extend(guest);
        commands.entity(player).insert(Downed::default());
    }
}

/// Allies pick each other up: one standing over a downed player (`REVIVE_RADIUS`) fills their
/// rescue, and after `REVIVE_TICKS` of it they're back on their feet where they lay, with some
/// health. Stepping off loses it.
pub(super) fn revive(
    mut commands: Commands,
    mut lobby: ResMut<Lobby>,
    mut downed: Query<(Entity, &ControlledBy, &InRoom, &Pos, &mut Downed, &mut Health, &MaxHealth), With<Dead>>,
    rescuers: Query<(&InRoom, &Pos), (With<ControlledBy>, InPlay)>,
) {
    for (player, controlled_by, room, pos, mut down, mut health, max) in &mut downed {
        let rescued = rescuers.iter().any(|(r, p)| r == room && p.0.distance(pos.0) <= REVIVE_RADIUS);
        let rescue = if rescued { (down.rescue + 1.0 / REVIVE_TICKS as f32).min(1.0) } else { 0.0 };
        down.set_if_neq(Downed { rescue });
        if rescue < 1.0 {
            continue;
        }
        health.0 = (max.0 as f32 * REVIVE_HEALTH).round() as i32;
        commands.entity(player).remove::<Dead>();
        let guest = lobby.guest_id(controlled_by.owner);
        if let Some(run) = lobby.run_mut(room.key) {
            run.waiting.retain(|id| Some(*id) != guest);
        }
        info!(room = room.key.0, ?guest, "picked back up");
    }
}

/// Server-only: a player just back in a run, out of harm's way until `until`: nothing hits them,
/// and monsters pay them no mind.
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
        commands.entity(player).try_remove::<Downed>();
        if !lobby.run_on(room.key) {
            continue;
        }
        commands.entity(player).try_insert(Sheltered { until: now + SHELTER_TICKS });
        spawn_fx(&mut commands, *room, Fx::at(FxKind::Blast(FxColor::Spirit), at.0, REBIRTH_REACH, now, 14));
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
            commands.entity(player).try_remove::<Sheltered>();
        }
    }
}

/// A monster killed pays its bounty to the party (more on a Gilded wave); a boss brings the
/// downed back.
/// On a Restless wave, every `RISES_EVERY`th to die rises again (once).
pub(super) fn pay_bounties(timeline: Res<LocalTimeline>, mut commands: Commands, mut lobby: ResMut<Lobby>, killed: Query<(Entity, &Mob, &InRoom, &Pos, &ClassId, Option<&Might>), Added<Dead>>) {
    let now = timeline.tick().0;
    for (entity, mob, room, pos, class, might) in &killed {
        let omen = lobby.run(room.key).and_then(|run| run.omen);
        // What another called up (summoned, risen, split off) pays nothing: no farming a boss's
        // endless summons.
        if mob.raised_by.is_none() {
            lobby.give_gold(room.key, mob.bounty * omen.map_or(1, Omen::bounty_scale));
        }
        let boss = class.def().is_boss();
        // Only those that could rise count: not a boss, nor one raised by another (summoned, or
        // risen already).
        let may_rise = !boss && mob.raised_by.is_none();
        let deaths = lobby.run_mut(room.key).filter(|_| may_rise).map_or(0, |run| {
            run.deaths += 1;
            run.deaths
        });
        if omen.is_some_and(Omen::raises)
            && may_rise
            && deaths.is_multiple_of(RISES_EVERY)
            && let Some(risen) = ClassId::by_key(RISES_AS)
        {
            let wave = lobby.run(room.key).map_or(1, |run| run.wave);
            let size = lobby.fighting(room.key);
            spawn_mob(&mut commands, &mut lobby, now, *room, risen, pos.0, toughness(wave, size), crate::combat::might_of(might), Some(entity));
        }
        if boss {
            if let Some(run) = lobby.run_mut(room.key) {
                run.raise_fallen = true;
            }
            info!(room = room.key.0, boss = class.def().name, "a boss falls");
            spawn_fx(&mut commands, *room, Fx::at(FxKind::Coins, pos.0, 3.0, now, 40));
        }
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
        let bosses = [5, 10, 15, 20, 25].map(|w| boss_of(w).expect("a boss wave"));
        assert_eq!(bosses.map(|b| b.def().id.clone()), ["butcher", "pale_lich", "grave_lord", "ember_archmage", "plague_mother"]);
        assert_eq!(boss_of(30), Some(bosses[0]), "the bosses come round again");
        assert!(boss_toughness(30, 1) > boss_toughness(5, 1) * 1.8);
        assert_eq!(elite_odds(3), 0.0);
        assert!(roster(1).iter().all(|(c, _)| c.def().mob.as_ref().unwrap().from_wave <= 1) && !roster(1).is_empty());
        assert!(roster(12).len() > roster(3).len());
        assert!(ClassId::by_key(RISES_AS).is_some_and(|c| c.def().mob.is_some()));
        assert!(ClassId::by_key(crate::brain::SPLITS_INTO).is_some_and(|c| c.def().mob.is_some()));
    }

    /// A wave is the same every run: the same monsters, in the same order, the same elites, the
    /// same omen.
    #[test]
    fn a_wave_is_the_same_every_time() {
        let sequence = |wave: u32| {
            let mut run = Run { rng: wave_chances(wave), ..Run::new(0, wave) };
            let mut budget = budget(wave, 1);
            let mut picks = Vec::new();
            while budget > 0.0 {
                let class = pick(&mut run.rng, &roster(wave), budget).unwrap();
                let mob = class.def().mob.as_ref().unwrap();
                budget -= mob.cost * mob.pack as f32;
                let elite = (run.rng.f32() < elite_odds(wave)).then(|| run.rng.usize(..Affix::ALL.len()));
                picks.push((class, elite));
            }
            (picks, run.omen)
        };
        for wave in [1, 7, 12, 23, 40] {
            assert_eq!(sequence(wave), sequence(wave), "wave {wave} came out differently");
        }
        assert_ne!(sequence(12).0, sequence(13).0, "every wave the same as the next");
        // Omens in turn, between the bosses, from wave 4.
        let omens: Vec<(u32, Omen)> = (1..=20).filter_map(|w| Omen::of_wave(w).map(|o| (w, o))).collect();
        assert_eq!(
            omens,
            [(4, Omen::Gilded), (7, Omen::Horde), (9, Omen::Frenzy), (12, Omen::Restless), (14, Omen::BloodMoon), (17, Omen::Volatile), (19, Omen::Gilded)]
        );
        assert!((1..200).all(|w| boss_of(w).is_none() || Omen::of_wave(w).is_none()), "an omen on a boss wave");
        // The newer kinds come later on.
        for (key, from) in [("void_acolyte", 13), ("ember_imp", 16), ("storm_caller", 19)] {
            let class = ClassId::by_key(key).unwrap();
            assert!(roster(from).iter().any(|(c, _)| *c == class) && !roster(from - 1).iter().any(|(c, _)| *c == class), "{key}");
        }
    }

    #[test]
    fn lairs_well_away_from_the_party() {
        let lairs = [Vec2::new(5.0, 0.0), Vec2::new(30.0, 0.0), Vec2::new(200.0, 0.0)];
        assert_eq!(pick_lair(&lairs, &[Vec2::ZERO]), Some(Vec2::new(30.0, 0.0)));
        assert_eq!(pick_lair(&lairs[..1], &[Vec2::ZERO]), Some(Vec2::new(5.0, 0.0)));
    }
}
