//! Playable classes, loaded from `assets/classes.ron`.
//!
//! Gameplay code never names a class: it reads a `ClassDef` (HP, speed, auto-attack) through
//! the player's `ClassId`. Adding or retuning a class is a data change.

use std::sync::LazyLock;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::{PLAYER_RADIUS, TICK_HZ};

/// The class file, compiled into both server and client so they can't disagree.
pub const CLASSES_RON: &str = include_str!("../assets/classes.ron");

#[derive(Deserialize, Debug, Clone)]
pub struct ClassDef {
    /// Stable key; the client also uses it to pick the class's look.
    pub id: String,
    pub name: String,
    pub role: String,
    pub blurb: String,
    pub max_hp: i32,
    pub move_speed: f32,
    pub attack: AttackDef,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AttackDef {
    /// Damage on hit (for projectiles with `far_damage`: point blank).
    pub damage: i32,
    pub cooldown_ticks: u32,
    /// Ticks from starting an attack to it going off; you stand still meanwhile.
    pub windup_ticks: u32,
    pub kind: AttackKind,
}

#[derive(Deserialize, Debug, Clone)]
pub enum AttackKind {
    /// Hits everyone in a cone in front of the attacker.
    Melee { range: f32, arc_degrees: f32 },
    /// Flies straight until it hits someone, a blocking tile, or runs out of range. With
    /// `far_damage`, damage scales linearly from the attack's `damage` (point blank) to it (after
    /// flying the full range).
    Projectile {
        speed: f32,
        radius: f32,
        range: f32,
        #[serde(default)]
        far_damage: Option<i32>,
    },
}

impl AttackDef {
    /// Damage of a hit after the attack flew `distance`.
    pub fn damage_at(&self, distance: f32) -> i32 {
        let AttackKind::Projectile { range, far_damage: Some(far), .. } = self.kind else { return self.damage };
        let t = (distance / range).clamp(0.0, 1.0);
        (self.damage as f32 + (far - self.damage) as f32 * t).round() as i32
    }
}

impl AttackKind {
    /// How far from your center the attack can hit a target's center: a swing reaches the
    /// target's body, a projectile flies its range.
    pub fn reach(&self) -> f32 {
        match *self {
            AttackKind::Melee { range, .. } => range + PLAYER_RADIUS,
            AttackKind::Projectile { range, .. } => range,
        }
    }

    /// (speed, radius) for projectile attacks.
    pub fn projectile(&self) -> Option<(f32, f32)> {
        match *self {
            AttackKind::Projectile { speed, radius, .. } => Some((speed, radius)),
            AttackKind::Melee { .. } => None,
        }
    }

    /// How many ticks a projectile flies before it's out of range (0 for melee).
    pub fn lifetime_ticks(&self) -> u32 {
        self.projectile().map_or(0, |(speed, _)| (self.reach() / speed * TICK_HZ as f32).ceil() as u32)
    }
}

/// Which class a player (or a projectile's shooter) is: an index into the class file.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub struct ClassId(pub u8);

impl ClassId {
    pub fn def(self) -> &'static ClassDef {
        &classes()[self.0 as usize]
    }

    /// `None` for ids that aren't in the class file (e.g. from a hacked client).
    pub fn checked(self) -> Option<Self> {
        ((self.0 as usize) < classes().len()).then_some(self)
    }

    pub fn by_key(key: &str) -> Option<Self> {
        classes().iter().position(|c| c.id == key).map(|i| ClassId(i as u8))
    }

    pub fn all() -> impl Iterator<Item = ClassId> {
        (0..classes().len() as u8).map(ClassId)
    }
}

static CLASSES: LazyLock<Vec<ClassDef>> =
    LazyLock::new(|| ron::from_str(CLASSES_RON).unwrap_or_else(|e| panic!("assets/classes.ron is invalid: {e}")));

pub fn classes() -> &'static [ClassDef] {
    &CLASSES
}

/// FNV-1a over the class file, folded into the protocol id.
pub const fn classes_hash() -> u64 {
    let bytes = CLASSES_RON.as_bytes();
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u64).wrapping_mul(0x0100_0000_01b3);
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_file_is_sane() {
        let all = classes();
        assert!(!all.is_empty() && all.len() <= u8::MAX as usize);
        for (i, c) in all.iter().enumerate() {
            assert!(all[..i].iter().all(|o| o.id != c.id), "duplicate class id {}", c.id);
            assert!((1..=1000).contains(&c.max_hp), "{}: max_hp", c.id);
            assert!((1.0..=12.0).contains(&c.move_speed), "{}: move_speed", c.id);
            assert!(c.attack.damage > 0 && c.attack.cooldown_ticks > 0, "{}: attack", c.id);
            assert!(c.attack.windup_ticks < c.attack.cooldown_ticks, "{}: windup as long as the cooldown", c.id);
            match c.attack.kind {
                AttackKind::Melee { range, arc_degrees } => {
                    assert!((0.5..=4.0).contains(&range) && (10.0..=360.0).contains(&arc_degrees), "{}: melee", c.id);
                }
                AttackKind::Projectile { speed, radius, range, far_damage } => {
                    assert!(far_damage.is_none_or(|far| far > 0), "{}: far_damage", c.id);
                    assert!((2.0..=60.0).contains(&speed) && (0.05..=1.0).contains(&radius), "{}: projectile", c.id);
                    assert!((1.0..=30.0).contains(&range), "{}: projectile range", c.id);
                }
            }
        }
    }

    #[test]
    fn ids_round_trip() {
        for id in ClassId::all() {
            assert_eq!(ClassId::by_key(&id.def().id), Some(id));
        }
        assert_eq!(ClassId(200).checked(), None);
    }

    #[test]
    fn far_damage_scales_with_distance() {
        let kind = |far_damage| AttackKind::Projectile { speed: 10.0, radius: 0.2, range: 10.0, far_damage };
        let attack = |kind| AttackDef { damage: 10, cooldown_ticks: 40, windup_ticks: 10, kind };
        assert_eq!([0.0, 5.0, 10.0, 99.0].map(|d| attack(kind(Some(30))).damage_at(d)), [10, 20, 30, 30]);
        assert_eq!(attack(kind(None)).damage_at(5.0), 10);
    }

    #[test]
    fn projectile_lifetime_covers_its_range() {
        let kind = AttackKind::Projectile { speed: 16.0, radius: 0.2, range: 8.0, far_damage: None };
        assert_eq!(kind.lifetime_ticks(), 32);
    }
}
