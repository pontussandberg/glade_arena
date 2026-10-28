//! A dash toward the aim, through fighters, cutting each one crossed for `damage`; it stops at
//! walls. Until the dasher's body reaches the cursor (`DashDef::length`), at least
//! `min_distance` (if given; without it, always all the way) and at most `distance`. Covers
//! `distance` in `ticks` at normal speed (longer slowed, shorter hasted), so a shorter one is
//! over sooner; eased out, most of the way is covered early and it comes to rest softly.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::AbilitySlot;
use crate::config::PLAYER_RADIUS;
use crate::map::Map;

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct DashDef {
    pub distance: f32,
    #[serde(default)]
    pub min_distance: Option<f32>,
    pub ticks: u32,
    pub damage: i32,
}

impl DashDef {
    /// How far a dash aimed `aim` (from the dasher to the cursor) goes: until the front of the
    /// dasher's body reaches the cursor (where its aim indicator, the ground its body sweeps,
    /// ends), within its `min_distance` and `distance`; or all the way without a `min_distance`.
    pub fn length(&self, aim: Vec2) -> f32 {
        match self.min_distance {
            Some(min) => (aim.length() - PLAYER_RADIUS).clamp(min, self.distance),
            None => self.distance,
        }
    }

    /// How far it goes a tick at normal speed.
    pub fn pace(&self) -> f32 {
        self.distance / self.ticks as f32
    }

    /// The dash the ability in `slot` starts at `tick`, toward `dir`, aimed `aim`.
    pub fn start(&self, slot: AbilitySlot, dir: Vec2, aim: Vec2, tick: u32) -> Dash {
        let length = self.length(aim);
        Dash { started_at: tick, dir, length, left: length, slot }
    }
}

/// A dash in progress: moving toward `dir` from `started_at` until it has covered its `length`
/// (`left` is how much of it is still to go). Started by the ability in `slot`: its pace, and
/// the damage it cuts for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Dash {
    pub started_at: u32,
    pub dir: Vec2,
    pub length: f32,
    pub left: f32,
    pub slot: AbilitySlot,
}

impl Dash {
    /// One tick of it (`tick`) from `pos`, at `pace` (its own, slowed or hasted): where the
    /// dasher is after it. It takes as many ticks as its length at that pace would, but eased
    /// out: the way covered by each tick falls off (quadratically) toward the end, so it sets
    /// out fast and comes to rest softly. Counted whether or not a wall (or water's edge,
    /// anything unwalkable) stops it; one whose top is under the dasher's `feet` doesn't
    /// (`Map::line_walkable_at`).
    pub fn step(&mut self, map: &Map, pos: Vec2, pace: f32, tick: u32, feet: f32) -> Vec2 {
        let ticks = (self.length / pace.max(1e-6)).ceil().max(1.0);
        // Stepped first the tick after it started.
        let stepped = tick.saturating_sub(self.started_at + 1) as f32;
        let along = |stepped: f32| 1.0 - (1.0 - (stepped / ticks).min(1.0)).powi(2);
        let step = (self.length * (along(stepped + 1.0) - along(stepped))).min(self.left);
        self.left -= step;
        let next = pos + self.dir * step;
        if map.line_walkable_at(pos, next, feet) { next } else { pos }
    }

    /// Whether it has gone its whole length. Not quite zero: rounding leaves a sliver after the
    /// last full step.
    pub fn over(&self) -> bool {
        self.left < 1e-4
    }
}

/// A fighter's dash in progress, if any. Server-authoritative, replicated and shown like
/// `AttackState`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct Dashing(pub Option<Dash>);

/// Does a dash at `dasher` cut `target`? Bodies touching, and not through a wall.
pub fn hits(map: &Map, dasher: Vec2, target: Vec2) -> bool {
    dasher.distance(target) <= 2.0 * PLAYER_RADIUS && map.shot_clear(dasher, target)
}
