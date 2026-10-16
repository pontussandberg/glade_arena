//! A thrown ability: a projectile thrown instantly toward the aim (no windup, no root), for a
//! fixed damage. It flies like an auto-attack's (`sim::projectile_pos`).

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use serde::Deserialize;

use super::AbilitySlot;
use crate::classes::{ClassId, Shot};
use crate::protocol::Projectile;
use crate::sim::shot_spawn;

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Throw {
    pub speed: f32,
    pub radius: f32,
    pub range: f32,
    pub damage: i32,
}

impl Throw {
    pub fn shot(&self) -> Shot {
        Shot { speed: self.speed, radius: self.radius, range: self.range }
    }

    /// Thrown by `owner` (of `class`, with the ability in `slot`) from `pos` toward `dir` at
    /// `tick`.
    pub fn projectile(&self, owner: PeerId, class: ClassId, slot: AbilitySlot, pos: Vec2, dir: Vec2, tick: u32) -> Projectile {
        Projectile { owner, class, origin: shot_spawn(pos, dir, self.radius), dir, spawn_tick: tick, ability: Some(slot), crit: false, extra: false }
    }
}
