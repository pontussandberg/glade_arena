use core::time::Duration;

/// Fixed simulation rate, identical on server and client.
pub const TICK_HZ: u64 = 32;
pub const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_HZ);
/// Seconds per tick, for the sim's per-tick math.
pub const TICK_DT: f32 = 1.0 / TICK_HZ as f32;
/// How often the server sends replication updates: every tick, so a command's result comes back
/// as soon as it's there (nothing is predicted).
pub const SEND_INTERVAL: Duration = TICK_DURATION;

pub const SERVER_PORT: u16 = 5888;
/// Includes the class file's hash: clients built with different class numbers can't connect.
/// The game's version (the workspace's): kept with every run on the leaderboards.
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const PROTOCOL_ID: u64 = 0xA4E7_0009 ^ crate::classes::classes_hash();
/// Netcode key shared by server and client. Fine for local dev; real auth will have the
/// account service mint connect tokens instead (see README).
pub const DEV_PRIVATE_KEY: [u8; 32] = [0; 32];

// Gameplay tuning that isn't per class, in world units (1 unit = 1 meter = 1 tile).
// Per-class numbers (and what every class shares) are in `assets/classes/`; the map is in
// `map.rs`.
/// Further than anything moves in a tick (a hasted dash goes about 1.8 m): a fighter that moved
/// further was put there (spawned, respawned, through a waygate), and is drawn there at once,
/// never sliding over.
pub const JUMP: f32 = 2.5;
/// Body radius used for hits (projectiles and melee reach).
pub const PLAYER_RADIUS: f32 = 0.5;
/// How tall a body is to a shot (meters): one flying higher passes over it.
pub const BODY_HEIGHT: f32 = 2.1;
/// How high a shot leaves the hand (meters): where its climb or fall starts from.
pub const SHOT_HEIGHT: f32 = 1.0;
/// How fast a thrown shot falls (m/s², the real thing), one that falls at all (`Shot::falls`).
pub const GRAVITY: f32 = 9.81;
/// A jump (`protocol::Jump`): how high it goes at the top (meters), and how long it is in the
/// air (ticks), up and down.
pub const JUMP_HEIGHT: f32 = 1.0;
pub const JUMP_TICKS: u32 = 28;
/// How long a killed player is out of the fight before respawning (3 s).
pub const RESPAWN_TICKS: u32 = 3 * TICK_HZ as u32;

// Pickups lying in the arena (spots in `map.rs`), used up the moment a fighter touches one.
/// How long a taken pickup is gone before it's back (15 s), a heal longer (20 s).
pub const PICKUP_RESPAWN_TICKS: u32 = 15 * TICK_HZ as u32;
pub const HEAL_RESPAWN_TICKS: u32 = 20 * TICK_HZ as u32;
/// How close (center to center) a fighter has to come to take one.
pub const PICKUP_RADIUS: f32 = 0.9;
/// A heal restores this share of the fighter's max health.
pub const HEAL_FRACTION: f32 = 0.15;
/// A haste makes its taker walk this many times as fast, for this long (2 s).
pub const HASTE_FACTOR: f32 = 1.5;
pub const HASTE_TICKS: u32 = 2 * TICK_HZ as u32;

// Frenzy (`protocol::Frenzy`): killing a zombie makes its killer walk and attack faster for a
// while, more with every zombie it has killed this match.
/// The first kill's frenzy (a share of speed: 0.1 = 10%), and how much stronger each kill makes
/// the next one's.
pub const FRENZY_BASE: f32 = 0.10;
pub const FRENZY_PER_KILL: f32 = 0.02;
/// How long it takes to fade, for a fighter attacking from range and one in melee.
pub const FRENZY_RANGED_TICKS: u32 = 5 * TICK_HZ as u32 / 2;
pub const FRENZY_MELEE_TICKS: u32 = 5 * TICK_HZ as u32;

// Waygates (in `map.rs`): stepping onto one end puts a fighter on the other.
/// How close (center to center) a fighter has to come to an end to go through.
pub const GATE_RADIUS: f32 = 0.6;
/// How long both ends stay closed after someone goes through (8 s).
pub const GATE_CLOSED_TICKS: u32 = 8 * TICK_HZ as u32;

// Fury stones and chests (`PickupKind`), in the waves mode's wilds.
/// A fury stone's rush (`Fury`): how much faster its taker attacks, fading over this long.
pub const FURY_HASTE: f32 = 0.35;
pub const FURY_TICKS: u32 = 8 * TICK_HZ as u32;
/// How long a used fury stone takes to come back.
pub const FURY_RESPAWN_TICKS: u32 = 45 * TICK_HZ as u32;
