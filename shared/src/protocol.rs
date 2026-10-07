//! Wire protocol: replicated components and player inputs.

use bevy::ecs::entity::MapEntities;
use bevy::math::Curve;
use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

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

/// Server-authoritative; replicated but never predicted.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Health(pub i32);

/// Tick at which the player may fire again. Predicted so rollbacks restore it.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct FireCooldown {
    pub ready_at: u32,
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Projectile {
    pub owner: PeerId,
    pub dir: Vec2,
    pub spawn_tick: u32,
}

impl Projectile {
    /// Everything a newly fired projectile spawns with, on both client and server. The shared
    /// `PreSpawned` hash is what matches the server's copy to the one the client predicted.
    pub fn bundle(self, pos: Vec2) -> impl Bundle {
        (
            Name::from("Projectile"),
            Pos(pos),
            self,
            PreSpawned::new(crate::sim::projectile_prespawn_hash(self.owner, self.spawn_tick)),
        )
    }
}

/// Everything a client sends each tick. Clients never send positions, only intent.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerInput {
    /// Raw movement intent; the sim clamps and normalizes it.
    pub movement: Vec2,
    /// Aim direction on the gameplay plane; the sim normalizes it.
    pub aim: Vec2,
    pub fire: bool,
}

impl MapEntities for PlayerInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(input::native::InputPlugin::<PlayerInput>::default());

        app.component::<PlayerId>().replicate();
        app.component::<Health>().replicate();
        app.component::<Pos>()
            .replicate()
            .predict()
            .add_linear_interpolation();
        app.component::<FireCooldown>().replicate().predict();
        app.component::<Projectile>().replicate().predict();
    }
}
