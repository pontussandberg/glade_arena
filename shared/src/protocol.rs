//! Wire protocol: replicated components, player commands and messages.

use bevy::math::Curve;
use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

pub use crate::abilities::dash::{Dash, Dashing};
pub use crate::abilities::{AbilitySlot, CastOrder, Cooldowns, MAX_ABILITIES};
pub use crate::classes::ClassId;
use crate::classes::{AttackDef, Chill};
use crate::map::MapId;
use crate::rooms::{LeaveReason, Mode, NO_TEAM, RoomKey, RoomSummary, RoomView};

/// Marks a player entity and says which client controls it. Projectiles don't carry it (their
/// owner is `Projectile::owner`), so `With<PlayerId>` always means "a player".
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct PlayerId(pub PeerId);

/// Which side a player fights on: `rooms::TEAM_A` or `rooms::TEAM_B`, or `rooms::NO_TEAM` in
/// free-for-all; zombies are all `rooms::ZOMBIES`. Allies can't hurt each other.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Reflect)]
pub struct Team(pub u8);

impl Team {
    pub fn allied(self, other: Team) -> bool {
        self.0 != NO_TEAM && self.0 == other.0
    }
}

/// Position on the 2D gameplay plane (the client renders it in 3D, see `render/`).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect, Deref, DerefMut)]
pub struct Pos(pub Vec2);

impl Ease for Pos {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| Pos(start.0.lerp(end.0, t)))
    }
}

/// Server-authoritative and replicated. The maximum is its `MaxHealth`.
/// Zero means dead and waiting to respawn: hidden, and can't move or attack.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Health(pub i32);

impl Health {
    /// The one rule for "in the fight" on the client: drawing and the bot use it.
    pub fn alive(&self) -> bool {
        self.0 > 0
    }
}

/// The most health a fighter has: its class's `max_hp`, or more for a monster grown with the
/// waves (or an elite). Server-authoritative and replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct MaxHealth(pub i32);

/// The last few hits a player took, for the damage numbers over it. Server-authoritative and
/// replicated like `Health`. Every hit gets the next running number, so a client shows each one
/// once, even when updates arrive bunched or skip a state.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default, Reflect)]
pub struct RecentHits(pub Vec<Hit>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Hit {
    pub seq: u32,
    /// Health lost (or, for a heal, gained).
    pub amount: i32,
    pub kind: HitKind,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum HitKind {
    Damage,
    /// Damage, critical.
    Crit,
    /// Health gained (a heal pickup).
    Heal,
}

impl RecentHits {
    /// How many hits are kept: more than can land between two updates (every 50 ms).
    const KEEP: usize = 4;

    /// The running number of the latest hit (0 before the first).
    pub fn seq(&self) -> u32 {
        self.0.last().map_or(0, |hit| hit.seq)
    }

    pub fn push(&mut self, amount: i32, kind: HitKind) {
        let seq = self.seq() + 1;
        if self.0.len() == Self::KEEP {
            self.0.remove(0);
        }
        self.0.push(Hit { seq, amount, kind });
    }
}

/// The auto-attack's state: the attack in progress (if any; from the click until the next may
/// start), when the last one went off, and the follow-up's window. Server-authoritative and
/// replicated: others' is shown on the same delayed timeline as their positions, so they see your
/// windup coming.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct AttackState {
    pub swing: Option<Swing>,
    /// The tick the last attack went off (`None`: never).
    pub released_at: Option<u32>,
    /// An attack started by this tick is quick (`FollowUp`; `None`: not armed). Used up when an
    /// attack goes off, not when one is cancelled.
    pub quick_until: Option<u32>,
    /// The next attack is primed (`abilities::prime`) by the ability in this slot: sure to crit,
    /// and this much more attack speed. Used up when a primed attack goes off, not when one is
    /// cancelled.
    pub primed: Option<(AbilitySlot, f32)>,
}

impl AttackState {
    /// Would an attack started at `now` (a fractional tick) be quick?
    pub fn quick_at(&self, now: f32) -> bool {
        self.quick_until.is_some_and(|until| now <= until as f32)
    }

    /// Is the next attack (or the one under way) empowered by the `follow_up` at `now` (a
    /// fractional tick)?
    pub fn empowered_at(&self, now: f32) -> bool {
        self.quick_at(now) || self.swing.is_some_and(|swing| swing.quick)
    }

    /// Is the `follow_up` still to be used at `now` (a fractional tick): armed, or its empowered
    /// attack winding up? Not once that attack has gone off, hit or miss.
    pub fn armed_at(&self, now: f32) -> bool {
        self.quick_at(now) || self.winding_up().is_some_and(|swing| swing.quick)
    }

    /// Has the attack in progress gone off yet?
    pub fn released(&self) -> bool {
        self.swing.is_some_and(|swing| self.released_at.is_some_and(|at| at >= swing.started_at))
    }

    /// The attack in progress while it's still winding up (rooting its fighter).
    pub fn winding_up(&self) -> Option<Swing> {
        self.swing.filter(|_| !self.released())
    }
}

/// An attack, from the click until the next may start: aim locked toward `dir`, winding up for
/// `windup_ticks` until it goes off, then the backswing until `ready_at`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Swing {
    pub started_at: u32,
    pub dir: Vec2,
    /// Quickened by the attack's `follow_up`.
    pub quick: bool,
    /// Its hit armed the `follow_up`: the rest of its backswing runs at that attack speed too,
    /// so the empowered attack can come that much sooner.
    pub hastened: bool,
    /// Attack speed on top, as a share of the normal (0.1 = 10% more), from what the attacker
    /// had going when it started (a `Frenzy`, a prime), kept for the whole attack.
    pub haste: f32,
    /// Primed (`AttackState::primed`): sure to crit.
    pub primed: bool,
}

impl Swing {
    // Its timing depends only on the attack (`AttackDef`), so any attack can be timed and
    // tested, not just the classes'.

    /// How many times the normal attack speed it has.
    pub fn speed(&self, attack: &AttackDef) -> f32 {
        attack.speed(self.quick) + self.haste
    }

    /// How long it winds up.
    pub fn windup_ticks(&self, attack: &AttackDef) -> u32 {
        attack.windup_ticks(self.speed(attack))
    }

    /// How long the whole attack is: its attack timer.
    pub fn ticks(&self, attack: &AttackDef) -> u32 {
        attack.period_ticks(self.speed(attack))
    }

    /// The tick the attack goes off.
    pub fn releases_at(&self, attack: &AttackDef) -> u32 {
        self.started_at + self.windup_ticks(attack)
    }

    /// How many times the normal attack speed its backswing has: hastened, or quick all through.
    pub fn backswing_speed(&self, attack: &AttackDef) -> f32 {
        attack.speed(self.quick || self.hastened) + self.haste
    }

    /// The tick the next attack may start: after its backswing, sped up if its hit armed the
    /// `follow_up` (a quick one's already is).
    pub fn ready_at(&self, attack: &AttackDef) -> u32 {
        let backswing = self.ticks(attack) - self.windup_ticks(attack);
        let backswing = if self.hastened && !self.quick {
            (backswing as f32 * self.speed(attack) / self.backswing_speed(attack)).round() as u32
        } else {
            backswing
        };
        self.releases_at(attack) + backswing.max(1)
    }

    /// How far it is from going off to the next attack being ready at `now` (a fractional
    /// tick), from 0 to 1: the cast bar, shown once it has gone off.
    pub fn cooldown_progress(&self, now: f32, attack: &AttackDef) -> f32 {
        let (from, to) = (self.releases_at(attack) as f32, self.ready_at(attack) as f32);
        ((now - from) / (to - from)).clamp(0.0, 1.0)
    }

    /// How far along the windup is at `now` (a fractional tick), from 0 to 1.
    pub fn windup_progress(&self, now: f32, attack: &AttackDef) -> f32 {
        ((now - self.started_at as f32) / self.windup_ticks(attack) as f32).clamp(0.0, 1.0)
    }
}

/// Interpolation for positions: a walk or a dash blends, but a jump (`JUMP`: a respawn, a
/// waygate) shows at once, rather than the fighter sliding across the map to where it came out.
fn lerp_or_jump(start: Pos, end: Pos, t: f32) -> Pos {
    if start.distance(end.0) > crate::config::JUMP { start } else { Pos(start.lerp(end.0, t)) }
}

/// Interpolation for state that jumps instead of blending: others' attack state and swings
/// change on the same delayed timeline as their positions, so a windup shows when their body
/// stops, not a round of interpolation delay earlier.
fn hold<C>(start: C, _end: C, _t: f32) -> C {
    start
}

/// Crowd control on a player: how much it's slowed and when, and when it's frozen in place
/// (rooted). Server-authoritative like `Health`. The spans are in ticks, so each fighter's are
/// shown on the timeline it's drawn on (a span that hadn't started yet doesn't apply before it).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct Chilled {
    /// The fraction of speed taken away while `slowed`.
    pub slow: f32,
    pub slowed: Span,
    pub rooted: Span,
}

/// Ticks `from` (inclusive) to `until` (exclusive). The default covers none.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct Span {
    pub from: u32,
    pub until: u32,
}

impl Span {
    /// Whether the span covers `now` (a fractional tick for drawing in between).
    pub fn covers(&self, now: f32) -> bool {
        self.from as f32 <= now && now < self.until as f32
    }

    /// `ticks` from the tick after `now`, or the rest of this span if it's still running and
    /// lasts longer.
    fn renewed(self, now: u32, ticks: u32) -> Span {
        let until = now + 1 + ticks;
        if self.covers(now as f32 + 1.0) && self.until >= until { self } else { Span { from: now + 1, until } }
    }
}

impl Chilled {
    /// A hit at `now` applies `chill`, from the next tick. A new slow replaces a running one if
    /// it's at least as strong; a root extends a running one.
    pub fn apply(&mut self, chill: Chill, now: u32) {
        if chill.slow > 0.0 && chill.slow_ticks > 0 {
            let running = self.slowed.covers(now as f32 + 1.0);
            if !running || chill.slow >= self.slow {
                self.slowed = self.slowed.renewed(now, chill.slow_ticks);
                self.slow = chill.slow;
            }
        }
        if chill.root_ticks > 0 {
            self.rooted = self.rooted.renewed(now, chill.root_ticks);
        }
    }

    /// What's left of a player's speed at `tick` (1 unslowed).
    pub fn speed_factor(&self, tick: u32) -> f32 {
        if self.slowed.covers(tick as f32) { 1.0 - self.slow } else { 1.0 }
    }

    pub fn rooted_at(&self, tick: u32) -> bool {
        self.rooted.covers(tick as f32)
    }
}

/// Walking faster: `HASTE_FACTOR` times as fast during the span. From a haste pickup.
/// Server-authoritative like `Chilled` (in ticks for the same reason).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct Hasted(pub Span);

impl Hasted {
    /// Hasted from the tick after `now` for `HASTE_TICKS` (or the rest of a longer one running).
    pub fn start(&mut self, now: u32) {
        self.0 = self.0.renewed(now, crate::config::HASTE_TICKS);
    }

    /// How many times its walking speed a player has at `tick` (1 unhasted).
    pub fn speed_factor(&self, tick: u32) -> f32 {
        if self.0.covers(tick as f32) { crate::config::HASTE_FACTOR } else { 1.0 }
    }
}

/// A rush of speed from killing zombies: walking and attacking `strength` faster (0.1 = 10%)
/// from `from`, fading linearly to nothing at `until` (ticks). Each zombie a fighter kills in a
/// match starts it again, stronger by the zombies killed before (`FRENZY_PER_KILL` each; counted
/// in `Member::zombie_kills`). Server-authoritative and replicated: an attack's speed is fixed as it
/// starts (`Swing::haste`), and the action bar shows it.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct Frenzy {
    pub from: u32,
    pub until: u32,
    pub strength: f32,
}

impl Frenzy {
    /// The frenzy from a zombie kill at `now`, after `kills` others this match, for a fighter
    /// that fights in `melee` or not (a melee fighter's lasts longer).
    pub fn after_kill(now: u32, kills: u32, melee: bool) -> Frenzy {
        let ticks = if melee { crate::config::FRENZY_MELEE_TICKS } else { crate::config::FRENZY_RANGED_TICKS };
        let strength = crate::config::FRENZY_BASE + crate::config::FRENZY_PER_KILL * kills as f32;
        Frenzy { from: now, until: now + ticks, strength }
    }

    /// How much faster it makes its fighter at `tick` (a fractional tick, for drawing in between):
    /// 0 once it's faded.
    pub fn at(&self, tick: f32) -> f32 {
        let (from, until) = (self.from as f32, self.until as f32);
        if tick < from || tick >= until { 0.0 } else { self.strength * (until - tick) / (until - from) }
    }
}

/// What a pickup does to whoever takes it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum PickupKind {
    /// Restores `HEAL_FRACTION` of max health (taken even at full health).
    Heal,
    /// Walking `HASTE_FACTOR` times as fast for `HASTE_TICKS`.
    Haste,
    /// A treasure chest, in the waves mode: gold for everyone in the run. Lies out in the wilds
    /// during a wave, gone once taken or the wave's over.
    Chest,
    /// A blood shrine: a frenzy (`SHRINE_FRENZY`), walking and attacking faster for a while.
    Shrine,
}

/// A monster made stronger than its kind, in the waves mode: bigger, tougher, worth more, and
/// with a twist of its own. Server-authoritative and replicated (the client draws it bigger, in
/// its twist's color).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct Elite(pub Affix);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum Affix {
    /// Always hasted.
    Swift,
    /// Heals by what it deals.
    Vampiric,
    /// Blows up where it dies, a moment later.
    Volatile,
    /// Its hits freeze for a moment.
    Frostbound,
    /// Splits into smaller monsters as it dies.
    Splitting,
}

impl Affix {
    pub const ALL: [Affix; 5] = [Affix::Swift, Affix::Vampiric, Affix::Volatile, Affix::Frostbound, Affix::Splitting];

    pub fn label(self) -> &'static str {
        match self {
            Affix::Swift => "Swift",
            Affix::Vampiric => "Vampiric",
            Affix::Volatile => "Volatile",
            Affix::Frostbound => "Frostbound",
            Affix::Splitting => "Splitting",
        }
    }
}

/// The shopkeeper, in the waves mode: a fighter no one can hurt, standing at the map's camp
/// (`Map::shop`). Near them, players buy and sell items. Replicated, so the client draws them as
/// the shop.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Shopkeeper;

/// How close to the shopkeeper a player has to be to trade.
pub const SHOP_RADIUS: f32 = 4.0;

/// A player down in a run against the waves (they have one life a wave), `rescue` of the way
/// (0 to 1) to being picked back up by an ally standing over them for `REVIVE_TICKS`. Stepping
/// off loses it. Server-authoritative and replicated: the client marks where they lie, and how
/// far along their rescue is. Gone once they're back up.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct Downed {
    pub rescue: f32,
}

/// How close (center to center) an ally has to stand to pick a downed player up, and for how long.
pub const REVIVE_RADIUS: f32 = 1.6;
pub const REVIVE_TICKS: u32 = 3 * crate::config::TICK_HZ as u32;

/// What an effect looks like: its color and feel.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum FxColor {
    Fire,
    Spirit,
    Frost,
    Gold,
    Blood,
    Poison,
    Void,
    Storm,
    Thorn,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum FxKind {
    /// An arc of lightning from `at` to `to`.
    Arc,
    /// A burst out to `radius` around `at`, at once.
    Blast(FxColor),
    /// Ground marked out to `radius` around `at` until `until`: a frost patch, a blizzard, a
    /// vortex, a healing circle.
    Zone(FxColor),
    /// Something about to land at `at` (out to `radius`) at `until`: a keg's fuse, a strike from
    /// the sky, a bloater swelling. Drawn filling up until then.
    Warn(FxColor),
    /// A keg flying from `at` to `to`, landing at `until`.
    Lob,
    /// Lightning down from the sky onto `at`.
    Bolt,
    /// Gold flying up from `at`.
    Coins,
    /// A monster clawing its way up out of the ground at `at`.
    Rise,
    /// Rime left on the ground out to `radius` round `at` until `until` (Hoarfrost Boots): a
    /// frost rune, ice shards bursting up and sinking again as it thaws, a cold mist.
    Rime,
}

/// A passing effect of an item or a monster, for the client to draw: what, where, and from when
/// to when (ticks). Server-authoritative, replicated; despawned once it's over. Gameplay never
/// reads it: what it shows has already happened on the server, or is about to.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Fx {
    pub kind: FxKind,
    pub at: Vec2,
    pub to: Vec2,
    pub radius: f32,
    pub from: u32,
    pub until: u32,
}

impl Fx {
    /// An effect at one spot, out to `radius`, from `now` for `ticks`.
    pub fn at(kind: FxKind, at: Vec2, radius: f32, now: u32, ticks: u32) -> Fx {
        Fx { kind, at, to: at, radius, from: now, until: now + ticks }
    }

    /// An effect from `at` to `to` (an arc, a lob), from `now` for `ticks`.
    pub fn between(kind: FxKind, at: Vec2, to: Vec2, radius: f32, now: u32, ticks: u32) -> Fx {
        Fx { kind, at, to, radius, from: now, until: now + ticks }
    }
}

/// A pickup lying at `at` (one of its map's `pickups`), or, while `back_at` is set, taken (last
/// by `taken_by`) and gone until that tick. Server-authoritative: the server decides who takes
/// it, clients draw it, its timer, and its taker's burst.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Pickup {
    pub kind: PickupKind,
    pub at: Vec2,
    pub back_at: Option<u32>,
    pub taken_by: Option<PeerId>,
}

/// A pair of waygates (one of its map's `gates`): a fighter stepping onto either end is put on
/// the other, and both close until `back_at` (the tick they open again; `None`: open, never
/// used). Server-authoritative, like a pickup: the server decides who goes through; clients draw
/// the gates, whether they're open, and a flash on both ends when `back_at` changes.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Waygate {
    pub ends: [Vec2; 2],
    pub back_at: Option<u32>,
}

impl Waygate {
    /// How many ticks (fractional, for drawing between them) until it opens at `tick`: 0 while
    /// it's open.
    pub fn closed_for(&self, tick: f32) -> f32 {
        self.back_at.map_or(0.0, |back_at| (back_at as f32 - tick).max(0.0))
    }

    pub fn open_at(&self, tick: u32) -> bool {
        self.closed_for(tick as f32) == 0.0
    }
}

/// The player's most recent melee swing, for drawing it: when, which way, and whether its
/// `follow_up` empowered it. Server-authoritative and replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct LastSwing {
    pub tick: u32,
    pub dir: Vec2,
    pub empowered: bool,
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Projectile {
    pub owner: PeerId,
    /// The shooter's class: speed, size, range and damage come from its attack (or ability).
    pub class: ClassId,
    /// Where it was thrown from; with `dir` and `spawn_tick` that says where it is at any tick
    /// (`sim::projectile_pos`).
    pub origin: Vec2,
    pub dir: Vec2,
    pub spawn_tick: u32,
    /// Thrown with the ability in this slot, rather than the auto-attack (`None`).
    pub ability: Option<AbilitySlot>,
    /// A primed auto-attack's (`AttackState::primed`): sure to crit. Set by the server.
    pub crit: bool,
}

impl Projectile {
    /// Everything a newly fired projectile spawns with on the server.
    pub fn bundle(self) -> impl Bundle {
        (Name::from("Projectile"), Pos(self.origin), self)
    }
}

/// What a player wants to do: kept by the server on its fighter, changed by the `PlayerCommand`s
/// its client sends (the moment they arrive), and what the sim runs on each tick. Clients never
/// send positions, only intent.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerInput {
    /// Point the player right-clicked to walk to (`Map::walk_target` of the click). The sim
    /// pathfinds there; `None` stands still.
    pub move_to: Option<Vec2>,
    /// Keys held to walk (WASD, free camera): this way on the gameplay plane, overriding
    /// `move_to`. The sim normalizes it; zero means walk to `move_to` instead.
    pub walk: Vec2,
    /// Aim direction on the gameplay plane; the sim normalizes it.
    pub aim: Vec2,
    /// Left click: auto-attack toward `aim`, if the last attack is done (else the click is
    /// dropped). Set by `PlayerCommand::Attack`, used up by the next tick.
    pub fire: bool,
    /// Abilities' keys pressed since the last tick, in order (`order_cast`), each used toward its
    /// own aim. Set by `PlayerCommand::Cast`, used up by the next tick, whether or not they could
    /// be used (as in League).
    pub casts: [Option<CastOrder>; MAX_ABILITIES],
}

impl PlayerInput {
    /// Orders the ability in `slot` used toward `aim`, after any ordered before it. Pressed again
    /// before it's used, it keeps its place in line and takes the new aim.
    pub fn order_cast(&mut self, slot: AbilitySlot, aim: Vec2) {
        let order = CastOrder { slot, aim };
        if let Some(same) = self.casts.iter_mut().flatten().find(|o| o.slot == slot) {
            *same = order;
        } else if let Some(free) = self.casts.iter_mut().find(|o| o.is_none()) {
            *free = Some(order);
        }
    }

    /// The abilities ordered used, in order.
    pub fn casts(&self) -> impl Iterator<Item = CastOrder> + '_ {
        self.casts.iter().flatten().copied()
    }
}

/// What a client tells the server its player should do, League style: sent when it happens, and
/// applied by the server the moment it arrives (not scheduled for a future tick). Nothing is
/// predicted: the client sees the result once the server's next update comes back.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum PlayerCommand {
    /// Right click: walk to this point (`Map::walk_target` of the click).
    MoveTo(Vec2),
    /// Stop walking (S).
    Stop,
    /// Keys held to walk (WASD, free camera), this way; zero when none are.
    Walk(Vec2),
    /// Left click: auto-attack toward this aim. Holding the button doesn't attack again: each
    /// attack is its own click, once the last one is done.
    Attack(Vec2),
    /// An ability's key (Q, W, E, R): use the ability in this slot toward this aim.
    Cast(AbilitySlot, Vec2),
}

impl PlayerCommand {
    /// Is it an order to move (or stop)? One cancels an attack still winding up, as in League
    /// (`sim::cancel_attack`), and drops a click no tick has seen yet: the last order wins.
    pub fn moves(&self) -> bool {
        match *self {
            PlayerCommand::MoveTo(_) | PlayerCommand::Stop => true,
            PlayerCommand::Walk(dir) => dir != Vec2::ZERO,
            PlayerCommand::Attack(_) | PlayerCommand::Cast(..) => false,
        }
    }

    /// What the command does to a player's intent.
    pub fn apply(self, input: &mut PlayerInput) {
        if self.moves() {
            input.fire = false;
        }
        match self {
            PlayerCommand::MoveTo(point) => {
                input.move_to = Some(point);
                input.walk = Vec2::ZERO;
            }
            PlayerCommand::Stop => input.move_to = None,
            PlayerCommand::Walk(dir) => {
                input.walk = dir;
                if dir != Vec2::ZERO {
                    input.move_to = None;
                }
            }
            PlayerCommand::Attack(aim) => {
                input.aim = aim;
                input.fire = true;
            }
            PlayerCommand::Cast(slot, aim) => input.order_cast(slot, aim),
        }
    }
}

/// The class picked in the room's lobby. Once the match has started, the server spawns our
/// fighter as this class when it arrives (or at the start, the last one picked before it).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ChooseClass(pub ClassId);

/// First thing after connecting: who we were last time (kept by the browser), to be called that
/// again if the name is free. The server answers `LobbyEvent::Welcome`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hello {
    pub guest_name: Option<String>,
}

/// Asks about rooms, from the browser or a room's lobby. The server answers with
/// `LobbyEvent::Room`, or `LobbyEvent::Refused` saying why not.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RoomRequest {
    /// Make a room on `map` and lead it.
    Create { name: String, mode: Mode, map: MapId },
    Join(RoomKey),
    /// Join the room called this, or make it and start it right away: for bots and tests.
    QuickJoin(String),
    /// A room of our own on this map, never listed, straight into its arena.
    Practice(MapId),
    Leave,
    /// Into the room's arena, once its match is on, as our picked class.
    EnterArena,
    /// Out of the arena, back to the room's lobby (still in the room).
    LeaveArena,
    /// Leader only, before the start.
    SetMode(Mode),
    /// Leader only, before the start; in practice, any time (everything starts over on it).
    SetMap(MapId),
    SetTeam(u8),
    /// Leader only.
    Start,
    /// Practice only: a target dummy at this point, the next class in turn: standing there, or
    /// (`walks`) walking back and forth along a line through it.
    PlaceDummy { at: Vec2, walks: bool },
    /// Practice only: our abilities with no cooldowns of their own (ready again the moment
    /// they're used), or back to normal.
    NoCooldowns(bool),
    /// Waves only, near the shopkeeper: buy this item, or raise it a level.
    Buy(crate::items::Item),
    /// Waves only, near the shopkeeper: sell the item in this slot.
    Sell(u8),
    /// Waves only, between waves: we're ready for the next (or not, again).
    Ready(bool),
}

/// What the server tells a client about guests and rooms.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum LobbyEvent {
    /// Who we are, for as long as we're connected.
    Welcome { guest_id: u32, name: String },
    /// Every room, while we're in the browser: on arrival and whenever it changes.
    RoomList(Vec<RoomSummary>),
    /// The room we're in, whenever anything in it changes.
    Room(RoomView),
    /// We're out of our room, back to the browser.
    Left(LeaveReason),
    /// A request that couldn't be done.
    Refused(String),
}

/// Reliable, ordered channel for the few messages we send.
pub struct Reliable;

pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_channel::<Reliable>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);
        app.register_message::<PlayerCommand>().add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ChooseClass>().add_direction(NetworkDirection::ClientToServer);
        app.register_message::<Hello>().add_direction(NetworkDirection::ClientToServer);
        app.register_message::<RoomRequest>().add_direction(NetworkDirection::ClientToServer);
        app.register_message::<LobbyEvent>().add_direction(NetworkDirection::ServerToClient);

        app.component::<PlayerId>().replicate();
        app.component::<ClassId>().replicate();
        app.component::<Team>().replicate();
        app.component::<Health>().replicate();
        app.component::<Chilled>().replicate();
        app.component::<RecentHits>().replicate();
        app.component::<Hasted>().replicate();
        app.component::<Frenzy>().replicate();
        app.component::<Pickup>().replicate();
        app.component::<MaxHealth>().replicate();
        app.component::<crate::items::Satchel>().replicate();
        app.component::<Elite>().replicate();
        app.component::<Shopkeeper>().replicate();
        app.component::<Downed>().replicate();
        app.component::<Fx>().replicate();
        app.component::<Waygate>().replicate();
        // Others' fighters are interpolated (smooth, a little in the past); our own isn't: it's
        // shown as the server last said it was, for the least delay after a command.
        app.component::<Pos>().replicate().add_interpolation_with(lerp_or_jump);
        app.component::<AttackState>().replicate().add_interpolation_with(hold);
        app.component::<LastSwing>().replicate().add_interpolation_with(hold);
        app.component::<Cooldowns>().replicate().add_interpolation_with(hold);
        app.component::<Dashing>().replicate().add_interpolation_with(hold);
        // Projectiles fly on rails: clients draw them from this alone (`sim::projectile_pos`).
        app.component::<Projectile>().replicate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FRENZY_MELEE_TICKS, FRENZY_RANGED_TICKS};

    #[test]
    fn frenzy_grows_with_kills_and_fades() {
        let first = Frenzy::after_kill(100, 0, false);
        assert!((first.strength - 0.1).abs() < 1e-6);
        // After ten kills, the next one's is three times the first.
        assert!((Frenzy::after_kill(100, 10, false).strength - 0.3).abs() < 1e-6);
        assert_eq!(first.until - first.from, FRENZY_RANGED_TICKS);
        assert_eq!(Frenzy::after_kill(100, 0, true).until - 100, FRENZY_MELEE_TICKS);
        let half = 100.0 + FRENZY_RANGED_TICKS as f32 / 2.0;
        assert!((first.at(half) - 0.05).abs() < 1e-6, "fades linearly");
        assert_eq!(first.at(99.0), 0.0);
        assert_eq!(first.at(first.until as f32), 0.0);
    }

    #[test]
    fn a_hasted_swing_is_quicker() {
        let attack = &crate::classes::ClassId(0).def().attack;
        let plain = Swing { started_at: 0, dir: Vec2::X, quick: false, hastened: false, haste: 0.0, primed: false };
        let hasted = Swing { haste: 0.3, ..plain };
        assert!(hasted.windup_ticks(attack) < plain.windup_ticks(attack));
        assert!(hasted.ready_at(attack) < plain.ready_at(attack));
    }
}
