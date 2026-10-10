//! Wire protocol: replicated components, player inputs and messages.

use bevy::ecs::entity::MapEntities;
use bevy::math::Curve;
use bevy::prelude::*;
use lightyear::input::prelude::InputConfig;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

pub use crate::classes::ClassId;
use crate::classes::Chill;

/// Marks a player entity and says which client controls it. Projectiles don't carry it (their
/// owner is `Projectile::owner`), so `With<PlayerId>` always means "a player".
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct PlayerId(pub PeerId);

/// Position on the 2D gameplay plane (the client renders it in 3D, see `render.rs`).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect, Deref, DerefMut)]
pub struct Pos(pub Vec2);

impl Ease for Pos {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| Pos(start.0.lerp(end.0, t)))
    }
}

/// Server-authoritative; replicated but never predicted. The maximum is the class's `max_hp`.
/// Zero means dead and waiting to respawn: hidden, and can't move or attack.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Health(pub i32);

impl Health {
    /// The one rule for "in the fight" on the client: prediction, drawing and the bot use it.
    pub fn alive(&self) -> bool {
        self.0 > 0
    }
}

/// The last few hits a player took, for the damage numbers over it. Server-authoritative and
/// replicated like `Health`. Every hit gets the next running number, so a client shows each one
/// once, even when updates arrive bunched or skip a state.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default, Reflect)]
pub struct RecentHits(pub Vec<Hit>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Hit {
    pub seq: u32,
    pub amount: i32,
    pub crit: bool,
}

impl RecentHits {
    /// How many hits are kept: more than can land between two updates (every 50 ms).
    const KEEP: usize = 4;

    /// The running number of the latest hit (0 before the first).
    pub fn seq(&self) -> u32 {
        self.0.last().map_or(0, |hit| hit.seq)
    }

    pub fn push(&mut self, amount: i32, crit: bool) {
        let seq = self.seq() + 1;
        if self.0.len() == Self::KEEP {
            self.0.remove(0);
        }
        self.0.push(Hit { seq, amount, crit });
    }
}

/// The auto-attack's state: when the next one may start, the windup in progress (if any), and
/// when the last one went off. Predicted, so your own windup starts instantly and rollbacks
/// restore it; replicated (and shown on the same delayed timeline as others' positions), so
/// others see your windup coming.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct AttackState {
    pub ready_at: u32,
    pub windup: Option<Windup>,
    /// The tick the last attack went off (`None`: never). Kept rather than worked out from
    /// `ready_at`, which a Rift Step hit resets.
    pub released_at: Option<u32>,
}

/// An attack winding up: aim locked toward `dir`, goes off `windup_ticks` after `started_at`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Windup {
    pub started_at: u32,
    pub dir: Vec2,
}

impl Windup {
    /// The tick the attack goes off.
    pub fn releases_at(&self, class: ClassId) -> u32 {
        self.started_at + class.def().attack.windup_ticks
    }

    /// How far along the windup is at `now` (a fractional tick), from 0 to 1.
    pub fn progress(&self, now: f32, class: ClassId) -> f32 {
        let windup = class.def().attack.windup_ticks.max(1) as f32;
        ((now - self.started_at as f32) / windup).clamp(0.0, 1.0)
    }
}

/// Interpolation for state that jumps instead of blending: others' attack state and swings
/// change on the same delayed timeline as their positions, so a windup shows when their body
/// stops, not a round of interpolation delay earlier.
fn hold<C>(start: C, _end: C, _t: f32) -> C {
    start
}

/// The Q ability's state: when it may be used again, and a dash in progress. Predicted (your
/// own Q happens at the press), replicated and shown on the same delayed timeline as others'
/// positions, like `AttackState`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct AbilityState {
    pub ready_at: u32,
    pub dash: Option<Dash>,
}

impl AbilityState {
    /// When the ability was last used (`None`: never).
    pub fn used_at(&self, class: ClassId) -> Option<u32> {
        (self.ready_at > 0).then(|| self.ready_at.saturating_sub(class.def().ability.cooldown_ticks))
    }
}

/// A dash: moving toward `dir` for the class's dash ticks after `started_at`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Dash {
    pub started_at: u32,
    pub dir: Vec2,
}

/// Crowd control on a player: how much it's slowed and when, and when it's frozen in place
/// (rooted). Server-authoritative like `Health`: replicated, never predicted. The spans are in
/// ticks, so a client's own predicted movement replays them exactly after a rollback (a span
/// that hadn't started yet doesn't apply to the ticks before it), and others' are shown on the
/// timeline they're drawn on.
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

/// The player's most recent melee swing, for drawing it. Predicted, so your own swing shows
/// instantly, and replicated, so others see it too.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct LastSwing {
    pub tick: u32,
    pub dir: Vec2,
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
    /// Thrown with the Q ability rather than the auto-attack.
    pub ability: bool,
}

impl Projectile {
    /// Everything a newly fired projectile spawns with, on both client and server. The shared
    /// `PreSpawned` hash is what matches the server's copy to the one the client predicted.
    pub fn bundle(self) -> impl Bundle {
        (
            Name::from("Projectile"),
            Pos(self.origin),
            self,
            PreSpawned::new(crate::sim::projectile_prespawn_hash(&self)),
        )
    }
}

/// Everything a client sends each tick. Clients never send positions, only intent.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerInput {
    /// Tile the player right-clicked to walk to. The sim pathfinds there; `None` stands still.
    pub move_to: Option<IVec2>,
    /// Keys held to walk (WASD, free camera): this way on the gameplay plane, overriding
    /// `move_to`. The sim normalizes it; zero means walk to `move_to` instead.
    pub walk: Vec2,
    /// Aim direction on the gameplay plane; the sim normalizes it.
    pub aim: Vec2,
    /// Left mouse held: auto-attack toward `aim`.
    pub fire: bool,
    /// Q pressed: use the ability toward `aim`. Sent for one tick per press.
    pub ability: bool,
}

impl MapEntities for PlayerInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

/// Sent once after connecting: the class picked on the join screen. The server spawns the
/// player when it arrives.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ChooseClass(pub ClassId);

/// Reliable, ordered channel for the few messages we send.
pub struct Reliable;

pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(input::native::InputPlugin::<PlayerInput> {
            // Inputs also carry the client's interpolation delay, so the server can judge hits
            // against where the attacker actually saw their target (lag compensation).
            config: InputConfig { lag_compensation: true, ..default() },
        });

        app.add_channel::<Reliable>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ChooseClass>().add_direction(NetworkDirection::ClientToServer);

        app.component::<PlayerId>().replicate();
        app.component::<ClassId>().replicate();
        app.component::<Health>().replicate();
        app.component::<Chilled>().replicate();
        app.component::<RecentHits>().replicate();
        app.component::<Pos>()
            .replicate()
            .predict()
            .add_linear_interpolation();
        app.component::<AttackState>().replicate().predict().add_interpolation_with(hold);
        app.component::<LastSwing>().replicate().predict().add_interpolation_with(hold);
        app.component::<AbilityState>().replicate().predict().add_interpolation_with(hold);
        // Every client predicts every projectile, not just its own: they fly on rails, so each
        // client can draw them where they really are, on the same clock as its own player.
        app.component::<Projectile>().replicate().predict();
    }
}
