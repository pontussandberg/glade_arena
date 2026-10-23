//! How each kind of ability looks to its user: the aim indicator a normal cast shows on the
//! ground (`Indicator`, `casting.rs`), and its picture on the action bar (`icon`,
//! `action_bar.rs`). One module per kind, like the shared `abilities`.

mod dash;
mod nova;
mod prime;
mod strike;
mod throw;

use arena_shared::abilities::{AbilityKind, MAX_ABILITIES};
use arena_shared::protocol::{AbilitySlot, Cooldowns};
use bevy::prelude::*;

/// Where an ability will go, lying flat on the ground at its user's feet while a normal cast
/// waits for its click.
pub(crate) struct Indicator {
    /// Pointing along world +X.
    pub mesh: Mesh,
    /// Whether it turns toward the cursor (a nova's is all around).
    pub aimed: bool,
}

pub(crate) fn indicator(kind: &AbilityKind) -> Indicator {
    match kind {
        AbilityKind::Projectile(throw) => throw::indicator(throw),
        AbilityKind::Dash(dash) => dash::indicator(dash),
        AbilityKind::Nova(nova) => nova::indicator(nova),
        AbilityKind::Prime(_) => prime::indicator(),
        AbilityKind::Strike(strike) => strike::indicator(strike),
    }
}

/// How far along +X the indicator is stretched, aimed `aim` (from its user to the cursor): a
/// dash's reaches as far as the dash would go; the rest keep their size.
pub(crate) fn indicator_length(kind: &AbilityKind, aim: Vec2) -> f32 {
    match kind {
        AbilityKind::Dash(dash) => dash::length(dash, aim),
        AbilityKind::Projectile(_) | AbilityKind::Nova(_) | AbilityKind::Prime(_) | AbilityKind::Strike(_) => 1.0,
    }
}

/// The ability's picture, filling its icon (`action_bar`).
pub(crate) fn icon(commands: &mut Commands, kind: &AbilityKind) -> Entity {
    match kind {
        AbilityKind::Projectile(_) => throw::icon(commands),
        AbilityKind::Dash(_) => dash::icon(commands),
        AbilityKind::Nova(_) => nova::icon(commands),
        AbilityKind::Prime(_) => prime::icon(commands),
        AbilityKind::Strike(_) => strike::icon(commands),
    }
}

/// When each of a fighter's abilities was last used, as last seen (`None` before it was first
/// seen): to tell a cast that just happened from one that had already, when it was first seen.
#[derive(Component, Default)]
pub(crate) struct SeenCasts(Option<[Option<u32>; MAX_ABILITIES]>);

impl SeenCasts {
    /// Starting from what `cooldowns` show as the fighter is first seen: those casts are old
    /// news, and the next one counts, even if it's the fighter's first (a fighter that casts
    /// before anything else changes its cooldowns would otherwise have that cast swallowed as
    /// the baseline).
    pub(crate) fn seen(cooldowns: Option<&Cooldowns>) -> SeenCasts {
        SeenCasts(cooldowns.map(|cooldowns| std::array::from_fn(|i| cooldowns.used_at(AbilitySlot(i as u8)))))
    }

    /// The abilities used since this was last asked, going by `cooldowns`. None the first time,
    /// if it started from nothing: those were used before we saw the fighter.
    pub(crate) fn new_casts(&mut self, cooldowns: &Cooldowns) -> impl Iterator<Item = AbilitySlot> + use<> {
        let now: [Option<u32>; MAX_ABILITIES] = std::array::from_fn(|i| cooldowns.used_at(AbilitySlot(i as u8)));
        let before = self.0.replace(now);
        (0..MAX_ABILITIES)
            .filter(move |&i| before.is_some_and(|before| now[i].is_some() && before[i] != now[i]))
            .map(|i| AbilitySlot(i as u8))
    }
}
