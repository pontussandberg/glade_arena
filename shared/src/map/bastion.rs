//! Sunken Bastion: the waves mode's second map, a drowned fortress that opens as the run
//! deepens. The party starts walled into the heart of it — the camp and the south yard, graves
//! and rubble under the curtain wall — and every fourth wave a sealed court unlocks
//! (`MapId::at_stage`), its gate torn open between waves:
//!
//! - wave 5, the east court: the flooded cistern, black pools crossed on planks and a ford
//! - wave 9, the west court: the broken armory, a chasm through its floor, a forge in ruins
//! - wave 13, the north court: the sanctum, standing stones around an altar
//!
//! Each court brings its own lairs, chests and pickups, so the fight spreads as it grows. A
//! sealed court stands choked with black thicket; its stage cuts it clear and tears its gate
//! open. A stage only ever opens ground, so no one is ever shut in by one.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, squircle, wave};
use crate::protocol::PickupKind;

const SIZE: IVec2 = IVec2::new(72, 72);

/// Meters inside the bastion's edge (negative outside): a rounded square, wobbling.
fn edge(p: Vec2) -> f32 {
    let rho = squircle(Vec2::new(p.x / 32.5, p.y / 32.5));
    let wobble = 0.03 * wave(p.x * 0.13) * wave(p.y * 0.19);
    (1.0 + wobble - rho) * 18.0
}

fn beyond(_p: Vec2) -> Tile {
    Tile::Forest
}

/// The curtain wall's ring, the cross walls that seal the courts, and the camp's own bailey.
const CURTAIN: f32 = 29.5;
const COURT: f32 = 11.5;
const BAILEY: f32 = 6.5;

/// The cistern's pools (middle, radii) in the east court, and the old well-pond among the
/// yard's graves (there from the start).
const POOLS: [(Vec2, Vec2); 3] =
    [(Vec2::new(21.0, 4.0), Vec2::new(6.0, 5.5)), (Vec2::new(24.0, -11.0), Vec2::new(4.0, 3.5)), (Vec2::new(-3.0, -26.0), Vec2::new(1.5, 1.3))];

fn in_pool(c: Vec2) -> bool {
    POOLS.iter().any(|&(at, radii)| {
        let d = (c - at) / radii;
        let rim = 1.0 + 0.1 * wave(c.x * 0.6) * wave(c.y * 0.5);
        d.length_squared() < rim * rim
    })
}

/// The chasm through the armory's floor in the west court.
const CHASM: (Vec2, Vec2) = (Vec2::new(-21.0, 2.0), Vec2::new(5.0, 5.0));

fn in_chasm(c: Vec2) -> bool {
    let d = (c - CHASM.0) / CHASM.1;
    let rim = 1.0 + 0.08 * wave(c.x * 0.5 + 1.0) * wave(c.y * 0.7);
    d.length_squared() < rim * rim
}

/// Each sealed court's gate: the tiles carved open (wall to path) once the run reaches its
/// stage, and the torches lit beside it then.
struct Gate {
    stage: u8,
    tiles: [Vec2; 3],
    torches: [Vec2; 2],
}

const GATES: [Gate; 3] = [
    // East, the cistern.
    Gate { stage: 1, tiles: [Vec2::new(11.5, -1.5), Vec2::new(11.5, -0.5), Vec2::new(11.5, 0.5)], torches: [Vec2::new(10.4, -2.5), Vec2::new(10.4, 1.5)] },
    // West, the armory.
    Gate { stage: 2, tiles: [Vec2::new(-11.5, -1.5), Vec2::new(-11.5, -0.5), Vec2::new(-11.5, 0.5)], torches: [Vec2::new(-10.4, -2.5), Vec2::new(-10.4, 1.5)] },
    // North, the sanctum.
    Gate { stage: 3, tiles: [Vec2::new(-1.5, 11.5), Vec2::new(-0.5, 11.5), Vec2::new(0.5, 11.5)], torches: [Vec2::new(-2.5, 10.4), Vec2::new(1.5, 10.4)] },
];

/// Where each gate's court lies, to keep what belongs to a stage out of the map before it: the
/// east court, the west, the north.
fn court_stage(c: Vec2) -> u8 {
    match () {
        _ if c.x > COURT => 1,
        _ if c.x < -COURT => 2,
        _ if c.y > COURT => 3,
        _ => 0,
    }
}

/// Ruin walls: (from, to, torch on the `from` end). The forge hut in the armory, the altar in
/// the sanctum, and broken walls in the yard.
const WALLS: &[(Vec2, Vec2, bool)] = &[
    // The yard: lone broken walls among the graves.
    (Vec2::new(-10.5, -13.5), Vec2::new(-8.5, -13.5), true),
    (Vec2::new(8.5, -12.5), Vec2::new(10.5, -12.5), false),
    // The armory's forge hut, its door to the north.
    (Vec2::new(-18.5, -17.5), Vec2::new(-13.5, -17.5), true),
    (Vec2::new(-18.5, -17.5), Vec2::new(-18.5, -22.5), false),
    (Vec2::new(-13.5, -17.5), Vec2::new(-13.5, -22.5), false),
    (Vec2::new(-18.5, -22.5), Vec2::new(-13.5, -22.5), false),
    // Racks fallen over in the armory.
    (Vec2::new(-27.5, -15.5), Vec2::new(-27.5, -19.5), true),
    (Vec2::new(-15.5, 13.5), Vec2::new(-15.5, 16.5), false),
    // The cistern's broken aqueduct piers.
    (Vec2::new(14.5, -20.5), Vec2::new(14.5, -18.5), true),
    (Vec2::new(19.5, -22.5), Vec2::new(19.5, -20.5), false),
    (Vec2::new(24.5, -24.5), Vec2::new(24.5, -22.5), false),
    // The sanctum's altar, its door to the south.
    (Vec2::new(-3.5, 25.5), Vec2::new(-1.5, 25.5), true),
    (Vec2::new(1.5, 25.5), Vec2::new(3.5, 25.5), true),
    (Vec2::new(-3.5, 25.5), Vec2::new(-3.5, 28.5), false),
    (Vec2::new(3.5, 25.5), Vec2::new(3.5, 28.5), false),
    (Vec2::new(-3.5, 28.5), Vec2::new(3.5, 28.5), false),
];

/// The standing stones around the sanctum's altar.
fn standing_stones() -> Vec<Vec2> {
    (0..8)
        .map(|i| {
            let a = i as f32 / 8.0 * std::f32::consts::TAU + 0.4;
            Vec2::new(0.5, 19.5) + Vec2::new(a.cos(), a.sin() * 0.8) * 6.0
        })
        .collect()
}

/// The graves in the south yard, rows either side of the road, every other row shifted.
fn graves() -> Vec<Vec2> {
    let mut stones = Vec::new();
    for row in 0..4 {
        let y = -14.5 - 3.0 * row as f32;
        let shift = (row % 2) as f32;
        for col in 0..3 {
            stones.push(Vec2::new(-9.5 + 2.0 * col as f32 + shift, y));
            stones.push(Vec2::new(4.5 + 2.0 * col as f32 + shift, y));
        }
    }
    stones
}

const ROCKS: &[Vec2] = &[
    // Rubble in the yard.
    Vec2::new(-10.5, 4.5),
    Vec2::new(10.5, -8.5),
    Vec2::new(-4.5, -27.5),
    Vec2::new(5.5, -26.5),
    // The armory's fallen stone.
    Vec2::new(-25.5, -24.5),
    Vec2::new(-24.5, -24.5),
    Vec2::new(-27.5, 20.5),
    Vec2::new(-19.5, 24.5),
    Vec2::new(-14.5, -26.5),
    // The cistern's.
    Vec2::new(27.5, 17.5),
    Vec2::new(15.5, 21.5),
    Vec2::new(27.5, -17.5),
    // The sanctum's.
    Vec2::new(-9.5, 22.5),
    Vec2::new(9.5, 16.5),
];

const TREES: &[Vec2] = &[
    Vec2::new(-10.5, -24.5),
    Vec2::new(10.5, -21.5),
    Vec2::new(3.5, -12.5),
    Vec2::new(17.5, 24.5),
    Vec2::new(25.5, 13.5),
    Vec2::new(-24.5, 13.5),
    Vec2::new(-17.5, 22.5),
    Vec2::new(-8.5, 16.5),
    Vec2::new(8.5, 24.5),
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

/// What lies out there, by the stage that opens it (0: there from the start).
const PICKUPS: &[(u8, Vec2, PickupKind)] = &[
    (0, Vec2::new(-2.5, -1.5), PickupKind::Heal),
    (0, Vec2::new(0.5, -16.5), PickupKind::Haste),
    (0, Vec2::new(8.5, -22.5), PickupKind::Heal),
    (0, Vec2::new(-8.5, -27.5), PickupKind::Fury),
    (1, Vec2::new(14.5, 8.5), PickupKind::Heal),
    (1, Vec2::new(18.5, -4.5), PickupKind::Fury),
    (1, Vec2::new(24.5, -17.5), PickupKind::Haste),
    (2, Vec2::new(-14.5, 10.5), PickupKind::Heal),
    (2, Vec2::new(-24.5, 8.5), PickupKind::Fury),
    (2, Vec2::new(-16.5, -8.5), PickupKind::Haste),
    (3, Vec2::new(0.5, 19.5), PickupKind::Fury),
    (3, Vec2::new(-8.5, 18.5), PickupKind::Heal),
];

const LAIRS: &[(u8, Vec2)] = &[
    (0, Vec2::new(-9.5, -26.5)),
    (0, Vec2::new(9.5, -26.5)),
    (0, Vec2::new(-9.5, 9.5)),
    (0, Vec2::new(9.5, 9.5)),
    (1, Vec2::new(26.5, 24.5)),
    (1, Vec2::new(26.5, -24.5)),
    (1, Vec2::new(14.5, 14.5)),
    (2, Vec2::new(-26.5, 24.5)),
    (2, Vec2::new(-26.5, -24.5)),
    (2, Vec2::new(-14.5, 15.5)),
    (3, Vec2::new(-9.5, 26.5)),
    (3, Vec2::new(9.5, 26.5)),
    (3, Vec2::new(8.5, 13.5)),
];

const CHESTS: &[(u8, Vec2)] = &[
    (0, Vec2::new(10.5, -27.5)),
    (0, Vec2::new(-10.5, -18.5)),
    (1, Vec2::new(26.5, 12.5)),
    (1, Vec2::new(16.5, -13.5)),
    (1, Vec2::new(27.5, -2.5)),
    (2, Vec2::new(-26.5, 12.5)),
    (2, Vec2::new(-15.5, -24.5)),
    (2, Vec2::new(-27.5, -8.5)),
    (3, Vec2::new(-8.5, 27.5)),
    (3, Vec2::new(8.5, 27.5)),
];

/// Roads: out of the camp's gates, and down each court once it's open.
const ROADS: &[(u8, Vec2, Vec2)] = &[
    (0, Vec2::new(0.0, -6.5), Vec2::new(0.0, -27.0)),
    (0, Vec2::new(6.5, 0.0), Vec2::new(11.0, 0.0)),
    (0, Vec2::new(-6.5, 0.0), Vec2::new(-11.0, 0.0)),
    (0, Vec2::new(0.0, 6.5), Vec2::new(0.0, 11.0)),
    (1, Vec2::new(12.0, 0.0), Vec2::new(16.0, -4.0)),
    (1, Vec2::new(16.0, -4.0), Vec2::new(26.0, -17.0)),
    (2, Vec2::new(-12.0, 0.0), Vec2::new(-16.0, -4.0)),
    (2, Vec2::new(-16.0, -4.0), Vec2::new(-16.0, -16.0)),
    (3, Vec2::new(0.0, 12.0), Vec2::new(0.5, 18.0)),
];

/// The middle tile of the gate this staged variant just opened (none for the base).
pub(super) fn gate_opened(id: MapId) -> Option<Vec2> {
    let stage = match id {
        MapId::Bastion2 => 1,
        MapId::Bastion3 => 2,
        MapId::Bastion4 => 3,
        _ => return None,
    };
    GATES.iter().find(|gate| gate.stage == stage).map(|gate| gate.tiles[1])
}

/// Distance from `p` to the segment `a`-`b`.
fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    if ab.length_squared() < 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The bastion at `stage`: 0 the yard alone, each stage after it one more court open.
pub(super) fn build(id: MapId, stage: u8) -> Map {
    let mut map = Map::blank(id, SIZE, edge, beyond);
    let roads: Vec<(Vec2, Vec2)> = ROADS.iter().filter(|(s, ..)| *s <= stage).map(|&(_, a, b)| (a, b)).collect();
    map.paint(|c, tile| {
        // Outside the curtain wall there is only forest.
        if c.x.abs().max(c.y.abs()) > CURTAIN + 0.5 {
            return Tile::Forest;
        }
        // A sealed court stands choked with black thicket until its gate opens and the way is
        // cut clear.
        if court_stage(c) > stage {
            let t = Map::tile_of(c);
            return if (t.x + t.y).rem_euclid(3) == 0 && wave(c.x * 0.31 + c.y * 0.17) > -0.5 { Tile::Tree } else { Tile::Forest };
        }
        if in_pool(c) {
            // The cistern's pools, crossed on planks in the north and a ford in the south.
            return if (c.y - 3.5).abs() < 1.0 && c.x > 16.0 {
                Tile::Planks
            } else if (c.y + 10.5).abs() < 1.0 {
                Tile::Ford
            } else {
                Tile::Water
            };
        }
        if in_chasm(c) {
            // The armory's chasm, planks laid over the middle of it.
            return if (c.y - 1.5).abs() < 1.0 { Tile::Planks } else { Tile::Chasm };
        }
        if c.x.abs().max(c.y.abs()) < BAILEY {
            return Tile::Path;
        }
        if roads.iter().any(|&(a, b)| to_segment(c, a, b) < 1.0) {
            return Tile::Path;
        }
        // The cistern's floor is sodden around its pools.
        if c.x > COURT + 1.0 && POOLS.iter().any(|&(at, radii)| ((c - at) / (radii + Vec2::splat(2.5))).length_squared() < 1.0) {
            return Tile::Marsh;
        }
        tile
    });
    // The curtain wall, and the cross walls that seal the courts.
    let mut wall_ring = |r: f32, open_mid: bool| {
        let steps = (r + 0.5) as i32;
        for i in -steps..steps {
            let along = i as f32 + 0.5;
            if open_mid && along.abs() < 2.0 {
                continue;
            }
            for p in [Vec2::new(along, r), Vec2::new(along, -r), Vec2::new(r, along), Vec2::new(-r, along)] {
                map.set(p, Tile::Wall);
            }
        }
    };
    wall_ring(CURTAIN, false);
    wall_ring(BAILEY, true);
    map.line(Vec2::new(COURT, -CURTAIN), Vec2::new(COURT, CURTAIN), Tile::Wall);
    map.line(Vec2::new(-COURT, -CURTAIN), Vec2::new(-COURT, CURTAIN), Tile::Wall);
    map.line(Vec2::new(-COURT + 1.0, COURT), Vec2::new(COURT - 1.0, COURT), Tile::Wall);
    // The open gates are carved back out, and torches lit beside them.
    let mut torches: Vec<Vec2> = [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)].map(|(x, y)| Vec2::new(x, y) * BAILEY).to_vec();
    torches.extend([Vec2::new(2.5, 7.6), Vec2::new(-2.5, -7.6), Vec2::new(7.6, -2.5), Vec2::new(-7.6, 2.5)]);
    for gate in &GATES {
        if gate.stage > stage {
            continue;
        }
        for t in gate.tiles {
            map.set(t, Tile::Path);
        }
        torches.extend(gate.torches);
    }
    for &(a, b, torch) in WALLS {
        if court_stage((a + b) / 2.0) > stage {
            continue;
        }
        map.line(a, b, Tile::Wall);
        if torch {
            torches.push(a);
        }
    }
    for p in graves() {
        map.set(p, Tile::Rock);
    }
    for p in standing_stones() {
        if stage >= 3 {
            map.set(p, Tile::Rock);
        }
    }
    for &p in ROCKS {
        if court_stage(p) <= stage {
            map.set(p, Tile::Rock);
        }
    }
    for &p in TREES {
        if court_stage(p) <= stage {
            map.set(p, Tile::Tree);
        }
    }
    map.spawn_points.extend_from_slice(SPAWNS);
    map.pickups = PICKUPS.iter().filter(|(s, ..)| *s <= stage).map(|&(_, p, kind)| (p, kind)).collect();
    map.torches = torches;
    // Lairs and chests on walkable ground, only in the open courts.
    let walkable_near = |map: &Map, p: Vec2| map.nearest_walkable(Map::tile_of(p), 4).map(Map::center).expect("open ground near a lair or chest");
    map.lairs = LAIRS.iter().filter(|(s, _)| *s <= stage).map(|&(_, p)| walkable_near(&map, p)).collect();
    map.chests = CHESTS.iter().filter(|(s, _)| *s <= stage).map(|&(_, p)| walkable_near(&map, p)).collect();
    map.shop = Some(Vec2::new(0.5, 1.5));
    map.line(Vec2::new(-0.5, 2.5), Vec2::new(1.5, 2.5), Tile::Wall);
    map.counter = [-0.5, 0.5, 1.5].map(|x| Map::tile_of(Vec2::new(x, 2.5))).to_vec();
    map.shop_faces = Vec2::NEG_Y;
    map
}
