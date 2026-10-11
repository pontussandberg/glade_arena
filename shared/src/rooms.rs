//! Rooms: what the server browser lists and a room's lobby shows. A room is a private arena a
//! guest creates and leads: the leader picks free-for-all or two teams and starts the match,
//! which then runs (endlessly, for now) until the last member leaves. Anyone can join from the
//! browser, before or during the match.

use core::time::Duration;

use lightyear::prelude::PeerId;
use serde::{Deserialize, Serialize};

use crate::classes::ClassId;

/// Names a room for as long as the server runs.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RoomKey(pub u32);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// Everyone against everyone.
    #[default]
    Ffa,
    /// Two teams; members pick their side.
    Teams,
}

impl Mode {
    /// How many fit in a room of this mode.
    pub fn capacity(self) -> usize {
        match self {
            Mode::Ffa => MAX_FFA,
            Mode::Teams => 2 * MAX_PER_TEAM,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ffa => "Free for all",
            Mode::Teams => "Teams",
        }
    }
}

pub const MAX_FFA: usize = 10;
pub const MAX_PER_TEAM: usize = 10;
/// Longest room name, in characters.
pub const ROOM_NAME_MAX: usize = 24;
/// A member who hasn't touched anything for this long is taken out of the room.
pub const AFK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Teams, as `Team` and `Member::team` carry them. In free-for-all everyone is `NO_TEAM`. Players
/// never see these ids: each sees their own team as allies and the other as enemies.
pub const NO_TEAM: u8 = 0;
pub const TEAM_A: u8 = 1;
pub const TEAM_B: u8 = 2;
pub const TEAMS: [u8; 2] = [TEAM_A, TEAM_B];

/// A line in the server browser.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoomSummary {
    pub key: RoomKey,
    pub name: String,
    pub mode: Mode,
    pub players: u8,
    pub started: bool,
}

impl RoomSummary {
    pub fn full(&self) -> bool {
        self.players as usize >= self.mode.capacity()
    }
}

/// Everything a member sees of their room.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoomView {
    pub key: RoomKey,
    pub name: String,
    pub mode: Mode,
    pub started: bool,
    /// A practice room: ours alone, never listed.
    pub practice: bool,
    /// The leader's `Member::guest_id`.
    pub leader: u32,
    /// In the order they joined.
    pub members: Vec<Member>,
}

impl RoomView {
    pub fn member(&self, guest_id: u32) -> Option<&Member> {
        self.members.iter().find(|m| m.guest_id == guest_id)
    }

    pub fn on_team(&self, team: u8) -> usize {
        self.members.iter().filter(|m| m.team == team).count()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Member {
    pub guest_id: u32,
    pub name: String,
    /// The `PlayerId` their fighter has, once in the arena.
    pub peer: PeerId,
    pub team: u8,
    pub class: Option<ClassId>,
    /// Their fighter is in the arena (else they're in the lobby, picking).
    pub in_arena: bool,
}

/// Why we're no longer in a room.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaveReason {
    /// We asked to.
    Left,
    /// Idle for `AFK_TIMEOUT`.
    Afk,
}

/// A room name as typed, made fit to show: trimmed, printable only, at most `ROOM_NAME_MAX`
/// characters. `None` if nothing is left.
pub fn clean_room_name(name: &str) -> Option<String> {
    let clean: String = name.chars().filter(|c| !c.is_control()).take(ROOM_NAME_MAX).collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_string())
}

/// What guest names are made of (plain ASCII, as the UI's font has it).
const NORSE_FIRST_NAMES: [&str; 80] = [
    "Agnar", "Alfhild", "Arne", "Asa", "Asgeir", "Askel", "Astrid", "Bergljot", "Bjarke", "Bjorn", "Bodil", "Bolli",
    "Brynja", "Dagny", "Egil", "Einar", "Eir", "Eirik", "Erling", "Eyvind", "Finnr", "Freydis", "Frida", "Geir",
    "Gisli", "Gorm", "Grim", "Gudrun", "Gunhild", "Gunnar", "Halfdan", "Hakon", "Hallbjorn", "Harald", "Hedda",
    "Helga", "Hervor", "Hilda", "Hjalmar", "Hrafn", "Ingrid", "Ivar", "Jarl", "Kari", "Ketil", "Knut", "Kolbein",
    "Leif", "Liv", "Magnus", "Njal", "Odd", "Olaf", "Orm", "Ragna", "Ragnar", "Ragnhild", "Rolf", "Runa", "Sigrid",
    "Sigurd", "Sigvald", "Skadi", "Snorri", "Solveig", "Steinar", "Sten", "Svala", "Sven", "Thora", "Thorfinn",
    "Thorgrim", "Thorstein", "Thyra", "Toke", "Torvald", "Ulf", "Valdis", "Vigdis", "Yrsa",
];
const NORSE_BYNAMES: [&str; 80] = [
    "Ironside", "Bloodaxe", "Fairhair", "Forkbeard", "Bluetooth", "the Boneless", "Snakeeye", "Longsword",
    "Shieldbreaker", "Stormborn", "Ravenfeeder", "Wolfsbane", "Frostbeard", "Skullsplitter", "the Red", "the Black",
    "the Bold", "the Grim", "the Tall", "the Stout", "the Unruly", "the Wise", "the Lucky", "Halfhand", "Oneeye",
    "Ironfist", "Stonearm", "Thunderfoot", "Ashwalker", "Mistborn", "Seaborn", "Wavebreaker", "Oakheart",
    "Firebeard", "Goldtooth", "Silvertongue", "Bearclaw", "Elkhorn", "Hammerhand", "Spearshaker", "Helmcleaver",
    "Ringgiver", "Shipburner", "Wormtongue", "Trollslayer", "Giantbane", "Rimeheart", "Hrafnsson", "Bjornsson",
    "Ulfsson", "Ivarsson", "Haraldsson", "Sigurdsson", "Olafsson", "Ketilsson", "Egilsson", "Gunnarsson",
    "Thorsson", "Leifsson", "Ragnarsdottir", "Sigridsdottir", "Ingridsdottir", "Astridsdottir", "Helgasdottir",
    "of the Fjord", "of the Glade", "of the North", "the Wanderer", "the Skald", "the Berserk", "the Shieldmaiden",
    "Crowbeard", "Sootface", "Coldsnap", "Deepdelver", "Beardless", "Gapetooth", "Barefoot", "Squint", "Hairybreeks",
];

/// How many guest names there are without a number.
pub const GUEST_NAMES: u32 = (NORSE_FIRST_NAMES.len() * NORSE_BYNAMES.len()) as u32;

/// Guests get a Norse name, until there are accounts: a first name and a byname ("Bjorn
/// Ironside"), `seed` picking which. The server hands out only names no one connected has; when
/// nearly every pair is taken, a number goes on the end ("Bjorn Ironside 12").
pub fn guest_name(seed: u32) -> String {
    let seed = seed as usize;
    let first = NORSE_FIRST_NAMES[seed % NORSE_FIRST_NAMES.len()];
    let byname = NORSE_BYNAMES[seed / NORSE_FIRST_NAMES.len() % NORSE_BYNAMES.len()];
    format!("{first} {byname}")
}

/// A guest's first name ("Bjorn" of "Bjorn Ironside"), for where the whole name is too long.
pub fn first_name(name: &str) -> &str {
    name.split(' ').next().unwrap_or(name)
}

/// Whether `name` is one `guest_name` gives (with or without a number on the end), so a returning
/// guest may have it again.
pub fn is_guest_name(name: &str) -> bool {
    let Some((first, rest)) = name.split_once(' ') else { return false };
    if !NORSE_FIRST_NAMES.contains(&first) {
        return false;
    }
    // Bynames have spaces of their own ("the Boneless"): any byname, then maybe a number.
    NORSE_BYNAMES.iter().any(|byname| match rest.strip_prefix(byname) {
        Some("") => true,
        Some(number) => number
            .strip_prefix(' ')
            .is_some_and(|n| (1..=4).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit())),
        None => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_names_are_trimmed_and_capped() {
        assert_eq!(clean_room_name("  hi \n"), Some("hi".into()));
        assert_eq!(clean_room_name(" \t "), None);
        assert_eq!(clean_room_name(&"x".repeat(40)).unwrap().len(), ROOM_NAME_MAX);
    }

    #[test]
    fn guest_names() {
        assert_eq!(guest_name(0), "Agnar Ironside");
        assert_eq!(guest_name(GUEST_NAMES - 1), "Yrsa Hairybreeks");
        assert!((0..GUEST_NAMES).all(|seed| is_guest_name(&guest_name(seed))));
        let all: std::collections::HashSet<String> = (0..GUEST_NAMES).map(guest_name).collect();
        assert_eq!(all.len(), GUEST_NAMES as usize, "two seeds give the same name");
        assert!(is_guest_name("Ivar the Boneless 12"));
        assert!(!is_guest_name("Ivar the Boneless 12a"));
        assert!(!is_guest_name("Ivar  the Boneless"));
        assert!(!is_guest_name("Ivar the"));
        assert!(!is_guest_name("Admin"));
    }
}
