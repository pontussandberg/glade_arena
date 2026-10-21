//! Rooms: what the server browser lists and a room's lobby shows. A room is a private arena a
//! guest creates and leads: the leader picks free-for-all or two teams and starts the match,
//! which runs until a fighter (or a team) has `KILLS_TO_WIN` kills, zombies roaming the arena
//! meanwhile; then everyone is back in the lobby, to start another. Anyone can join from the
//! browser, before or during the match.

use core::time::Duration;

use lightyear::prelude::PeerId;
use serde::{Deserialize, Serialize};

use crate::classes::ClassId;
use crate::map::MapId;

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
    /// Everyone together against endless waves of monsters, buying items between them
    /// (`RunView`). On its own map (`MapId::Wilds`).
    Waves,
}

impl Mode {
    /// How many fit in a room of this mode.
    pub fn capacity(self) -> usize {
        match self {
            Mode::Ffa => MAX_FFA,
            Mode::Teams => 2 * MAX_PER_TEAM,
            Mode::Waves => MAX_PARTY,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ffa => "Free for all",
            Mode::Teams => "Teams",
            Mode::Waves => "Waves",
        }
    }

    /// One line on what it is.
    pub fn blurb(self) -> &'static str {
        match self {
            Mode::Ffa => "Everyone for themselves",
            Mode::Teams => "Two sides, pick yours",
            Mode::Waves => "Together against endless monsters",
        }
    }

    /// Players against monsters only, never each other.
    pub fn pve(self) -> bool {
        self == Mode::Waves
    }

    pub const PVP: [Mode; 2] = [Mode::Ffa, Mode::Teams];
    pub const PVE: [Mode; 1] = [Mode::Waves];

    /// The maps it may be played on: the waves have theirs.
    pub fn maps(self) -> &'static [MapId] {
        match self {
            Mode::Waves => &[MapId::Wilds, MapId::Bastion],
            _ => &MapId::PVP,
        }
    }

    /// `map` if it may be played on, else the first that may. A staged map counts as its base:
    /// fitting never turns a mid-run bastion into another map.
    pub fn fit_map(self, map: MapId) -> MapId {
        if self.maps().contains(&map.base()) { map } else { self.maps()[0] }
    }
}

pub const MAX_FFA: usize = 10;
pub const MAX_PER_TEAM: usize = 10;
/// The most who fight the waves together.
pub const MAX_PARTY: usize = 5;
/// Runs shown on each leaderboard (one a party size).
pub const LEADERBOARD_ROWS: usize = 5;

/// Longest name a player gives their entry on the leaderboards, in characters.
pub const SCORE_NAME_MAX: usize = 20;

/// A run against the waves that came to an end, for the leaderboards: the furthest wave it
/// reached, where, who fought it (the names they go by there, and their heroes, as the last wave
/// came), when (a UTC date, ISO 8601) and on what version of the game. There's a board for each
/// party size.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HighScore {
    pub wave: u32,
    pub map: MapId,
    pub party: Vec<(String, ClassId)>,
    pub date: String,
    pub version: String,
}

impl HighScore {
    /// Better first: further, then sooner.
    pub fn rank(a: &HighScore, b: &HighScore) -> core::cmp::Ordering {
        b.wave.cmp(&a.wave).then(a.date.cmp(&b.date))
    }
}

/// A name for a leaderboard entry as typed, made fit to show: trimmed, printable only, at most
/// `SCORE_NAME_MAX` characters. `None` if nothing is left.
pub fn clean_score_name(name: &str) -> Option<String> {
    let clean: String = name.chars().filter(|c| !c.is_control() && *c != ';').take(SCORE_NAME_MAX).collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_string())
}

/// What a leaderboard for `size` fighters is called.
pub fn party_label(size: usize) -> String {
    match size {
        1 => "Solo".into(),
        2 => "Duo".into(),
        3 => "Trio".into(),
        n => format!("{n} players"),
    }
}
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
/// The zombies' side: foes to every player, in either mode.
pub const ZOMBIES: u8 = 3;

/// How many kills win a lobby match: a fighter's own in free-for-all, a team's together in teams.
/// Only players count: a zombie's kill, or killing one, scores nothing.
pub const KILLS_TO_WIN: u32 = 5;

/// Who won a match.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Winner {
    /// This member (`Member::guest_id`), in free-for-all.
    Guest(u32),
    /// A team: who was on it as it won (`winners`) and who was on the other (`losers`).
    Team { winners: Vec<u32>, losers: Vec<u32> },
}

/// A line in the server browser.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoomSummary {
    pub key: RoomKey,
    pub name: String,
    pub mode: Mode,
    pub map: MapId,
    pub players: u8,
    /// Its match is on: no way in from the list (`RoomRequest::Join`).
    pub started: bool,
    /// Only those with its password may join.
    pub locked: bool,
}

impl RoomView {
    /// Everyone but the leader is ready (the leader may start).
    pub fn all_ready(&self) -> bool {
        self.members.iter().all(|m| m.ready || m.guest_id == self.leader)
    }
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
    pub map: MapId,
    pub started: bool,
    /// A practice room: ours alone, never listed.
    pub practice: bool,
    /// A run against the waves of our own (`RoomRequest::PlaySolo`): never listed.
    pub solo: bool,
    /// The leader's `Member::guest_id`.
    pub leader: u32,
    /// In the order they joined.
    pub members: Vec<Member>,
    /// Kills that win its match (`KILLS_TO_WIN`); `None` if it never ends (practice).
    pub goal: Option<u32>,
    /// Each team's kills this match (`TEAMS` order), in teams.
    pub team_kills: [u32; 2],
    /// Who won the match: set the moment it's won, and kept, with everyone's kills, until the
    /// next one starts.
    pub winner: Option<Winner>,
    /// The waves mode's run: on, or the last one, until the next starts.
    pub run: Option<RunView>,
}

/// Where a run against the waves is, as its members see it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RunView {
    /// The wave being fought, or the one coming up while shopping (1 to begin with).
    pub wave: u32,
    pub phase: Phase,
    /// Monsters still to come this wave, and alive.
    pub enemies_left: u32,
    /// Who's ready for the next wave (`Member::guest_id`), while shopping.
    pub ready: Vec<u32>,
    /// Who's down (each has one life a wave): until an ally picks them up, or the wave is cleared.
    /// If everyone is, the run is over.
    pub waiting: Vec<u32>,
    /// The furthest wave this room has reached.
    pub best: u32,
    /// The boss of this wave, if it has one (its class's name).
    pub boss: Option<String>,
    /// What's strange about this wave, if anything: known while shopping for it.
    pub omen: Option<Omen>,
}

/// Every how many waves a boss leads one (the waves mode's).
pub const BOSS_EVERY: u32 = 5;

/// What can be strange about a wave, past the first few.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Omen {
    /// Far more of them.
    Horde,
    /// Elites everywhere.
    BloodMoon,
    /// Every monster hasted.
    Frenzy,
    /// Every bounty doubled.
    Gilded,
    /// The dead may rise again.
    Restless,
    /// Everything bursts as it dies.
    Volatile,
}

impl Omen {
    pub const ALL: [Omen; 6] = [Omen::Horde, Omen::BloodMoon, Omen::Frenzy, Omen::Gilded, Omen::Restless, Omen::Volatile];
    /// The order they come in (`Omen::of_wave`): a Gilded wave first, then the rest.
    const IN_TURN: [Omen; 6] = [Omen::Gilded, Omen::Horde, Omen::Frenzy, Omen::Restless, Omen::BloodMoon, Omen::Volatile];

    /// What's strange about `wave`, always the same: the second and fourth wave after each boss
    /// (`BOSS_EVERY`: 7, 9, 12, 14, ...) and the fourth of the run, each in turn (`IN_TURN`).
    pub fn of_wave(wave: u32) -> Option<Omen> {
        let (laps, after) = (wave / BOSS_EVERY, wave % BOSS_EVERY);
        let fourth = match after {
            2 => false,
            4 => true,
            _ => return None,
        };
        // Not wave 2: the first is wave 4, the first in turn.
        let n = (2 * laps + u32::from(fourth)).checked_sub(1)?;
        Some(Self::IN_TURN[n as usize % Self::IN_TURN.len()])
    }

    pub fn label(self) -> &'static str {
        match self {
            Omen::Horde => "Horde",
            Omen::BloodMoon => "Blood Moon",
            Omen::Frenzy => "Frenzy",
            Omen::Gilded => "Gilded",
            Omen::Restless => "Restless Dead",
            Omen::Volatile => "Volatile",
        }
    }

    /// How much more of the wave there is.
    pub fn budget_scale(self) -> f32 {
        if self == Omen::Horde { 1.5 } else { 1.0 }
    }

    /// The odds a monster comes as an elite, against `base`.
    pub fn elite_odds(self, base: f32) -> f32 {
        if self == Omen::BloodMoon { (base * 3.0).max(0.25) } else { base }
    }

    /// How many times its bounty a kill pays.
    pub fn bounty_scale(self) -> u32 {
        if self == Omen::Gilded { 2 } else { 1 }
    }

    /// Whether every monster (bosses aside) comes hasted.
    pub fn hastes(self) -> bool {
        self == Omen::Frenzy
    }

    /// Whether the dead may rise again.
    pub fn raises(self) -> bool {
        self == Omen::Restless
    }

    /// Whether every monster bursts as it dies.
    pub fn bursts(self) -> bool {
        self == Omen::Volatile
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Omen::Horde => "Half as many again come out of the lairs",
            Omen::BloodMoon => "Elites everywhere",
            Omen::Frenzy => "Every monster is hasted",
            Omen::Gilded => "Every bounty is doubled",
            Omen::Restless => "Every third monster to die rises again",
            Omen::Volatile => "Everything bursts as it dies",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Between waves: the next starts in this many seconds (sooner once everyone's ready).
    Shop { secs_left: u32 },
    Fight,
    /// Everyone fell on this wave.
    Over { reached: u32 },
}

impl RoomView {
    pub fn member(&self, guest_id: u32) -> Option<&Member> {
        self.members.iter().find(|m| m.guest_id == guest_id)
    }

    /// Whether this guest may change the room's mode and map: its leader, before the start.
    pub fn may_edit(&self, guest_id: u32) -> bool {
        self.leader == guest_id && !self.started
    }

    pub fn on_team(&self, team: u8) -> usize {
        self.members.iter().filter(|m| m.team == team).count()
    }

    /// A team's kills this match.
    pub fn kills_of(&self, team: u8) -> u32 {
        TEAMS.iter().position(|&t| t == team).map_or(0, |i| self.team_kills[i])
    }

    /// Who won, as players read it: "Bjorn Ironside", or a team as `guest_id` sees it ("Your
    /// team", "The other team"), by the side they were on as it was won. `None` for a team's win
    /// to someone who wasn't in that match.
    pub fn winner_name(&self, guest_id: u32) -> Option<String> {
        match self.winner.as_ref()? {
            Winner::Guest(id) if *id == guest_id => Some("You".into()),
            Winner::Guest(id) => Some(self.member(*id).map_or("Someone who left".into(), |m| m.name.clone())),
            Winner::Team { winners, .. } if winners.contains(&guest_id) => Some("Your team".into()),
            Winner::Team { losers, .. } if losers.contains(&guest_id) => Some("The other team".into()),
            Winner::Team { .. } => None,
        }
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
    /// Players they killed this match (or the last one, until the next starts), and zombies.
    pub kills: u32,
    pub zombie_kills: u32,
    /// Ready to start (in the room's lobby, before the start).
    pub ready: bool,
}

/// Why we're no longer in a room.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaveReason {
    /// We asked to.
    Left,
    /// Idle for `AFK_TIMEOUT`.
    Afk,
}

/// Longest room password, in characters.
pub const PASSWORD_MAX: usize = 24;

/// A room password as typed: trimmed, printable only, at most `PASSWORD_MAX` characters. `None`
/// (no password) if nothing is left.
pub fn clean_password(password: &str) -> Option<String> {
    let clean: String = password.chars().filter(|c| !c.is_control()).take(PASSWORD_MAX).collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_string())
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
