//! Abilities: up to `MAX_ABILITIES` per class, on Q, W, E and R (`AbilitySlot`), each of one of
//! a few kinds. Each kind is a module of its own, holding its numbers and everything it does:
//! `throw` (a projectile), `dash`, `nova` and `prime`. What's the same for every kind is here: the
//! definition (`AbilityDef`), cooldowns (`Cooldowns`), and using one (`step`).

pub mod dash;
pub mod nova;
pub mod prime;
pub mod throw;

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use serde::{Deserialize, Serialize};

use crate::classes::{Chill, ClassId, Shot, common, seconds};
use crate::protocol::{Chilled, PlayerInput, Projectile};
use dash::{DashDef, Dashing};
use nova::NovaDef;
use prime::PrimeDef;
use throw::Throw;

/// The most abilities a class can have: one per key (Q, W, E, R; the client binds them). Checked
/// as the class files load.
pub const MAX_ABILITIES: usize = 4;

/// Which of a class's abilities: its place in the class file's `abilities`, and so its key.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Reflect)]
pub struct AbilitySlot(pub u8);

impl AbilitySlot {
    fn index(self) -> usize {
        self.0 as usize
    }
}

/// An ability's key pressed: use the ability in `slot` toward `aim` (`PlayerInput::casts`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct CastOrder {
    pub slot: AbilitySlot,
    pub aim: Vec2,
}

/// An ability, from the class file.
#[derive(Deserialize, Debug, Clone)]
pub struct AbilityDef {
    pub name: String,
    pub description: String,
    pub cooldown_ticks: u32,
    pub kind: AbilityKind,
    /// Whether it starts and waits for the global cooldown (`Common::global_cooldown_ticks`):
    /// yes unless it opts out.
    #[serde(default = "yes")]
    pub global_cooldown: bool,
}

fn yes() -> bool {
    true
}

/// What an ability does. Each kind's numbers and rules are in its own module.
#[derive(Deserialize, Debug, Clone, Copy)]
pub enum AbilityKind {
    /// Thrown instantly toward the aim (`throw`).
    Projectile(Throw),
    /// A dash toward the aim, through fighters (`dash`).
    Dash(DashDef),
    /// A burst around the user, at once (`nova`).
    Nova(NovaDef),
    /// The user's next auto-attack readied: sure to crit, and quicker (`prime`).
    Prime(PrimeDef),
}

/// What an ability's hit does to whoever it hits. Abilities never crit (a prime makes an
/// auto-attack crit instead).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Effect {
    pub damage: i32,
    pub chill: Chill,
}

impl AbilityKind {
    /// What it does to each fighter it hits.
    pub fn effect(&self) -> Effect {
        match *self {
            AbilityKind::Projectile(throw) => Effect { damage: throw.damage, chill: Chill::default() },
            AbilityKind::Dash(dash) => Effect { damage: dash.damage, chill: Chill::default() },
            AbilityKind::Nova(nova) => Effect { damage: nova.damage, chill: nova.chill },
            AbilityKind::Prime(_) => Effect::default(),
        }
    }

    /// Whether it's used toward an aim (a prime isn't: it goes off at once, whatever the aim).
    pub fn aimed(&self) -> bool {
        !matches!(self, AbilityKind::Prime(_))
    }

    /// How far it reaches: a throw's range, a dash's distance, a nova's radius (a prime, none).
    pub fn reach(&self) -> f32 {
        match *self {
            AbilityKind::Projectile(throw) => throw.range,
            AbilityKind::Dash(dash) => dash.distance,
            AbilityKind::Nova(nova) => nova.radius,
            AbilityKind::Prime(_) => 0.0,
        }
    }

    /// How a thrown ability flies (`None` for anything else).
    pub fn shot(&self) -> Option<Shot> {
        let AbilityKind::Projectile(throw) = self else { return None };
        Some(throw.shot())
    }

    /// The numbers its description can name (`ClassDef::describe_ability`).
    pub fn placeholders(&self) -> Vec<(&'static str, String)> {
        let effect = self.effect();
        let mut all = vec![("ability_damage", effect.damage.to_string()), ("ability_reach", format!("{} m", self.reach()))];
        if effect.chill.root_ticks > 0 {
            all.push(("ability_root", seconds(effect.chill.root_ticks)));
        }
        if let AbilityKind::Prime(prime) = self {
            all.push(("ability_attack_speed", format!("{:.0}%", prime.attack_speed * 100.0)));
        }
        all
    }
}

/// When each ability may be used again, and when it last was. Server-authoritative, replicated
/// and shown like `AttackState`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct Cooldowns {
    pub slots: [SlotCooldown; MAX_ABILITIES],
    /// When the global cooldown is over: shared by every ability on it.
    pub global_ready_at: u32,
    /// No cooldowns of their own (practice's tool; the server sets it). The global cooldown
    /// still runs.
    pub free: bool,
}

/// One ability's cooldown.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct SlotCooldown {
    /// When its own cooldown is over.
    pub ready_at: u32,
    /// When it was last used (`None`: never).
    pub used_at: Option<u32>,
}

impl Cooldowns {
    fn slot(&self, slot: AbilitySlot) -> SlotCooldown {
        self.slots.get(slot.index()).copied().unwrap_or_default()
    }

    /// When the ability's own cooldown is over (at once, with none).
    pub fn cooldown_over_at(&self, slot: AbilitySlot) -> u32 {
        if self.free { 0 } else { self.slot(slot).ready_at }
    }

    /// When the global cooldown is over for `ability` (at once, if it's not on it).
    pub fn global_over_at(&self, ability: &AbilityDef) -> u32 {
        if ability.global_cooldown { self.global_ready_at } else { 0 }
    }

    /// When the ability in `slot` may be used next: both cooldowns over.
    pub fn usable_at(&self, slot: AbilitySlot, ability: &AbilityDef) -> u32 {
        self.cooldown_over_at(slot).max(self.global_over_at(ability))
    }

    /// When the ability in `slot` was last used (`None`: never).
    pub fn used_at(&self, slot: AbilitySlot) -> Option<u32> {
        self.slot(slot).used_at
    }

    /// The ability in `slot` used at `tick`: its cooldown starts, and the global one if it's on
    /// it.
    fn start(&mut self, slot: AbilitySlot, ability: &AbilityDef, tick: u32) {
        let Some(cooldown) = self.slots.get_mut(slot.index()) else { return };
        *cooldown = SlotCooldown { ready_at: if self.free { tick } else { tick + ability.cooldown_ticks }, used_at: Some(tick) };
        if ability.global_cooldown {
            self.global_ready_at = tick + common().global_cooldown_ticks;
        }
    }
}

/// What using an ability did the tick it was used.
#[derive(Debug)]
pub enum Cast {
    /// Spawn this projectile.
    Throw(Projectile),
    /// A dash started (now in `Dashing`).
    Dash,
    /// `ability`, a nova, bursts around the caster; the server resolves who it hits
    /// (`NovaDef::hits`).
    Nova { ability: &'static AbilityDef, nova: NovaDef },
    /// The next auto-attack readied (`AttackState::primed`).
    Prime(PrimeDef),
}

/// One tick of a fighter's abilities. A dash that has gone its distance ends, and a root stops
/// one in progress. Each ability's key pressed since the last tick (`PlayerInput::casts`, in the
/// order they were pressed) uses it toward its aim, if it's ready: a throw and a nova go off at
/// once, and so does a prime (whatever the aim); a dash starts (mid-windup too, the attack still going off when it's over; not while
/// rooted) and runs until it has covered its distance, moved by `sim::move_player`. One used
/// starts the global cooldown before the next is tried.
#[allow(clippy::too_many_arguments)]
pub fn step(
    tick: u32,
    owner: PeerId,
    class: ClassId,
    pos: Vec2,
    input: &PlayerInput,
    chilled: &Chilled,
    cooldowns: &mut Cooldowns,
    dashing: &mut Dashing,
) -> Vec<Cast> {
    let rooted = chilled.rooted_at(tick);
    if dashing.0.is_some_and(|dash| dash.over()) || rooted {
        dashing.0 = None;
    }
    let mut casts = Vec::new();
    for CastOrder { slot, aim } in input.casts() {
        let Some(ability) = class.def().ability(slot) else { continue };
        if tick < cooldowns.usable_at(slot, ability) {
            continue;
        }
        // Every kind but a prime goes toward its aim, and needs one.
        let dir = aim.try_normalize();
        casts.push(match (ability.kind, dir) {
            (AbilityKind::Prime(prime), _) => Cast::Prime(prime),
            (_, None) => continue,
            (AbilityKind::Projectile(throw), Some(dir)) => Cast::Throw(throw.projectile(owner, class, slot, pos, dir, tick)),
            // Mid-windup too: the windup goes on, and the attack still goes off when it's over,
            // from wherever the dash took you (still aimed where it was locked).
            (AbilityKind::Dash(def), Some(dir)) if !rooted => {
                dashing.0 = Some(def.start(slot, dir, aim, tick));
                Cast::Dash
            }
            (AbilityKind::Dash(_), _) => continue,
            (AbilityKind::Nova(nova), _) => Cast::Nova { ability, nova },
        });
        cooldowns.start(slot, ability, tick);
    }
    casts
}
