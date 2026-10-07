use core::time::Duration;

/// Fixed simulation rate, identical on server and client.
pub const TICK_HZ: u64 = 64;
pub const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_HZ);
/// Seconds per tick, for the sim's per-tick math.
pub const TICK_DT: f32 = 1.0 / TICK_HZ as f32;
/// How often the server sends replication updates.
pub const SEND_INTERVAL: Duration = Duration::from_millis(50);

pub const SERVER_PORT: u16 = 5888;
pub const PROTOCOL_ID: u64 = 0xA4E7_0001;
/// Netcode key shared by server and client. Fine for local dev; real auth will have the
/// account service mint connect tokens instead (see README).
pub const DEV_PRIVATE_KEY: [u8; 32] = [0; 32];

// Gameplay tuning, in world units (1 unit = 1 meter = 1 tile). The map is in `map.rs`.
pub const PLAYER_RADIUS: f32 = 0.5;
pub const PLAYER_SPEED: f32 = 6.0;
pub const MAX_HEALTH: i32 = 100;

pub const PROJECTILE_RADIUS: f32 = 0.2;
pub const PROJECTILE_SPEED: f32 = 18.0;
pub const PROJECTILE_DAMAGE: i32 = 20;
pub const PROJECTILE_LIFETIME_TICKS: u32 = 80;
pub const FIRE_COOLDOWN_TICKS: u32 = 24;
