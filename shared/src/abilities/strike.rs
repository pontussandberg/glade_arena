//! A strike: a shell called down onto the aimed spot (clamped to `range`), landing `fuse_ticks`
//! later — time to step out of its warning ring. Everyone of the caster's foes within `radius`
//! when it lands takes `damage` and `chill`. What falls is the `shell`'s look: a meteor, a
//! lightning bolt, a lobbed bomb.

use serde::Deserialize;

use crate::classes::{Chill, Shell};

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct StrikeDef {
    pub range: f32,
    pub radius: f32,
    pub damage: i32,
    pub fuse_ticks: u32,
    #[serde(default)]
    pub chill: Chill,
    #[serde(default)]
    pub shell: Shell,
}
