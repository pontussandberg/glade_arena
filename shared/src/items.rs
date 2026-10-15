//! Items, for the waves mode: bought from the shopkeeper with gold, up to `SLOTS` carried. None
//! gives stats; each does something in a fight (an arc of lightning off a hit, a burst off a
//! kill, wisps circling you), and buying one again raises it a level, up to `MAX_LEVEL`, doing
//! it bigger. A few are for one class only.
//!
//! What each does, and its numbers at each level, are here (`Item::describe` and the tuning
//! functions it reads), so the shop's words can't drift from what the server does with them.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::classes::ClassId;
use crate::config::TICK_HZ;

/// How many items a fighter carries at most.
pub const SLOTS: usize = 6;
/// How far an item can be raised by buying it again.
pub const MAX_LEVEL: u8 = 3;
/// What selling an item gives back, as a share of what was spent on it.
pub const SELL_BACK: f32 = 0.5;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum Item {
    StormCoil,
    EmberHeart,
    RimeSigil,
    Bloodfang,
    BrambleMantle,
    EchoStone,
    PowderKeg,
    WispLantern,
    HoarfrostBoots,
    DoomBell,
    MidasCoin,
    PhoenixFeather,
    ThunderCrown,
    VoidOrb,
    MirrorShard,
    BerserkerTotem,
    RicochetCharm,
    HydraSpear,
    GravecallerRift,
    HeartOfWinter,
}

/// What kind of thing sets an item off, for the shop's grouping and the tooltip's tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    OnHit,
    OnKill,
    OnCast,
    WhenHit,
    Always,
}

impl Trigger {
    pub fn label(self) -> &'static str {
        match self {
            Trigger::OnHit => "ON HIT",
            Trigger::OnKill => "ON KILL",
            Trigger::OnCast => "ON ABILITY",
            Trigger::WhenHit => "WHEN HIT",
            Trigger::Always => "PASSIVE",
        }
    }
}

/// One of three numbers by level (1 to `MAX_LEVEL`).
fn lv<T: Copy>(level: u8, values: [T; 3]) -> T {
    values[(level.clamp(1, MAX_LEVEL) - 1) as usize]
}

/// Seconds to ticks.
pub const fn secs(s: f32) -> u32 {
    (s * TICK_HZ as f32) as u32
}

// Each item's numbers by level. The server does what these say; `describe` says it.

/// Storm Coil: (how many enemies an arc jumps to, its damage per enemy, how far each jump reaches).
pub fn storm_coil(level: u8) -> (u32, i32, f32) {
    (lv(level, [2, 3, 5]), lv(level, [12, 16, 20]), 5.0)
}
/// Ember Heart: (burst damage, radius).
pub fn ember_heart(level: u8) -> (i32, f32) {
    (lv(level, [22, 32, 44]), lv(level, [2.5, 3.0, 3.6]))
}
/// Rime Sigil: (every how many hits, how long the freeze is in ticks).
pub fn rime_sigil(level: u8) -> (u32, u32) {
    (lv(level, [4, 3, 2]), lv(level, [secs(0.8), secs(1.0), secs(1.2)]))
}
/// Bloodfang: health back per hit.
pub fn bloodfang(level: u8) -> i32 {
    lv(level, [3, 5, 8])
}
/// Bramble Mantle: (lash damage, radius, how long until it can lash again in ticks).
pub fn bramble_mantle(level: u8) -> (i32, f32, u32) {
    (lv(level, [14, 22, 32]), 3.0, secs(0.5))
}
/// Echo Stone: (burst damage, radius, delay in ticks).
pub fn echo_stone(level: u8) -> (i32, f32, u32) {
    (lv(level, [26, 40, 58]), lv(level, [3.0, 3.4, 3.8]), secs(0.75))
}
/// Powder Keg: (every how many ticks, blast damage, radius, fuse in ticks, how far it's thrown at most).
pub fn powder_keg(level: u8) -> (u32, i32, f32, u32, f32) {
    (lv(level, [secs(5.0), secs(4.0), secs(3.0)]), lv(level, [30, 40, 52]), 2.6, secs(1.0), 9.0)
}
/// Wisp Lantern: (how many wisps, damage per touch, how far they circle, how long a wisp waits to
/// hit the same enemy again in ticks).
pub fn wisp_lantern(level: u8) -> (u32, i32, f32, u32) {
    (lv(level, [2, 3, 4]), lv(level, [8, 11, 14]), 2.2, secs(0.5))
}
/// How fast wisps circle, in turns per second.
pub const WISP_TURNS: f32 = 0.6;
/// How close a wisp has to come to a body to touch it.
pub const WISP_REACH: f32 = 0.75;

/// Where wisp `wisp` of `count` is, from its holder, at `tick` (fractional, for drawing): evenly
/// spaced on a circle `orbit` out, turning `WISP_TURNS` a second. Only the server decides what
/// they touch; the client draws them from this.
pub fn wisp_offset(tick: f32, wisp: u32, count: u32, orbit: f32) -> Vec2 {
    let angle = tick / TICK_HZ as f32 * WISP_TURNS * std::f32::consts::TAU + wisp as f32 * std::f32::consts::TAU / count.max(1) as f32;
    Vec2::from_angle(angle) * orbit
}
/// Hoarfrost Boots: (slow, damage per pulse, how long a patch lasts in ticks, how often one is left in ticks).
pub fn hoarfrost_boots(level: u8) -> (f32, i32, u32, u32) {
    (lv(level, [0.3, 0.4, 0.5]), lv(level, [4, 6, 9]), secs(3.0), secs(0.4))
}
/// How big a frost patch is.
pub const FROST_PATCH_RADIUS: f32 = 1.3;
/// How often zones (frost patches, blizzards, rifts, vortices) hurt whoever is in them.
pub const ZONE_PULSE_TICKS: u32 = secs(0.5);
/// Doom Bell: (share of the damage dealt during the mark tolled again, how long the mark lasts in ticks).
pub fn doom_bell(level: u8) -> (f32, u32) {
    (lv(level, [0.4, 0.6, 0.85]), secs(3.0))
}
/// Midas Coin: (below this share of its health a hit finishes an enemy, gold it gives).
pub fn midas_coin(level: u8) -> (f32, u32) {
    (lv(level, [0.12, 0.16, 0.22]), lv(level, [3, 4, 6]))
}
/// Phoenix Feather: (share of health risen with, burst damage, radius, recharge in ticks).
pub fn phoenix_feather(level: u8) -> (f32, i32, f32, u32) {
    (lv(level, [0.4, 0.6, 0.8]), lv(level, [30, 50, 80]), 3.5, lv(level, [secs(90.0), secs(75.0), secs(60.0)]))
}
/// Thunder Crown: (every how many hits, strike damage, radius, delay in ticks).
pub fn thunder_crown(level: u8) -> (u32, i32, f32, u32) {
    (lv(level, [5, 4, 3]), lv(level, [36, 50, 68]), 2.2, secs(0.35))
}
/// Void Orb: (radius, pull in meters per second, damage per pulse, how long it lasts in ticks).
pub fn void_orb(level: u8) -> (f32, f32, i32, u32) {
    (lv(level, [3.0, 3.5, 4.0]), lv(level, [3.0, 4.0, 5.0]), lv(level, [5, 8, 12]), secs(2.0))
}
/// Mirror Shard: (how long the decoy stands in ticks, its blast damage, radius, recharge in ticks).
pub fn mirror_shard(level: u8) -> (u32, i32, f32, u32) {
    (secs(4.0), lv(level, [40, 60, 90]), 3.0, lv(level, [secs(12.0), secs(10.0), secs(8.0)]))
}
/// Berserker Totem: (how much faster each kill makes you, the most it stacks to, how long it fades in ticks).
pub fn berserker_totem(level: u8) -> (f32, f32, u32) {
    (lv(level, [0.06, 0.08, 0.1]), lv(level, [0.36, 0.48, 0.6]), secs(4.0))
}
/// Ricochet Charm: (how many times a shot bounces, how far it looks for its next target, share of
/// the damage each bounce keeps).
pub fn ricochet_charm(level: u8) -> (u8, f32, f32) {
    (lv(level, [1, 2, 3]), 7.0, 0.75)
}
/// Hydra Spear: (how many spears a javelin splits into, share of the damage each keeps).
pub fn hydra_spear(level: u8) -> (u32, f32) {
    (lv(level, [3, 4, 5]), lv(level, [0.4, 0.45, 0.5]))
}
/// Gravecaller Rift: damage per pulse to enemies in your rifts.
pub fn gravecaller_rift(level: u8) -> i32 {
    lv(level, [7, 11, 16])
}
/// Heart of Winter: (blizzard radius, slow, damage per pulse, how long it lasts in ticks).
pub fn heart_of_winter(level: u8) -> (f32, f32, i32, u32) {
    (3.6, 0.5, lv(level, [5, 8, 12]), lv(level, [secs(3.0), secs(4.0), secs(5.0)]))
}

impl Item {
    pub const ALL: [Item; 20] = [
        Item::StormCoil,
        Item::EmberHeart,
        Item::RimeSigil,
        Item::Bloodfang,
        Item::BrambleMantle,
        Item::EchoStone,
        Item::PowderKeg,
        Item::WispLantern,
        Item::HoarfrostBoots,
        Item::DoomBell,
        Item::MidasCoin,
        Item::PhoenixFeather,
        Item::ThunderCrown,
        Item::VoidOrb,
        Item::MirrorShard,
        Item::BerserkerTotem,
        Item::RicochetCharm,
        Item::HydraSpear,
        Item::GravecallerRift,
        Item::HeartOfWinter,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Item::StormCoil => "Storm Coil",
            Item::EmberHeart => "Ember Heart",
            Item::RimeSigil => "Rime Sigil",
            Item::Bloodfang => "Bloodfang",
            Item::BrambleMantle => "Bramble Mantle",
            Item::EchoStone => "Echo Stone",
            Item::PowderKeg => "Powder Keg",
            Item::WispLantern => "Wisp Lantern",
            Item::HoarfrostBoots => "Hoarfrost Boots",
            Item::DoomBell => "Doom Bell",
            Item::MidasCoin => "Midas Coin",
            Item::PhoenixFeather => "Phoenix Feather",
            Item::ThunderCrown => "Thunder Crown",
            Item::VoidOrb => "Void Orb",
            Item::MirrorShard => "Mirror Shard",
            Item::BerserkerTotem => "Berserker Totem",
            Item::RicochetCharm => "Ricochet Charm",
            Item::HydraSpear => "Hydra Spear",
            Item::GravecallerRift => "Gravecaller Rift",
            Item::HeartOfWinter => "Heart of Winter",
        }
    }

    /// What it costs at level 1.
    pub fn price(self) -> u32 {
        match self {
            Item::Bloodfang | Item::BrambleMantle => 250,
            Item::StormCoil | Item::RimeSigil | Item::HoarfrostBoots | Item::MidasCoin | Item::BerserkerTotem => 300,
            Item::RicochetCharm | Item::GravecallerRift | Item::HeartOfWinter => 300,
            Item::EmberHeart | Item::EchoStone | Item::DoomBell | Item::MirrorShard | Item::HydraSpear => 350,
            Item::PowderKeg | Item::WispLantern | Item::ThunderCrown | Item::VoidOrb => 400,
            Item::PhoenixFeather => 450,
        }
    }

    /// What raising it from `level` to the next costs (`level` 0: buying it).
    pub fn cost(self, level: u8) -> u32 {
        let factor = match level {
            0 => 1.0,
            1 => 1.2,
            _ => 1.6,
        };
        (self.price() as f32 * factor / 10.0).round() as u32 * 10
    }

    /// The only class (its `ClassDef::id`) that can carry it, if it's one class's.
    pub fn class(self) -> Option<&'static str> {
        match self {
            Item::HydraSpear => Some("javelinist"),
            Item::GravecallerRift => Some("revenant"),
            Item::HeartOfWinter => Some("frost_mage"),
            _ => None,
        }
    }

    /// Whether a fighter of `class` can carry it: one-class items for theirs only, and the
    /// ricochet only for those whose attacks fly.
    pub fn fits(self, class: ClassId) -> bool {
        let def = class.def();
        match self {
            Item::RicochetCharm => def.attack.kind.shot().is_some(),
            _ => self.class().is_none_or(|key| key == def.id),
        }
    }

    pub fn trigger(self) -> Trigger {
        match self {
            Item::StormCoil | Item::RimeSigil | Item::Bloodfang | Item::DoomBell | Item::MidasCoin | Item::ThunderCrown => Trigger::OnHit,
            Item::RicochetCharm | Item::HydraSpear => Trigger::OnHit,
            Item::EmberHeart | Item::BerserkerTotem => Trigger::OnKill,
            Item::EchoStone | Item::VoidOrb | Item::MirrorShard | Item::HeartOfWinter => Trigger::OnCast,
            Item::BrambleMantle | Item::PhoenixFeather => Trigger::WhenHit,
            Item::PowderKeg | Item::WispLantern | Item::HoarfrostBoots | Item::GravecallerRift => Trigger::Always,
        }
    }

    /// One line of flavor.
    pub fn flavor(self) -> &'static str {
        match self {
            Item::StormCoil => "A copper coil humming with a storm that never broke.",
            Item::EmberHeart => "Still warm. Still beating, faintly.",
            Item::RimeSigil => "Frost creeps out of the mark it leaves.",
            Item::Bloodfang => "It drinks, and so do you.",
            Item::BrambleMantle => "Touch it and it touches back.",
            Item::EchoStone => "Everything you do happens twice.",
            Item::PowderKeg => "Lit, thrown, forgotten. Mostly forgotten.",
            Item::WispLantern => "The lights inside want out. Let them.",
            Item::HoarfrostBoots => "Every step leaves winter behind.",
            Item::DoomBell => "It rings for them, a little late.",
            Item::MidasCoin => "The dying are worth their weight in gold.",
            Item::PhoenixFeather => "Death is a setback, not an ending.",
            Item::ThunderCrown => "The sky answers whoever wears it.",
            Item::VoidOrb => "A hole in the world, small and hungry.",
            Item::MirrorShard => "They'll chase the shine. It bites.",
            Item::BerserkerTotem => "Every kill feeds the next.",
            Item::RicochetCharm => "Why stop at one?",
            Item::HydraSpear => "Cut off one point, three more grow.",
            Item::GravecallerRift => "The wounds you open in the earth stay hungry.",
            Item::HeartOfWinter => "Your nova leaves a storm behind.",
        }
    }

    /// What it does at `level`, its numbers filled in.
    pub fn describe(self, level: u8) -> String {
        let s = |ticks: u32| crate::classes::seconds(ticks);
        let pct = |share: f32| format!("{:.0}%", share * 100.0);
        match self {
            Item::StormCoil => {
                let (jumps, damage, _) = storm_coil(level);
                format!("Auto-attack hits arc lightning through {jumps} more enemies, {damage} damage each.")
            }
            Item::EmberHeart => {
                let (damage, radius) = ember_heart(level);
                format!("Enemies you kill burst into flame: {damage} damage to enemies within {radius} m. Bursts can kill, and burst.")
            }
            Item::RimeSigil => {
                let (every, freeze) = rime_sigil(level);
                format!("Every {} auto-attack hit freezes its target for {}.", ordinal(every), s(freeze))
            }
            Item::Bloodfang => format!("Auto-attack hits heal you for {}.", bloodfang(level)),
            Item::BrambleMantle => {
                let (damage, radius, _) = bramble_mantle(level);
                format!("When you're hit, thorns lash every enemy within {radius} m for {damage} damage.")
            }
            Item::EchoStone => {
                let (damage, radius, delay) = echo_stone(level);
                format!("{} after you use your ability, it echoes where you stood: {damage} damage to enemies within {radius} m.", s(delay))
            }
            Item::PowderKeg => {
                let (every, damage, radius, fuse, _) = powder_keg(level);
                format!("Every {}, lob a keg at the nearest enemy. It blows {} later: {damage} damage within {radius} m.", s(every), s(fuse))
            }
            Item::WispLantern => {
                let (count, damage, _, _) = wisp_lantern(level);
                format!("{count} wisps circle you, burning enemies they touch for {damage} damage.")
            }
            Item::HoarfrostBoots => {
                let (slow, damage, lasts, _) = hoarfrost_boots(level);
                format!("Walking leaves frost behind for {}: enemies on it are slowed {} and take {damage} damage every {}.", s(lasts), pct(slow), s(ZONE_PULSE_TICKS))
            }
            Item::DoomBell => {
                let (share, lasts) = doom_bell(level);
                format!("Auto-attacks mark enemies. {} later the bell tolls: they take {} of the damage you dealt them meanwhile, again.", s(lasts), pct(share))
            }
            Item::MidasCoin => {
                let (below, gold) = midas_coin(level);
                format!("Hitting an enemy below {} health turns it to gold: it dies, and everyone gets {gold} more gold. Not bosses.", pct(below))
            }
            Item::PhoenixFeather => {
                let (share, damage, radius, recharge) = phoenix_feather(level);
                format!("A killing blow instead raises you at {} health in a burst of fire ({damage} damage within {radius} m). Recharges in {}.", pct(share), s(recharge))
            }
            Item::ThunderCrown => {
                let (every, damage, radius, _) = thunder_crown(level);
                format!("Every {} auto-attack hit calls down lightning on its target: {damage} damage within {radius} m.", ordinal(every))
            }
            Item::VoidOrb => {
                let (radius, _, damage, lasts) = void_orb(level);
                format!("Using your ability opens a vortex at the cursor for {}, dragging enemies within {radius} m in and dealing {damage} damage every {}.", s(lasts), s(ZONE_PULSE_TICKS))
            }
            Item::MirrorShard => {
                let (lasts, damage, radius, recharge) = mirror_shard(level);
                format!("Using your ability leaves a decoy of you for {}: enemies go for it, and it shatters for {damage} damage within {radius} m. Every {} at most.", s(lasts), s(recharge))
            }
            Item::BerserkerTotem => {
                let (each, most, _) = berserker_totem(level);
                format!("Each kill makes you walk and attack {} faster, stacking up to {}, fading if you stop killing.", pct(each), pct(most))
            }
            Item::RicochetCharm => {
                let (bounces, _, keeps) = ricochet_charm(level);
                let times = if bounces == 1 { "once".to_string() } else { format!("{bounces} times") };
                format!("Your shots that hit bounce on to the nearest enemy, {times}, each bounce keeping {} of the damage.", pct(keeps))
            }
            Item::HydraSpear => {
                let (count, keeps) = hydra_spear(level);
                format!("Javelins that hit split into {count} spears fanning on, each dealing {} of the damage.", pct(keeps))
            }
            Item::GravecallerRift => {
                format!("Enemies in your rifts take {} damage every {}.", gravecaller_rift(level), s(ZONE_PULSE_TICKS))
            }
            Item::HeartOfWinter => {
                let (radius, slow, damage, lasts) = heart_of_winter(level);
                format!("Frost Nova leaves a blizzard for {}: enemies within {radius} m are slowed {} and take {damage} damage every {}.", s(lasts), pct(slow), s(ZONE_PULSE_TICKS))
            }
        }
    }
}

/// "4th", "2nd".
fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// An item carried, how far it's raised, and what was spent on it (for selling it back).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Owned {
    pub item: Item,
    pub level: u8,
    pub spent: u32,
}

/// A fighter's gold and items, in the waves mode. Server-authoritative (kept for its player for
/// the whole run, even out of the arena) and replicated, so the HUD shows ours and allies see
/// each other's.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct Satchel {
    pub gold: u32,
    pub slots: [Option<Owned>; SLOTS],
}

impl Satchel {
    /// The level `item` is carried at (0: not carried).
    pub fn level(&self, item: Item) -> u8 {
        self.slots.iter().flatten().find(|o| o.item == item).map_or(0, |o| o.level)
    }

    /// Every item carried, with its level.
    pub fn items(&self) -> impl Iterator<Item = (Item, u8)> + '_ {
        self.slots.iter().flatten().map(|o| (o.item, o.level))
    }

    /// What buying `item` now costs (raising it, if it's carried), or why it can't be bought by a
    /// fighter of `class`.
    pub fn price_of(&self, item: Item, class: ClassId) -> Result<u32, &'static str> {
        if !item.fits(class) {
            return Err("Not for your class");
        }
        let level = self.level(item);
        if level >= MAX_LEVEL {
            return Err("Already at its highest level");
        }
        if level == 0 && self.slots.iter().all(Option::is_some) {
            return Err("No room: sell something first");
        }
        Ok(item.cost(level))
    }

    /// Buys `item` (or raises it a level) for a fighter of `class`.
    pub fn buy(&mut self, item: Item, class: ClassId) -> Result<(), &'static str> {
        let cost = self.price_of(item, class)?;
        if self.gold < cost {
            return Err("Not enough gold");
        }
        self.gold -= cost;
        if let Some(owned) = self.slots.iter_mut().flatten().find(|o| o.item == item) {
            owned.level += 1;
            owned.spent += cost;
        } else if let Some(free) = self.slots.iter_mut().find(|s| s.is_none()) {
            *free = Some(Owned { item, level: 1, spent: cost });
        }
        Ok(())
    }

    /// What selling the item in `slot` gives back.
    pub fn sell_value(owned: &Owned) -> u32 {
        (owned.spent as f32 * SELL_BACK) as u32
    }

    /// Sells the item in `slot`, if there's one there. What it gave back.
    pub fn sell(&mut self, slot: usize) -> Option<u32> {
        let owned = self.slots.get_mut(slot)?.take()?;
        let back = Satchel::sell_value(&owned);
        self.gold += back;
        Some(back)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_is_described_and_priced() {
        for item in Item::ALL {
            for level in 1..=MAX_LEVEL {
                let words = item.describe(level);
                assert!(!words.is_empty() && !words.contains(['{', '}']), "{item:?}");
            }
            assert!(item.price() > 0 && item.cost(1) >= item.price() && item.cost(2) >= item.cost(1));
            if let Some(key) = item.class() {
                assert!(ClassId::by_key(key).is_some_and(|c| c.def().playable), "{item:?} is for {key}, not a class");
            }
        }
        let mut seen = std::collections::HashSet::new();
        assert!(Item::ALL.iter().all(|item| seen.insert(item.name())), "two items share a name");
    }

    #[test]
    fn buying_raises_and_selling_gives_back_half() {
        let jav = ClassId::by_key("javelinist").unwrap();
        let revenant = ClassId::by_key("revenant").unwrap();
        let mut satchel = Satchel { gold: 10_000, ..default() };
        satchel.buy(Item::StormCoil, jav).unwrap();
        satchel.buy(Item::StormCoil, jav).unwrap();
        assert_eq!(satchel.level(Item::StormCoil), 2);
        satchel.buy(Item::StormCoil, jav).unwrap();
        assert!(satchel.buy(Item::StormCoil, jav).is_err(), "past the highest level");
        assert!(satchel.buy(Item::HydraSpear, revenant).is_err(), "another class's item");
        assert!(satchel.buy(Item::RicochetCharm, revenant).is_err(), "a melee fighter's ricochet");
        let spent = 10_000 - satchel.gold;
        assert_eq!(satchel.sell(0), Some(spent / 2));
        assert_eq!(satchel.level(Item::StormCoil), 0);
        for item in [Item::EmberHeart, Item::RimeSigil, Item::Bloodfang, Item::EchoStone, Item::PowderKeg, Item::VoidOrb] {
            satchel.buy(item, jav).unwrap();
        }
        assert_eq!(satchel.buy(Item::DoomBell, jav), Err("No room: sell something first"));
        satchel.gold = 0;
        assert_eq!(satchel.buy(Item::VoidOrb, jav), Err("Not enough gold"));
    }
}
