//! Riven Gorge: highlands split by a chasm running from the south-west to the north-east. No one
//! walks it, but shots fly over: across its narrows the two sides trade throws without ever
//! meeting. A stone arch crosses it in the middle and a rope bridge near each end; and two pairs
//! of waygates jump a fighter from one side's back line to the other's.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, wave};
use crate::protocol::PickupKind;

const SIZE: IVec2 = IVec2::new(80, 54);

/// Meters inside the highland's edge (negative outside).
fn edge(p: Vec2) -> f32 {
    let rho = ((p.x / 35.0) * (p.x / 35.0) + (p.y / 23.0) * (p.y / 23.0)).sqrt();
    let wobble = 0.06 * wave(p.x * 0.09) * wave(p.y * 0.21) + 0.03 * wave(p.x * 0.31) * wave(p.y * 0.13);
    (1.0 + wobble - rho) * 20.0
}

/// The chasm's centerline x for a given y. Odd in y, so it's point-symmetric.
fn chasm_x(y: f32) -> f32 {
    0.45 * y + 4.5 * wave(y * 0.1)
}

/// Half its width, across x: widest in the middle, where it runs most aslant, so it's about as
/// wide across everywhere.
fn chasm_half_width(y: f32) -> f32 {
    2.7 - 0.4 * (y.abs() / 12.0).min(1.0)
}

fn in_chasm(c: Vec2) -> bool {
    (c.x - chasm_x(c.y)).abs() < chasm_half_width(c.y)
}

/// The chasm runs on through the forest.
fn beyond(p: Vec2) -> Tile {
    if in_chasm(p) { Tile::Chasm } else { Tile::Forest }
}

/// The stone arch's half width, and the row of the rope bridge near the north end (mirrored, the
/// south one's): one plank wide, single file.
const ARCH_HALF_WIDTH: f32 = 2.0;
const ROPE_ROW: f32 = 15.5;

const WEST_SPAWNS: &[Vec2] = &[Vec2::new(-31.5, -0.5), Vec2::new(-26.5, -11.5), Vec2::new(-21.5, 13.5), Vec2::new(-16.5, 1.5)];

const WEST_PICKUPS: &[(Vec2, PickupKind)] = &[
    // On the rim, in throwing range of the other side's.
    (Vec2::new(1.5, 7.5), PickupKind::Heal),
    // By the south rope bridge.
    (Vec2::new(-16.5, -13.5), PickupKind::Haste),
];

/// Waygates, each pair its own mirror: from deep on one side to deep on the other.
const GATES: [[Vec2; 2]; 2] = [[Vec2::new(-26.5, 7.5), Vec2::new(26.5, -7.5)], [Vec2::new(-14.5, -17.5), Vec2::new(14.5, 17.5)]];

/// Walls on the west half (each mirrored): (from, to, torch on the `from` end).
const WEST_WALLS: &[(Vec2, Vec2, bool)] = &[
    // A ruined watchtower guarding the north rope bridge: three sides of a square.
    (Vec2::new(-1.5, 18.5), Vec2::new(1.5, 18.5), true),
    (Vec2::new(-1.5, 17.5), Vec2::new(-1.5, 16.5), false),
    (Vec2::new(-1.5, 13.5), Vec2::new(-1.5, 12.5), false),
    // Half a wall covering the west end of the arch.
    (Vec2::new(-7.5, 4.5), Vec2::new(-7.5, 6.5), true),
    (Vec2::new(-8.5, -3.5), Vec2::new(-8.5, -5.5), false),
    // A broken shrine out west.
    (Vec2::new(-24.5, 2.5), Vec2::new(-22.5, 2.5), true),
    (Vec2::new(-24.5, -2.5), Vec2::new(-22.5, -2.5), false),
    // A fallen wall in the south-west field.
    (Vec2::new(-20.5, -10.5), Vec2::new(-17.5, -10.5), true),
    // Lone pillars.
    (Vec2::new(-17.5, 8.5), Vec2::new(-17.5, 8.5), false),
    (Vec2::new(-30.5, -6.5), Vec2::new(-30.5, -6.5), false),
];
/// Boulders: the highland's crags.
const WEST_ROCKS: &[Vec2] = &[
    Vec2::new(-12.5, 4.5),
    Vec2::new(-12.5, 5.5),
    Vec2::new(-13.5, 5.5),
    Vec2::new(-19.5, -5.5),
    Vec2::new(-20.5, -6.5),
    Vec2::new(-8.5, 15.5),
    Vec2::new(-24.5, 11.5),
    Vec2::new(-33.5, 3.5),
    Vec2::new(-4.5, -3.5),
    Vec2::new(-27.5, -3.5),
    Vec2::new(-27.5, -4.5),
    Vec2::new(-14.5, 10.5),
    Vec2::new(-13.5, 11.5),
];
const WEST_TREES: &[Vec2] = &[
    Vec2::new(-21.5, -16.5),
    Vec2::new(-20.5, -17.5),
    Vec2::new(-28.5, 9.5),
    Vec2::new(-11.5, -9.5),
    Vec2::new(-15.5, 17.5),
    Vec2::new(-5.5, 20.5),
    Vec2::new(-23.5, -7.5),
    Vec2::new(-9.5, 9.5),
    Vec2::new(-31.5, 6.5),
];

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Gorge, SIZE, edge, beyond);
    map.paint(|c, tile| {
        if !in_chasm(c) {
            tile
        } else if c.y.abs() < ARCH_HALF_WIDTH {
            Tile::Bridge
        } else if c.y.abs() == ROPE_ROW {
            Tile::Planks
        } else {
            Tile::Chasm
        }
    });
    // Roads from each side's middle to the arch.
    map.paint(|c, tile| if tile == Tile::Grass && c.y.abs() < 1.0 && c.x.abs() > 3.0 && c.x.abs() < 14.0 { Tile::Path } else { tile });
    let mut torches = map.place_half(WEST_WALLS, WEST_ROCKS, WEST_TREES);
    // Torches at the arch's west corners and the north rope bridge's west end.
    for y in [-ARCH_HALF_WIDTH - 0.4, ARCH_HALF_WIDTH + 0.4, ROPE_ROW - 0.9, ROPE_ROW + 0.9] {
        torches.push(Vec2::new(chasm_x(y) - chasm_half_width(y) - 0.4, y));
    }
    map.gates.extend(GATES);
    map.mirror_spots(WEST_SPAWNS, WEST_PICKUPS, &torches);
    // The merchant in the middle, as near the one side as the other.
    map.shop = map.nearest_walkable(Map::tile_of(Vec2::ZERO), 4).map(Map::center);
    map
}
