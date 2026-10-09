//! Wire protocol: replicated components, player inputs and messages.

use bevy::ecs::entity::MapEntities;
use bevy::math::Curve;
use bevy::prelude::*;
use lightyear::input::prelude::InputConfig;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

pub use crate::classes::ClassId;

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
    pub dir: Vec2,
    pub spawn_tick: u32,
    /// Thrown with the Q ability rather than the auto-attack.
    pub ability: bool,
}

impl Projectile {
    /// Everything a newly fired projectile spawns with, on both client and server. The shared
    /// `PreSpawned` hash is what matches the server's copy to the one the client predicted.
    pub fn bundle(self, pos: Vec2) -> impl Bundle {
        (
            Name::from("Projectile"),
            Pos(pos),
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
        app.component::<Pos>()
            .replicate()
            .predict()
            .add_linear_interpolation();
        app.component::<AttackState>().replicate().predict().add_interpolation_with(hold);
        app.component::<LastSwing>().replicate().predict().add_interpolation_with(hold);
        app.component::<AbilityState>().replicate().predict().add_interpolation_with(hold);
        app.component::<Projectile>().replicate().predict();
    }
}
