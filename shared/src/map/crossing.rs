//! River Crossing: a forest clearing split by a river that runs north-south through the middle,
//! crossed by a stone bridge and two fords. Ruined walls, boulders and trees give cover.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, wave};
use crate::protocol::PickupKind;

const SIZE: IVec2 = IVec2::new(64, 44);

/// River centerline x for a given y. Odd in y, so the river is point-symmetric.
fn river_x(y: f32) -> f32 {
    1.8 * wave(y * 0.12)
}

const RIVER_HALF_WIDTH: f32 = 1.9;
const BRIDGE_HALF_WIDTH: f32 = 2.0;
const FORD_ROWS: [f32; 2] = [12.5, 13.5];

/// Meters inside the clearing's edge (negative outside).
fn clearing_margin(p: Vec2) -> f32 {
    let rho = ((p.x / 27.0) * (p.x / 27.0) + (p.y / 17.5) * (p.y / 17.5)).sqrt();
    // Even in (x, y): a product of two odd waves, so the edge stays point-symmetric.
    let wobble = 0.07 * wave(p.x * 0.11) * wave(p.y * 0.17) + 0.04 * wave(p.x * 0.23) * wave(p.y * 0.09);
    (1.0 + wobble - rho) * 18.0
}

/// The river runs on through the forest.
fn beyond(p: Vec2) -> Tile {
    if (p.x - river_x(p.y)).abs() < RIVER_HALF_WIDTH { Tile::Water } else { Tile::Forest }
}

/// Spawn points on the west bank; each is mirrored onto the east one.
const WEST_SPAWNS: &[Vec2] = &[Vec2::new(-23.5, -0.5), Vec2::new(-20.5, 7.5), Vec2::new(-20.5, -8.5), Vec2::new(-11.5, -2.5)];

/// A heal and a haste per bank: the heal up north by the river, the haste down south between the
/// spawns and the low wall.
const WEST_PICKUPS: &[(Vec2, PickupKind)] = &[(Vec2::new(-4.5, 10.5), PickupKind::Heal), (Vec2::new(-11.5, -8.5), PickupKind::Haste)];

/// Ruins, rocks and groves on the west bank (x < 0), in tile-center coordinates. Each is
/// mirrored onto the east bank. Walls are (from, to, torch on the `from` end).
const WEST_WALLS: &[(Vec2, Vec2, bool)] = &[
    // Ruined keep corner, with a collapsed gap.
    (Vec2::new(-15.5, 6.5), Vec2::new(-13.5, 6.5), false),
    (Vec2::new(-9.5, 6.5), Vec2::new(-11.5, 6.5), true),
    (Vec2::new(-15.5, 1.5), Vec2::new(-15.5, 5.5), true),
    // Low wall guarding the southern ford.
    (Vec2::new(-7.5, -8.5), Vec2::new(-7.5, -11.5), true),
    // Lone pillars.
    (Vec2::new(-20.5, -3.5), Vec2::new(-20.5, -3.5), false),
    (Vec2::new(-4.5, 5.5), Vec2::new(-4.5, 5.5), false),
];
const WEST_ROCKS: &[Vec2] = &[
    Vec2::new(-22.5, 3.5),
    Vec2::new(-21.5, 3.5),
    Vec2::new(-10.5, -4.5),
    Vec2::new(-4.5, -15.5),
    Vec2::new(-13.5, -12.5),
];
const WEST_TREES: &[Vec2] = &[
    Vec2::new(-18.5, -10.5),
    Vec2::new(-17.5, -11.5),
    Vec2::new(-24.5, -4.5),
    Vec2::new(-8.5, 11.5),
    Vec2::new(-3.5, 14.5),
    Vec2::new(-12.5, 12.5),
];

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Crossing, SIZE, clearing_margin, beyond);
    map.paint(|c, tile| {
        let in_river = (c.x - river_x(c.y)).abs() < RIVER_HALF_WIDTH;
        if in_river && c.y.abs() < BRIDGE_HALF_WIDTH {
            Tile::Bridge
        } else if in_river && FORD_ROWS.iter().any(|r| (c.y.abs() - r).abs() < 0.01) {
            Tile::Ford
        } else if in_river {
            Tile::Water
        } else if c.y.abs() < 1.0 {
            Tile::Path
        } else {
            tile
        }
    });
    let mut torches = map.place_half(WEST_WALLS, WEST_ROCKS, WEST_TREES);
    // Torches at the bridge's two west corners (mirrored onto the east ones).
    for y in [-BRIDGE_HALF_WIDTH - 0.4, BRIDGE_HALF_WIDTH + 0.4] {
        torches.push(Vec2::new(river_x(y) - (RIVER_HALF_WIDTH + 0.3), y));
    }
    map.mirror_spots(WEST_SPAWNS, WEST_PICKUPS, &torches);
    map
}
