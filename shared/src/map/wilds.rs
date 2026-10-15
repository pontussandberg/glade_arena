//! Hollow Wilds: the waves mode's map, far bigger than the others and not mirrored. A walled
//! camp in the middle, where the merchant keeps shop and the party starts; roads out of its four
//! gates to the places around it, each with its own way in and something to find there:
//!
//! - north, a graveyard of crooked stones around a ruined chapel, a blood shrine at its altar
//! - north-east, the hedge maze: old garden walls doubling back on themselves, chests deep in
//! - east, a moonlit lake, a stone bridge out to its island and a ford off the far side
//! - south-east, the quarry: boulders and a burned-out watchtower
//! - south, the rotting marsh and the witch's hut in it
//! - south-west, a circle of standing stones around a shrine
//! - west, the bone chasm, crossed by a stone bridge and a rope one; the bone pit beyond
//! - north-west, the dark grove, trees packed close
//!
//! Monsters come out of lairs (`Map::lairs`) out in these places, never in the camp. Waygates
//! jump from the camp's outskirts to the far corners.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, squircle, wave};
use crate::protocol::PickupKind;

const SIZE: IVec2 = IVec2::new(120, 84);

/// Meters inside the wilds' edge (negative outside): a rounded rectangle, wobbling.
fn edge(p: Vec2) -> f32 {
    let rho = squircle(Vec2::new(p.x / 55.5, p.y / 38.5));
    let wobble = 0.03 * wave(p.x * 0.11) * wave(p.y * 0.17) + 0.02 * wave(p.x * 0.27 + 1.0) * wave(p.y * 0.13 + 0.5);
    (1.0 + wobble - rho) * 22.0
}

/// The chasm runs on out into the forest, north and south.
fn beyond(p: Vec2) -> Tile {
    if in_chasm(p) { Tile::Chasm } else { Tile::Forest }
}

/// The camp's palisade: a square ring of tiles this far out from the middle (tile centers), a gate
/// in the middle of each side.
const PALISADE: f32 = 7.5;

/// The lake: its middle and radii, and the island in it.
const LAKE: (Vec2, Vec2) = (Vec2::new(32.0, 4.0), Vec2::new(11.5, 7.5));
const ISLAND: (Vec2, f32) = (Vec2::new(33.0, 4.0), 3.3);

fn in_lake(c: Vec2) -> bool {
    let d = (c - LAKE.0) / LAKE.1;
    let rim = 1.0 + 0.08 * wave(c.x * 0.5) * wave(c.y * 0.7);
    d.length_squared() < rim * rim
}

/// The chasm's centerline x for a given y, and how far north and south it runs.
fn chasm_x(y: f32) -> f32 {
    -36.0 + 1.6 * wave(y * 0.13)
}
const CHASM_SPAN: (f32, f32) = (-18.0, 16.0);

fn in_chasm(c: Vec2) -> bool {
    (CHASM_SPAN.0..=CHASM_SPAN.1).contains(&c.y) && (c.x - chasm_x(c.y)).abs() < 2.4
}

/// The rope bridge's row and the stone bridge's, over the chasm.
const ROPE_ROW: f32 = 9.5;
const STONE_ROWS: (f32, f32) = (-6.0, -3.0);

/// The marsh: its middle and radii.
const MARSH: (Vec2, Vec2) = (Vec2::new(-6.0, -27.0), Vec2::new(15.0, 8.0));

fn in_marsh(c: Vec2) -> bool {
    let d = (c - MARSH.0) / MARSH.1;
    let rim = 1.0 + 0.15 * wave(c.x * 0.6 + c.y * 0.3) * wave(c.y * 0.5 - c.x * 0.2);
    d.length_squared() < rim * rim
}

/// Deep pools in the marsh, no one wades.
const POOLS: &[Vec2] = &[
    Vec2::new(-14.5, -25.5),
    Vec2::new(-13.5, -25.5),
    Vec2::new(-14.5, -26.5),
    Vec2::new(-1.5, -29.5),
    Vec2::new(-0.5, -29.5),
    Vec2::new(-0.5, -30.5),
    Vec2::new(0.5, -30.5),
    Vec2::new(-10.5, -31.5),
    Vec2::new(-9.5, -31.5),
    Vec2::new(3.5, -24.5),
];

/// The dark grove's middle and radii.
const GROVE: (Vec2, Vec2) = (Vec2::new(-26.0, 25.0), Vec2::new(10.0, 6.5));

/// A tree on every third tile of the grove, along the diagonals, thinned here and there: dense,
/// but never closing anyone in (no tile has blocked ground on more than two of its four sides).
fn grove_tree(t: IVec2, c: Vec2) -> bool {
    let d = (c - GROVE.0) / GROVE.1;
    d.length_squared() < 1.0 && (t.x + t.y).rem_euclid(3) == 0 && wave(c.x * 0.37 + c.y * 0.21) > -0.4
}

/// Walls: (from, to, torch on the `from` end).
const WALLS: &[(Vec2, Vec2, bool)] = &[
    // The chapel, its door to the south, the altar inside.
    (Vec2::new(-4.5, 28.5), Vec2::new(-1.5, 28.5), true),
    (Vec2::new(4.5, 28.5), Vec2::new(1.5, 28.5), true),
    (Vec2::new(-4.5, 28.5), Vec2::new(-4.5, 35.5), false),
    (Vec2::new(4.5, 28.5), Vec2::new(4.5, 35.5), false),
    (Vec2::new(-4.5, 35.5), Vec2::new(4.5, 35.5), false),
    (Vec2::new(-0.5, 33.5), Vec2::new(0.5, 33.5), true),
    // The hedge maze: its outer walls, a way in from the south and one from the west.
    (Vec2::new(20.5, 17.5), Vec2::new(26.5, 17.5), true),
    (Vec2::new(29.5, 17.5), Vec2::new(36.5, 17.5), true),
    (Vec2::new(36.5, 17.5), Vec2::new(36.5, 31.5), false),
    (Vec2::new(20.5, 31.5), Vec2::new(36.5, 31.5), false),
    (Vec2::new(20.5, 17.5), Vec2::new(20.5, 22.5), false),
    (Vec2::new(20.5, 25.5), Vec2::new(20.5, 31.5), false),
    // Inside, walls doubling back: open at the east end, the west, the east.
    (Vec2::new(20.5, 21.5), Vec2::new(32.5, 21.5), false),
    (Vec2::new(24.5, 25.5), Vec2::new(36.5, 25.5), false),
    (Vec2::new(20.5, 28.5), Vec2::new(32.5, 28.5), false),
    // The witch's hut in the marsh, its door to the north.
    (Vec2::new(-9.5, -23.5), Vec2::new(-7.5, -23.5), true),
    (Vec2::new(-5.5, -23.5), Vec2::new(-3.5, -23.5), true),
    (Vec2::new(-9.5, -23.5), Vec2::new(-9.5, -28.5), false),
    (Vec2::new(-3.5, -23.5), Vec2::new(-3.5, -28.5), false),
    (Vec2::new(-9.5, -28.5), Vec2::new(-3.5, -28.5), false),
    // The burned watchtower in the quarry, its door to the west.
    (Vec2::new(35.5, -18.5), Vec2::new(39.5, -18.5), true),
    (Vec2::new(39.5, -18.5), Vec2::new(39.5, -22.5), false),
    (Vec2::new(35.5, -22.5), Vec2::new(39.5, -22.5), true),
    (Vec2::new(35.5, -18.5), Vec2::new(35.5, -19.5), false),
    (Vec2::new(35.5, -21.5), Vec2::new(35.5, -22.5), false),
    // Standing stones, round the shrine in the south-west.
    (Vec2::new(-31.5, -20.5), Vec2::new(-31.5, -20.5), true),
    (Vec2::new(-27.5, -22.5), Vec2::new(-27.5, -22.5), false),
    (Vec2::new(-26.5, -26.5), Vec2::new(-26.5, -26.5), true),
    (Vec2::new(-28.5, -30.5), Vec2::new(-28.5, -30.5), false),
    (Vec2::new(-32.5, -31.5), Vec2::new(-32.5, -31.5), true),
    (Vec2::new(-36.5, -29.5), Vec2::new(-36.5, -29.5), false),
    (Vec2::new(-37.5, -25.5), Vec2::new(-37.5, -25.5), true),
    (Vec2::new(-35.5, -21.5), Vec2::new(-35.5, -21.5), false),
    // Bone pit: broken walls around the far side of the chasm.
    (Vec2::new(-49.5, 5.5), Vec2::new(-45.5, 5.5), true),
    (Vec2::new(-49.5, -4.5), Vec2::new(-45.5, -4.5), true),
    (Vec2::new(-50.5, 4.5), Vec2::new(-50.5, -3.5), false),
    // Lone ruins out on the fields.
    (Vec2::new(14.5, 8.5), Vec2::new(16.5, 8.5), false),
    (Vec2::new(-15.5, -9.5), Vec2::new(-15.5, -11.5), true),
    (Vec2::new(16.5, -9.5), Vec2::new(18.5, -11.5), false),
    (Vec2::new(-18.5, 9.5), Vec2::new(-16.5, 9.5), false),
];

/// The graveyard's stones: rows of them either side of the north road.
fn tombstones() -> Vec<Vec2> {
    let mut stones = Vec::new();
    for row in 0..4 {
        let y = 19.5 + 3.0 * row as f32;
        for col in 0..4 {
            // Crooked: every other row shifted along.
            let shift = (row % 2) as f32;
            stones.push(Vec2::new(-16.5 + 3.0 * col as f32 + shift, y));
            stones.push(Vec2::new(7.5 + 3.0 * col as f32 + shift, y));
        }
    }
    stones
}

const ROCKS: &[Vec2] = &[
    // The quarry.
    Vec2::new(26.5, -20.5),
    Vec2::new(27.5, -20.5),
    Vec2::new(27.5, -21.5),
    Vec2::new(30.5, -25.5),
    Vec2::new(31.5, -25.5),
    Vec2::new(24.5, -27.5),
    Vec2::new(33.5, -29.5),
    Vec2::new(34.5, -29.5),
    Vec2::new(41.5, -25.5),
    Vec2::new(28.5, -31.5),
    Vec2::new(22.5, -23.5),
    Vec2::new(43.5, -14.5),
    // Boulders out on the fields.
    Vec2::new(-12.5, 4.5),
    Vec2::new(11.5, -3.5),
    Vec2::new(-22.5, 14.5),
    Vec2::new(-21.5, -16.5),
    Vec2::new(8.5, 14.5),
    Vec2::new(46.5, 14.5),
    Vec2::new(-44.5, 16.5),
    Vec2::new(-43.5, -14.5),
    Vec2::new(12.5, -22.5),
    Vec2::new(-20.5, 2.5),
];

const TREES: &[Vec2] = &[
    Vec2::new(-10.5, 13.5),
    Vec2::new(10.5, 12.5),
    Vec2::new(-12.5, -14.5),
    Vec2::new(13.5, -15.5),
    Vec2::new(20.5, 12.5),
    Vec2::new(22.5, -8.5),
    Vec2::new(-24.5, -8.5),
    Vec2::new(-25.5, 7.5),
    Vec2::new(44.5, 18.5),
    Vec2::new(18.5, -30.5),
    Vec2::new(-19.5, -34.5),
    Vec2::new(-41.5, 24.5),
    Vec2::new(-47.5, -18.5),
    Vec2::new(48.5, -6.5),
    Vec2::new(5.5, -15.5),
    Vec2::new(-5.5, 16.5),
];

/// Where the party (re)spawns: inside the camp.
const SPAWNS: &[Vec2] = &[
    Vec2::new(-4.5, -4.5),
    Vec2::new(4.5, -4.5),
    Vec2::new(-4.5, 4.5),
    Vec2::new(4.5, 4.5),
    Vec2::new(-4.5, 0.5),
    Vec2::new(4.5, 0.5),
    Vec2::new(0.5, -4.5),
    Vec2::new(0.5, 5.5),
];

const PICKUPS: &[(Vec2, PickupKind)] = &[
    (Vec2::new(-2.5, -1.5), PickupKind::Heal),
    (Vec2::new(19.5, 10.5), PickupKind::Heal),
    (Vec2::new(-6.5, -26.5), PickupKind::Heal),
    (Vec2::new(23.5, 30.5), PickupKind::Heal),
    (Vec2::new(-46.5, 0.5), PickupKind::Heal),
    (Vec2::new(0.5, 16.5), PickupKind::Haste),
    (Vec2::new(-21.5, -4.5), PickupKind::Haste),
    (Vec2::new(17.5, -2.5), PickupKind::Haste),
    (Vec2::new(-1.5, -17.5), PickupKind::Haste),
    (Vec2::new(33.5, 4.5), PickupKind::Shrine),
    (Vec2::new(0.5, 31.5), PickupKind::Shrine),
    (Vec2::new(-31.5, -25.5), PickupKind::Shrine),
];

/// Where monsters come out.
const LAIRS: &[Vec2] = &[
    Vec2::new(-12.5, 33.5),
    Vec2::new(42.5, 22.5),
    Vec2::new(50.5, 8.5),
    Vec2::new(44.5, -18.5),
    Vec2::new(22.5, -33.5),
    Vec2::new(-18.5, -31.5),
    Vec2::new(-47.5, 0.5),
    Vec2::new(-27.5, 26.5),
    Vec2::new(-44.5, -24.5),
    Vec2::new(12.5, 34.5),
];

/// Where chests may turn up, far from the camp.
const CHESTS: &[Vec2] = &[
    Vec2::new(22.5, 30.5),
    Vec2::new(34.5, 19.5),
    Vec2::new(30.5, -28.5),
    Vec2::new(37.5, -20.5),
    Vec2::new(34.5, 2.5),
    Vec2::new(-45.5, -8.5),
    Vec2::new(-24.5, 30.5),
    Vec2::new(-14.5, 23.5),
    Vec2::new(6.5, -32.5),
    Vec2::new(-38.5, -28.5),
    Vec2::new(-44.5, 20.5),
    Vec2::new(50.5, 0.5),
    Vec2::new(16.5, -33.5),
    Vec2::new(-1.5, 34.5),
];

/// Waygates, each pair from the camp's outskirts to a far corner.
const GATES: [[Vec2; 2]; 2] = [[Vec2::new(-12.5, 12.5), Vec2::new(46.5, -10.5)], [Vec2::new(12.5, -12.5), Vec2::new(-46.5, 13.5)]];

/// Roads, two tiles wide: out of each camp gate.
const ROADS: &[(Vec2, Vec2)] = &[
    (Vec2::new(0.0, 8.0), Vec2::new(0.0, 28.0)),
    (Vec2::new(8.0, 0.0), Vec2::new(14.0, 4.0)),
    (Vec2::new(14.0, 4.0), Vec2::new(22.0, 4.0)),
    (Vec2::new(0.0, -8.0), Vec2::new(-6.5, -22.0)),
    (Vec2::new(-8.0, 0.0), Vec2::new(-20.0, -4.5)),
    (Vec2::new(-20.0, -4.5), Vec2::new(-32.0, -4.5)),
    (Vec2::new(4.0, 18.0), Vec2::new(27.5, 16.5)),
    (Vec2::new(8.0, -6.0), Vec2::new(30.0, -18.0)),
    (Vec2::new(-6.0, -12.0), Vec2::new(-30.0, -24.0)),
    (Vec2::new(-6.0, 12.0), Vec2::new(-22.0, 22.0)),
];

/// Distance from `p` to the segment `a`-`b`.
fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Wilds, SIZE, edge, beyond);
    map.paint(|c, tile| {
        let island = c.distance(ISLAND.0) < ISLAND.1;
        let lake = in_lake(c) && !island;
        if lake && (c.y - LAKE.0.y).abs() < 1.1 {
            // The stone bridge out to the island from the west, the ford off its east side.
            if c.x < ISLAND.0.x { Tile::Bridge } else { Tile::Ford }
        } else if lake {
            Tile::Water
        } else if in_chasm(c) && (c.y - ROPE_ROW).abs() < 0.01 {
            Tile::Planks
        } else if in_chasm(c) && c.y > STONE_ROWS.0 && c.y < STONE_ROWS.1 {
            Tile::Bridge
        } else if in_chasm(c) {
            Tile::Chasm
        } else if c.x.abs().max(c.y.abs()) < PALISADE {
            // The camp's beaten ground.
            Tile::Path
        } else if in_marsh(c) {
            Tile::Marsh
        } else if ROADS.iter().any(|&(a, b)| to_segment(c, a, b) < 1.0) {
            Tile::Path
        } else {
            tile
        }
    });
    // The palisade, open in the middle of each side.
    for i in -8..8 {
        let along = i as f32 + 0.5;
        if along.abs() < 2.0 {
            continue;
        }
        for p in [Vec2::new(along, PALISADE), Vec2::new(along, -PALISADE), Vec2::new(PALISADE, along), Vec2::new(-PALISADE, along)] {
            map.set(p, Tile::Wall);
        }
    }
    let mut torches: Vec<Vec2> = [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)].map(|(x, y)| Vec2::new(x, y) * PALISADE).to_vec();
    // By each gate, outside.
    torches.extend([Vec2::new(2.5, 8.6), Vec2::new(-2.5, -8.6), Vec2::new(8.6, -2.5), Vec2::new(-8.6, 2.5)]);
    for &(a, b, torch) in WALLS {
        map.line(a, b, Tile::Wall);
        if torch {
            torches.push(a);
        }
    }
    for p in tombstones().into_iter().chain(ROCKS.iter().copied()) {
        map.set(p, Tile::Rock);
    }
    for &p in TREES {
        map.set(p, Tile::Tree);
    }
    let grove: Vec<IVec2> = map.tiles().filter(|&(t, tile)| tile == Tile::Grass && grove_tree(t, Map::center(t))).map(|(t, _)| t).collect();
    for t in grove {
        map.set(Map::center(t), Tile::Tree);
    }
    for &p in POOLS {
        map.set(p, Tile::Water);
    }
    // Torches at the lake bridge's west end and the chasm bridges' ends.
    torches.extend([Vec2::new(20.6, 2.6), Vec2::new(20.6, 5.4), Vec2::new(-32.6, -6.4), Vec2::new(-32.6, -2.6), Vec2::new(-39.4, -6.4)]);
    map.spawn_points.extend_from_slice(SPAWNS);
    map.pickups.extend_from_slice(PICKUPS);
    map.torches = torches;
    map.gates.extend_from_slice(&GATES);
    // Lairs and chests on walkable ground, wherever the layout left them.
    let walkable_near = |map: &Map, p: Vec2| map.nearest_walkable(Map::tile_of(p), 4).map(Map::center).expect("open ground near a lair or chest");
    map.lairs = LAIRS.iter().map(|&p| walkable_near(&map, p)).collect();
    map.chests = CHESTS.iter().map(|&p| walkable_near(&map, p)).collect();
    map.shop = Some(Vec2::new(0.5, 1.5));
    map.camp = Some((Vec2::ZERO, PALISADE));
    // The merchant's stall: a counter behind them.
    map.line(Vec2::new(-0.5, 2.5), Vec2::new(1.5, 2.5), Tile::Wall);
    map
}
