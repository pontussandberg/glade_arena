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
    /// Red against blue; members pick their side.
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
            Mode::Teams => "Red vs Blue",
        }
    }
}

pub const MAX_FFA: usize = 10;
pub const MAX_PER_TEAM: usize = 10;
/// Longest room name, in characters.
pub const ROOM_NAME_MAX: usize = 24;
/// A member who hasn't touched anything for this long is taken out of the room.
pub const AFK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Teams, as `Team` and `Member::team` carry them. In free-for-all everyone is `NO_TEAM`.
pub const NO_TEAM: u8 = 0;
pub const RED: u8 = 1;
pub const BLUE: u8 = 2;
pub const TEAMS: [u8; 2] = [RED, BLUE];

pub fn team_name(team: u8) -> &'static str {
    match team {
        RED => "Red",
        BLUE => "Blue",
        _ => "No team",
    }
}

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

/// Guests are called `Guest-` and four digits, until there are accounts.
pub fn guest_name(number: u16) -> String {
    format!("Guest-{:04}", number % 10_000)
}

pub fn is_guest_name(name: &str) -> bool {
    name.strip_prefix("Guest-").is_some_and(|n| n.len() == 4 && n.bytes().all(|b| b.is_ascii_digit()))
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
        assert!(is_guest_name(&guest_name(7)));
        assert_eq!(guest_name(7), "Guest-0007");
        assert!(!is_guest_name("Guest-12a4"));
        assert!(!is_guest_name("Admin"));
    }
}
