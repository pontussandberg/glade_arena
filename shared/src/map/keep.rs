//! Sunken Keep: a ruined keep on an island in the middle of a square moat, its curtain wall
//! breached, an altar in its paved courtyard. Each side reaches the island over a causeway or wades a
//! ford, both near its own end. Out in the four corners, marshes slow whoever wades in, a haste
//! lying in the deepest of each for those who dare.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, squircle, wave};
use crate::protocol::PickupKind;

const SIZE: IVec2 = IVec2::new(76, 52);

/// Meters inside the clearing's edge (negative outside): a rounded rectangle, wobbling.
fn edge(p: Vec2) -> f32 {
    let rho = squircle(Vec2::new(p.x / 33.5, p.y / 22.5));
    let wobble = 0.03 * wave(p.x * 0.13) * wave(p.y * 0.19) + 0.015 * wave(p.x * 0.29) * wave(p.y * 0.11);
    (1.0 + wobble - rho) * 20.0
}

fn beyond(_: Vec2) -> Tile {
    Tile::Forest
}

/// The moat runs between these squircle radii; the island is inside it.
const MOAT: (f32, f32) = (10.0, 13.0);
/// The curtain wall: a square ring of tiles this far out from the middle (tile centers).
const CURTAIN: f32 = 7.5;

/// Is `c` in the marsh centered on `at` with radii `r`? Its rim wavers.
fn in_bog(c: Vec2, at: Vec2, r: Vec2) -> bool {
    let d = (c - at) / r;
    let rim = 1.0 + 0.18 * wave(c.x * 0.7 + c.y * 0.3) * wave(c.y * 0.5 - c.x * 0.2);
    d.length_squared() < rim * rim
}

/// The marshes on the west half (each mirrored): one up north, one down south.
const WEST_BOGS: &[(Vec2, Vec2)] = &[(Vec2::new(-19.0, 13.5), Vec2::new(6.5, 4.0)), (Vec2::new(-21.0, -14.0), Vec2::new(5.0, 3.5))];

fn in_marsh(c: Vec2) -> bool {
    WEST_BOGS.iter().any(|&(at, r)| in_bog(c, at, r) || in_bog(c, -at, r))
}

const WEST_SPAWNS: &[Vec2] = &[Vec2::new(-30.5, 0.5), Vec2::new(-27.5, 7.5), Vec2::new(-28.5, -6.5), Vec2::new(-20.5, -2.5)];

const WEST_PICKUPS: &[(Vec2, PickupKind)] = &[
    // In the courtyard, just inside the west gate.
    (Vec2::new(-5.5, -1.5), PickupKind::Heal),
    // Deep in the northern marsh.
    (Vec2::new(-19.5, 13.5), PickupKind::Haste),
];

/// Walls on the west half (each mirrored): (from, to, torch on the `from` end).
const WEST_WALLS: &[(Vec2, Vec2, bool)] = &[
    // Corner towers of the curtain wall, 2 x 2.
    (Vec2::new(-7.5, 7.5), Vec2::new(-6.5, 7.5), true),
    (Vec2::new(-7.5, 6.5), Vec2::new(-6.5, 6.5), false),
    (Vec2::new(-7.5, -7.5), Vec2::new(-6.5, -7.5), true),
    (Vec2::new(-7.5, -6.5), Vec2::new(-6.5, -6.5), false),
    // A ruined gatehouse west of the causeway: two stubs flanking the road.
    (Vec2::new(-17.5, 5.5), Vec2::new(-15.5, 5.5), true),
    (Vec2::new(-17.5, -0.5), Vec2::new(-15.5, -0.5), true),
    // A broken barn in the south-west.
    (Vec2::new(-27.5, -12.5), Vec2::new(-24.5, -12.5), false),
    (Vec2::new(-27.5, -11.5), Vec2::new(-27.5, -10.5), false),
    // A chapel's ruin north of the moat, its doorway facing west.
    (Vec2::new(-3.5, 19.5), Vec2::new(1.5, 19.5), true),
    (Vec2::new(1.5, 18.5), Vec2::new(1.5, 17.5), false),
    (Vec2::new(-3.5, 17.5), Vec2::new(-1.5, 17.5), false),
    // A hedge of stones west of the north ford.
    (Vec2::new(-8.5, 16.5), Vec2::new(-6.5, 15.5), false),
    // Lone pillars out on the field.
    (Vec2::new(-25.5, 6.5), Vec2::new(-25.5, 6.5), false),
    (Vec2::new(-11.5, -17.5), Vec2::new(-11.5, -17.5), false),
    (Vec2::new(-12.5, 17.5), Vec2::new(-12.5, 17.5), false),
    // Pillars in the courtyard.
    (Vec2::new(-3.5, 3.5), Vec2::new(-3.5, 3.5), false),
    (Vec2::new(3.5, 3.5), Vec2::new(3.5, 3.5), false),
    // A wall stub inside each gate, so no one shoots straight through.
    (Vec2::new(-4.5, 2.5), Vec2::new(-4.5, 4.5), false),
];
const WEST_ROCKS: &[Vec2] = &[
    Vec2::new(-22.5, 0.5),
    Vec2::new(-14.5, -8.5),
    Vec2::new(-13.5, -9.5),
    Vec2::new(-31.5, 8.5),
    Vec2::new(-6.5, 19.5),
    Vec2::new(-4.5, -18.5),
];
const WEST_TREES: &[Vec2] = &[
    Vec2::new(-25.5, 12.5),
    Vec2::new(-14.5, 15.5),
    Vec2::new(-24.5, -16.5),
    Vec2::new(-17.5, -13.5),
    Vec2::new(-30.5, -3.5),
    Vec2::new(-20.5, 7.5),
    Vec2::new(-9.5, 13.5),
    Vec2::new(-2.5, -15.5),
];
/// Pools in the marsh: deep water no one wades.
const WEST_POOLS: &[Vec2] = &[
    Vec2::new(-16.5, 12.5),
    Vec2::new(-16.5, 13.5),
    Vec2::new(-15.5, 12.5),
    Vec2::new(-15.5, 13.5),
    Vec2::new(-15.5, 14.5),
    Vec2::new(-22.5, 14.5),
    Vec2::new(-23.5, 14.5),
    Vec2::new(-22.5, 15.5),
    Vec2::new(-21.5, -15.5),
    Vec2::new(-20.5, -15.5),
    Vec2::new(-21.5, -14.5),
];

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Keep, SIZE, edge, beyond);
    map.paint(|c, tile| {
        let d = squircle(c);
        let in_moat = d > MOAT.0 && d < MOAT.1;
        // The west causeway (north of the middle) and the south ford (west of it); mirrored, the
        // east causeway and north ford.
        let causeway = (c.x < 0.0 && c.y > 1.0 && c.y < 4.0) || (c.x > 0.0 && c.y < -1.0 && c.y > -4.0);
        let ford = (c.y < 0.0 && c.x > -5.0 && c.x < -3.0) || (c.y > 0.0 && c.x < 5.0 && c.x > 3.0);
        let in_curtain = c.x.abs().max(c.y.abs()) == CURTAIN;
        if in_moat && causeway {
            Tile::Bridge
        } else if in_moat && ford {
            Tile::Ford
        } else if in_moat {
            Tile::Water
        } else if in_curtain && !causeway && !ford {
            Tile::Wall
        } else if d >= MOAT.1 && causeway && c.x.abs() > 13.0 {
            // The road out to each causeway.
            Tile::Path
        } else if c.x.abs().max(c.y.abs()) < CURTAIN {
            // The courtyard's old paving.
            Tile::Path
        } else if in_marsh(c) {
            Tile::Marsh
        } else {
            tile
        }
    });
    // Breaches in the curtain wall: the north wall's west end and the east wall's north end
    // fallen (mirrored, the south wall's east end and the west wall's south end).
    for p in [Vec2::new(-2.5, CURTAIN), Vec2::new(-1.5, CURTAIN), Vec2::new(CURTAIN, 5.5), Vec2::new(CURTAIN, 4.5)] {
        map.set_mirrored(p, Tile::Grass);
    }
    let mut torches = map.place_half(WEST_WALLS, WEST_ROCKS, WEST_TREES);
    // The altar in the middle of the courtyard, 2 x 2, a torch on two corners.
    map.line_mirrored(Vec2::new(-0.5, 0.5), Vec2::new(0.5, 0.5), Tile::Wall);
    torches.push(Vec2::new(-0.5, 0.5));
    for &p in WEST_POOLS {
        map.set_mirrored(p, Tile::Water);
    }
    // Torches on the causeways' outer ends.
    torches.extend([Vec2::new(-13.6, 0.7), Vec2::new(-13.6, 4.3)]);
    map.mirror_spots(WEST_SPAWNS, WEST_PICKUPS, &torches);
    // The merchant in the middle, as near the one side as the other.
    map.shop = map.nearest_walkable(Map::tile_of(Vec2::ZERO), 4).map(Map::center);
    map
}
