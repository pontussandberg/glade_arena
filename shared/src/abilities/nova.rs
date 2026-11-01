//! A nova: a burst around the user, at once: everyone within `radius` (center to body, not
//! behind a wall) takes `damage` and `chill`.

use bevy::prelude::*;
use serde::Deserialize;

use crate::classes::Chill;
use crate::config::PLAYER_RADIUS;
use crate::map::Map;

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct NovaDef {
    pub radius: f32,
    pub damage: i32,
    #[serde(default)]
    pub chill: Chill,
}

impl NovaDef {
    /// Does it, burst at `caster`, reach `target`, each with how high its feet are? Its radius
    /// out to the target's body, no further up or down, and not through a wall. Only the server
    /// decides this.
    pub fn hits(&self, map: &Map, (caster, from): (Vec2, f32), (target, feet): (Vec2, f32)) -> bool {
        caster.distance(target) <= self.radius + PLAYER_RADIUS && (feet - from).abs() <= self.radius && map.shot_clear((caster, from), (target, feet))
    }
}
