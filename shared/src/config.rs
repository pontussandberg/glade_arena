use core::time::Duration;

/// Fixed simulation rate, identical on server and client.
pub const TICK_HZ: u64 = 64;
pub const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_HZ);
/// Seconds per tick, for the sim's per-tick math.
pub const TICK_DT: f32 = 1.0 / TICK_HZ as f32;
/// How often the server sends replication updates.
pub const SEND_INTERVAL: Duration = Duration::from_millis(50);

pub const SERVER_PORT: u16 = 5888;
/// Includes the class file's hash: clients built with different class numbers can't connect.
pub const PROTOCOL_ID: u64 = 0xA4E7_0002 ^ crate::classes::classes_hash();
/// Netcode key shared by server and client. Fine for local dev; real auth will have the
/// account service mint connect tokens instead (see README).
pub const DEV_PRIVATE_KEY: [u8; 32] = [0; 32];

// Gameplay tuning that isn't per class, in world units (1 unit = 1 meter = 1 tile).
// Per-class numbers are in `assets/classes.ron`; the map is in `map.rs`.
/// Body radius used for hits (projectiles and melee reach).
pub const PLAYER_RADIUS: f32 = 0.5;
/// How long a killed player is out of the fight before respawning (3 s).
pub const RESPAWN_TICKS: u32 = 3 * TICK_HZ as u32;
