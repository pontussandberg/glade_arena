//! A prime: used at once (no aim, no windup), it readies the user's next auto-attack, which is
//! sure to crit and has `attack_speed` more attack speed all through. It lasts until that attack
//! goes off (one cancelled first leaves it ready), and is used up then (`sim::release_attack`).

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct PrimeDef {
    /// More attack speed for the primed attack, as a share of the normal (0.5 = 50% more).
    pub attack_speed: f32,
}
