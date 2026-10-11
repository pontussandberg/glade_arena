//! Playable classes, loaded from `assets/classes.ron`.
//!
//! Gameplay code never names a class: it reads a `ClassDef` (HP, speed, auto-attack, Q ability)
//! through the player's `ClassId`. Retuning a class is a data change; a new class also needs its
//! look in the client (`arena.rs` figure and parts, `rig.rs` moves).

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
    pub ability: AbilityDef,
    /// What the class always has going for it, nothing to press (none by default).
    #[serde(default)]
    pub passive: Option<PassiveDef>,
    /// Critical hits (none by default).
    #[serde(default)]
    pub crit: Crit,
}

/// How much a critical hit multiplies its damage.
pub const CRIT_MULTIPLIER: f32 = 2.0;

/// The odds that an auto-attack hit this class deals is critical (a Q never is), for
/// `CRIT_MULTIPLIER` times its damage: `chance` normally, `vs_frozen` against a frozen (rooted) target.
#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq)]
#[serde(default)]
pub struct Crit {
    pub chance: f32,
    pub vs_frozen: f32,
}

impl Crit {
    /// The odds of a crit against a target that's `frozen` or not.
    pub fn chance_against(&self, frozen: bool) -> f32 {
        if frozen { self.chance.max(self.vs_frozen) } else { self.chance }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct AttackDef {
    /// Damage on hit (for projectiles with `far_scale`: point blank, the lowest).
    pub damage: i32,
    pub cooldown_ticks: u32,
    /// Ticks from starting an attack to it going off; you stand still meanwhile. At most
    /// `cooldown_ticks`: as long as it, the attack roots you for its whole cycle.
    pub windup_ticks: u32,
    pub kind: AttackKind,
    /// Crowd control each hit applies (none by default).
    #[serde(default)]
    pub chill: Chill,
    /// A quicker next attack after a hit (none by default).
    #[serde(default)]
    pub follow_up: Option<FollowUp>,
}

/// Landing an auto-attack makes the next one, if started within `within_ticks` of the hit, wind
/// up `faster` (the fraction of the windup cut). That quick one landing doesn't arm another.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct FollowUp {
    pub within_ticks: u32,
    pub faster: f32,
}

/// Crowd control a hit applies on top of its damage: a slow (`slow` is the fraction of speed
/// taken away, for `slow_ticks`) and a root (frozen in place for `root_ticks`: no walking, no
/// dashing). Both start the tick after the hit.
#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq)]
pub struct Chill {
    #[serde(default)]
    pub slow: f32,
    #[serde(default)]
    pub slow_ticks: u32,
    #[serde(default)]
    pub root_ticks: u32,
}

impl Chill {
    pub fn is_none(&self) -> bool {
        (self.slow == 0.0 || self.slow_ticks == 0) && self.root_ticks == 0
    }
}

#[derive(Deserialize, Debug, Clone)]
pub enum AttackKind {
    /// A thrust: hits everyone whose body a straight lane `width` wide, out from the attacker
    /// toward the aim, reaches.
    Melee { range: f32, width: f32 },
    /// Flies straight until it hits someone, a blocking tile, or runs out of range. With
    /// `far_scale`, damage scales linearly from the attack's `damage` (point blank) to `far_scale`
    /// times it (after flying the full range).
    Projectile {
        speed: f32,
        radius: f32,
        range: f32,
        #[serde(default)]
        far_scale: Option<f32>,
    },
}

/// The Q ability.
#[derive(Deserialize, Debug, Clone)]
pub struct AbilityDef {
    pub name: String,
    pub description: String,
    pub cooldown_ticks: u32,
    pub kind: AbilityKind,
}

/// A passive, as players read it. Only words: its effect is in the numbers it describes (e.g.
/// the attack's `far_scale`, the class's `crit`), which its description names by placeholder
/// (`ClassDef::describe`).
#[derive(Deserialize, Debug, Clone)]
pub struct PassiveDef {
    pub name: String,
    pub description: String,
}

#[derive(Deserialize, Debug, Clone)]
pub enum AbilityKind {
    /// Thrown instantly toward the aim (no windup, no root), fixed damage.
    Projectile { speed: f32, radius: f32, range: f32, damage: i32 },
    /// A dash toward the aim over `ticks`, through fighters, cutting each one crossed for
    /// `damage`; stops at walls. With `resets_attack`, landing a hit readies the auto-attack.
    Dash {
        distance: f32,
        ticks: u32,
        damage: i32,
        #[serde(default)]
        resets_attack: bool,
    },
    /// A burst around the user, at once: everyone within `radius` (center to body, not behind a
    /// wall) takes `damage` and `chill`.
    Nova {
        radius: f32,
        damage: i32,
        #[serde(default)]
        chill: Chill,
    },
}

/// How a projectile flies: the auto-attack's or the ability's.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Shot {
    pub speed: f32,
    pub radius: f32,
    pub range: f32,
}

impl Shot {
    /// How many ticks it flies before it's out of range.
    pub fn lifetime_ticks(&self) -> u32 {
        (self.range / self.speed * TICK_HZ as f32).ceil() as u32
    }
}

impl ClassDef {
    /// The projectile this class fires with its auto-attack, or (`ability`) with its Q.
    pub fn shot(&self, ability: bool) -> Option<Shot> {
        if ability { self.ability.kind.shot() } else { self.attack.kind.shot() }
    }

    /// A description from the class file (a passive's or the Q's) with its `{placeholders}`
    /// filled in from this class's numbers, so the words can't drift from them. The list is in
    /// `classes.ron`.
    pub fn describe(&self, text: &str) -> String {
        self.placeholders().into_iter().fold(text.to_string(), |text, (key, value)| text.replace(&format!("{{{key}}}"), &value))
    }

    fn placeholders(&self) -> Vec<(&'static str, String)> {
        let percent = |odds: f32| format!("{:.0}%", odds * 100.0);
        let mut all = vec![
            ("far_scale", percent(self.attack.far_scale())),
            ("attack_slow", percent(self.attack.chill.slow)),
            ("attack_slow_time", seconds(self.attack.chill.slow_ticks)),
            ("crit", percent(self.crit.chance)),
            ("crit_vs_frozen", percent(self.crit.vs_frozen)),
            ("crit_multiplier", percent(CRIT_MULTIPLIER)),
        ];
        if let Some(follow_up) = self.attack.follow_up {
            all.extend([("follow_up_time", seconds(follow_up.within_ticks)), ("follow_up_faster", percent(follow_up.faster))]);
        }
        let reach = match self.ability.kind {
            AbilityKind::Projectile { range, .. } => range,
            AbilityKind::Dash { distance, .. } => distance,
            AbilityKind::Nova { radius, chill, .. } => {
                all.push(("ability_root", seconds(chill.root_ticks)));
                radius
            }
        };
        all.extend([("ability_damage", self.ability.kind.damage().to_string()), ("ability_reach", format!("{reach} m"))]);
        all
    }
}

/// Ticks as players read them: "6 s", "2.5 s".
pub fn seconds(ticks: u32) -> String {
    let seconds = ticks as f32 / TICK_HZ as f32;
    if seconds.fract() == 0.0 { format!("{seconds} s") } else { format!("{seconds:.1} s") }
}

impl AbilityKind {
    /// What it deals to each fighter it hits.
    pub fn damage(&self) -> i32 {
        match *self {
            AbilityKind::Projectile { damage, .. } | AbilityKind::Dash { damage, .. } | AbilityKind::Nova { damage, .. } => damage,
        }
    }

    /// How a thrown ability flies (`None` for a dash or a nova).
    pub fn shot(&self) -> Option<Shot> {
        let AbilityKind::Projectile { speed, radius, range, .. } = *self else { return None };
        Some(Shot { speed, radius, range })
    }
}

impl AttackDef {
    /// The windup of an attack quickened by `follow_up` (the plain windup without one).
    pub fn quick_windup_ticks(&self) -> u32 {
        self.follow_up.map_or(self.windup_ticks, |f| (self.windup_ticks as f32 * (1.0 - f.faster)).round() as u32)
    }

    /// Damage after the full range as a multiple of point blank (1 without `far_scale`).
    pub fn far_scale(&self) -> f32 {
        match self.kind {
            AttackKind::Projectile { far_scale: Some(scale), .. } => scale,
            _ => 1.0,
        }
    }

    /// Damage of a hit after the attack flew `distance`.
    pub fn damage_at(&self, distance: f32) -> i32 {
        let AttackKind::Projectile { range, far_scale: Some(scale), .. } = self.kind else { return self.damage };
        let t = (distance / range).clamp(0.0, 1.0);
        (self.damage as f32 * (1.0 + (scale - 1.0) * t)).round() as i32
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

    /// How far from your center a target's body can reach and still be hit: a thrust reaches a
    /// body whose center is within its reach; a shot's front edge starts at the edge of you and
    /// flies its range. (What the range circle shows.)
    pub fn edge_reach(&self) -> f32 {
        match *self {
            AttackKind::Melee { range, .. } => range,
            AttackKind::Projectile { radius, range, .. } => PLAYER_RADIUS + 2.0 * radius + range,
        }
    }

    /// How a projectile attack flies (`None` for melee).
    pub fn shot(&self) -> Option<Shot> {
        let AttackKind::Projectile { speed, radius, range, .. } = *self else { return None };
        Some(Shot { speed, radius, range })
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
    text_hash(CLASSES_RON)
}

/// FNV-1a over `text`, skipping carriage returns: a CRLF checkout (git's autocrlf on Windows)
/// must agree with an LF one, or builds from the two couldn't connect.
const fn text_hash(text: &str) -> u64 {
    let bytes = text.as_bytes();
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\r' {
            hash = (hash ^ bytes[i] as u64).wrapping_mul(0x0100_0000_01b3);
        }
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
            assert!(c.attack.windup_ticks <= c.attack.cooldown_ticks, "{}: windup longer than the cooldown", c.id);
            match c.attack.kind {
                AttackKind::Melee { range, width } => {
                    assert!((0.5..=4.0).contains(&range) && (0.05..=3.0).contains(&width), "{}: melee", c.id);
                }
                AttackKind::Projectile { speed, radius, range, far_scale } => {
                    assert!(far_scale.is_none_or(|scale| (1.0..=5.0).contains(&scale)), "{}: far_scale", c.id);
                    assert!((2.0..=60.0).contains(&speed) && (0.05..=1.0).contains(&radius), "{}: projectile", c.id);
                    assert!((1.0..=30.0).contains(&range), "{}: projectile range", c.id);
                }
            }
            assert!(c.ability.cooldown_ticks > 0, "{}: ability cooldown", c.id);
            if c.ability.kind.damage() > 0 {
                assert!(c.ability.description.contains("{ability_damage}"), "{}: Q description doesn't name its damage", c.id);
            }
            match c.ability.kind {
                AbilityKind::Projectile { speed, radius, range, damage } => {
                    assert!(damage > 0 && (2.0..=60.0).contains(&speed) && (0.05..=1.0).contains(&radius), "{}: Q", c.id);
                    assert!((1.0..=30.0).contains(&range), "{}: Q range", c.id);
                }
                AbilityKind::Dash { distance, ticks, damage, .. } => {
                    assert!(damage >= 0 && (0.5..=15.0).contains(&distance) && (1..=64).contains(&ticks), "{}: dash", c.id);
                    assert!(ticks < c.ability.cooldown_ticks, "{}: dash longer than its cooldown", c.id);
                }
                AbilityKind::Nova { radius, damage, chill } => {
                    assert!(damage >= 0 && (0.5..=10.0).contains(&radius), "{}: nova", c.id);
                    assert!(damage > 0 || !chill.is_none(), "{}: a nova that does nothing", c.id);
                    chill_is_sane(&c.id, chill);
                }
            }
            chill_is_sane(&c.id, c.attack.chill);
            if let Some(FollowUp { within_ticks, faster }) = c.attack.follow_up {
                // Only a swing knows it landed when it goes off (`server::attack`).
                assert!(matches!(c.attack.kind, AttackKind::Melee { .. }), "{}: follow_up on a projectile", c.id);
                assert!((0.0..1.0).contains(&faster) && within_ticks > 0, "{}: follow_up", c.id);
            }
            if let Some(passive) = &c.passive {
                assert!(!passive.name.is_empty() && !passive.description.is_empty(), "{}: passive", c.id);
            }
            for text in c.passive.iter().map(|p| &p.description).chain([&c.ability.description]) {
                let filled = c.describe(text);
                assert!(!filled.contains(['{', '}']), "{}: unknown placeholder in {filled:?}", c.id);
            }
            let Crit { chance, vs_frozen } = c.crit;
            assert!((0.0..=1.0).contains(&chance) && (0.0..=1.0).contains(&vs_frozen), "{}: crit odds", c.id);
        }
    }

    fn chill_is_sane(id: &str, chill: Chill) {
        assert!((0.0..1.0).contains(&chill.slow), "{id}: slow must leave some speed");
        // Long enough to matter, short enough to fight back: at most 5 s slowed, 3 s rooted.
        assert!(chill.slow_ticks <= 5 * TICK_HZ as u32 && chill.root_ticks <= 3 * TICK_HZ as u32, "{id}: chill too long");
    }

    #[test]
    fn ids_round_trip() {
        for id in ClassId::all() {
            assert_eq!(ClassId::by_key(&id.def().id), Some(id));
        }
        assert_eq!(ClassId(200).checked(), None);
    }

    #[test]
    fn far_scale_scales_with_distance() {
        let kind = |far_scale| AttackKind::Projectile { speed: 10.0, radius: 0.2, range: 10.0, far_scale };
        let attack = |kind| AttackDef { damage: 10, cooldown_ticks: 40, windup_ticks: 10, kind, chill: Chill::default(), follow_up: None };
        assert_eq!([0.0, 5.0, 10.0, 99.0].map(|d| attack(kind(Some(3.0))).damage_at(d)), [10, 20, 30, 30]);
        assert_eq!(attack(kind(None)).damage_at(5.0), 10);
    }

    #[test]
    fn class_file_hash_ignores_line_endings() {
        assert_eq!(text_hash("a: 1,\r\nb: 2,\r\n"), text_hash("a: 1,\nb: 2,\n"));
        assert_ne!(text_hash("a: 1,\n"), text_hash("a: 2,\n"));
    }

    #[test]
    fn projectile_lifetime_covers_its_range() {
        let kind = AttackKind::Projectile { speed: 16.0, radius: 0.2, range: 8.0, far_scale: None };
        assert_eq!(kind.shot().unwrap().lifetime_ticks(), 32);
    }
}
