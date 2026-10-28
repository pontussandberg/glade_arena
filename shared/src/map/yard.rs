//! Practice Yard: a cozy village in open meadow (no forest to fill the view), at sunset. A
//! cobbled lane runs in from the west gate, where fighters come in, through the village square
//! (a well in its middle, the merchant's stall on its north side, lanterns at its corners) and
//! on east over a bridge across the stream to the open ground beyond. Timber-framed houses
//! with lit windows stand along the lane and round the square, with fences and flower beds
//! between them, trees here and there. And high ground to fight from and at (`relief`): a
//! plateau north of the village with sheer sides and a ramp up its south face, a hay stage
//! south of the lane with its own ramp, and a broad hill across the stream, each with a target
//! dummy on top; two more dummies stand in the square from the start, and one far down the
//! lane. For trying a hero's kit, aim and movement out, not for fighting over.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, squircle};

const SIZE: IVec2 = IVec2::new(96, 60);

/// Meters inside the field's edge (negative outside): a rounded rectangle, no wobble.
fn edge(p: Vec2) -> f32 {
    (1.0 - squircle(Vec2::new(p.x / 44.5, p.y / 26.5))) * 20.0
}

fn beyond(_: Vec2) -> Tile {
    Tile::Forest
}

/// Spawn points at the west gate; each is mirrored onto the east end too.
const WEST_SPAWNS: &[Vec2] = &[Vec2::new(-40.5, -6.5), Vec2::new(-40.5, -2.5), Vec2::new(-40.5, 1.5), Vec2::new(-40.5, 5.5)];

/// The square's middle (the well), and how far the paving reaches from it each way.
const SQUARE: Vec2 = Vec2::new(0.5, 0.5);
const PAVING: f32 = 6.0;

/// The shop: the merchant on the square's north side, facing south over the counter behind
/// them.
const SHOP: Vec2 = Vec2::new(0.5, 7.5);

/// The stream: these columns of tiles, the lane's bridge over it, and a ford further south.
const STREAM: (i32, i32) = (20, 22);
const FORD_Y: f32 = -12.5;

/// The houses: lower-left tile, size (tiles), which way the door faces.
const HOUSES: &[(IVec2, IVec2, IVec2)] = &[
    (IVec2::new(-17, 3), IVec2::new(5, 4), IVec2::NEG_Y),
    (IVec2::new(-15, -8), IVec2::new(4, 4), IVec2::Y),
    (IVec2::new(9, 3), IVec2::new(6, 4), IVec2::NEG_Y),
    (IVec2::new(10, -9), IVec2::new(4, 5), IVec2::Y),
    (IVec2::new(-6, 10), IVec2::new(5, 4), IVec2::NEG_Y),
    (IVec2::new(-24, -15), IVec2::new(4, 4), IVec2::X),
    (IVec2::new(1, -14), IVec2::new(7, 5), IVec2::Y),
    (IVec2::new(-28, 5), IVec2::new(4, 3), IVec2::NEG_Y),
];

/// Fences (from, to), round the yards.
const FENCES: &[(Vec2, Vec2)] = &[
    (Vec2::new(-18.0, 2.0), Vec2::new(-18.0, 8.0)),
    (Vec2::new(-18.0, 8.0), Vec2::new(-11.0, 8.0)),
    (Vec2::new(-16.0, -9.0), Vec2::new(-10.0, -9.0)),
    (Vec2::new(-10.0, -9.0), Vec2::new(-10.0, -3.0)),
    (Vec2::new(9.0, 8.0), Vec2::new(16.0, 8.0)),
    (Vec2::new(16.0, 8.0), Vec2::new(16.0, 2.0)),
    (Vec2::new(-7.0, 9.0), Vec2::new(-7.0, 15.0)),
    (Vec2::new(0.0, 9.0), Vec2::new(0.0, 15.0)),
    (Vec2::new(9.0, -10.0), Vec2::new(15.0, -10.0)),
    (Vec2::new(15.0, -10.0), Vec2::new(15.0, -3.0)),
];

/// Flower beds (tile centers).
const GARDENS: &[Vec2] = &[
    Vec2::new(-16.5, 2.5), Vec2::new(-15.5, 2.5), Vec2::new(-12.5, 2.5),
    Vec2::new(-14.5, -3.5), Vec2::new(-11.5, -3.5),
    Vec2::new(9.5, 2.5), Vec2::new(14.5, 2.5), Vec2::new(10.5, 7.5), Vec2::new(13.5, 7.5),
    Vec2::new(-6.5, 9.5), Vec2::new(-1.5, 9.5),
    Vec2::new(10.5, -3.5), Vec2::new(13.5, -3.5),
    Vec2::new(-7.5, -7.5), Vec2::new(-7.5, 7.5), Vec2::new(8.5, -7.5), Vec2::new(8.5, 7.5),
];

/// Trees, here and there.
const TREES: &[Vec2] = &[
    Vec2::new(-30.5, -6.5), Vec2::new(-33.5, 9.5), Vec2::new(-21.5, 12.5), Vec2::new(-20.5, -20.5),
    Vec2::new(-8.5, -19.5), Vec2::new(12.5, -18.5), Vec2::new(17.5, 12.5), Vec2::new(6.5, 16.5),
    Vec2::new(-35.5, -16.5), Vec2::new(25.5, -8.5), Vec2::new(26.5, 15.5), Vec2::new(38.5, -14.5),
    Vec2::new(-26.5, 20.5), Vec2::new(16.5, 20.5),
];

/// Boulders on the outskirts.
const ROCKS: &[Vec2] = &[Vec2::new(27.5, -4.5), Vec2::new(-32.5, 16.5), Vec2::new(30.5, 19.5), Vec2::new(-30.5, -20.5), Vec2::new(24.5, 10.5)];

/// Lanterns: the square's corners, the gate, the bridge, and down the far lane.
const LANTERNS: &[Vec2] = &[
    Vec2::new(-5.5, -5.5), Vec2::new(6.5, -5.5), Vec2::new(-5.5, 6.5), Vec2::new(6.5, 6.5),
    Vec2::new(-36.5, 2.5), Vec2::new(-36.5, -1.5),
    Vec2::new(18.5, 2.5), Vec2::new(18.5, -1.5),
    Vec2::new(-6.5, -14.5), Vec2::new(-18.5, 1.5),
];

/// The dummies: two in the square, one far down the lane, and one on each high place.
const DUMMIES: &[Vec2] = &[
    Vec2::new(4.5, -3.5),
    Vec2::new(-3.5, 4.5),
    Vec2::new(30.5, 0.5),
    Vec2::new(PLATEAU.x + 0.5, PLATEAU.y + 0.5),
    Vec2::new(STAGE.x + 0.5, STAGE.y + 0.5),
    Vec2::new(HILL.x + 0.5, HILL.y + 0.5),
];

/// The high ground: where the plateau stands (its middle) and how high and wide it is, with the
/// ramp up its south face; the hay stage likewise, its ramp from the east; the hill's crown,
/// height and reach.
const PLATEAU: Vec2 = Vec2::new(-10.0, 20.0);
const PLATEAU_TOP: f32 = 4.0;
const PLATEAU_RADIUS: f32 = 5.0;
const STAGE: Vec2 = Vec2::new(-14.0, -18.0);
const STAGE_TOP: f32 = 2.4;
const STAGE_HALF: f32 = 2.0;
const HILL: Vec2 = Vec2::new(35.0, 6.0);
const HILL_TOP: f32 = 5.0;
const HILL_REACH: f32 = 12.0;
/// How wide a ramp is (each way from its middle), and how far it runs for its height: a slope a
/// fighter walks up (under `map::CLIMB`).
const RAMP_HALF: f32 = 1.5;
const RAMP_RUN: f32 = 1.5;
/// Over how many meters a sheer side drops: a cliff, not walked up.
const SHEER: f32 = 0.6;

/// 0 to 1 smoothly as `t` goes 0 to 1.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How high the yard's ground is at `p`: flat but for the plateau, the stage and the hill.
fn relief(p: Vec2) -> f32 {
    // The plateau: its top inside its radius, sheer down just outside; a ramp up to it from the
    // south, rising over its run and then level onto the top.
    let plateau = PLATEAU_TOP * (1.0 - ((p.distance(PLATEAU) - PLATEAU_RADIUS) / SHEER).clamp(0.0, 1.0));
    let ramp_foot = PLATEAU.y - PLATEAU_RADIUS - PLATEAU_TOP * RAMP_RUN;
    let ramp = if (p.x - PLATEAU.x).abs() <= RAMP_HALF && p.y < PLATEAU.y { PLATEAU_TOP * ((p.y - ramp_foot) / (PLATEAU_TOP * RAMP_RUN)).clamp(0.0, 1.0) } else { 0.0 };
    // The stage: a square deck, sheer-sided, its ramp running up from the east.
    let out = (p - STAGE).abs().max_element();
    let deck = STAGE_TOP * (1.0 - ((out - STAGE_HALF) / SHEER).clamp(0.0, 1.0));
    let deck_foot = STAGE.x + STAGE_HALF + STAGE_TOP * RAMP_RUN;
    let deck_ramp = if (p.y - STAGE.y).abs() <= RAMP_HALF && p.x > STAGE.x { STAGE_TOP * ((deck_foot - p.x) / (STAGE_TOP * RAMP_RUN)).clamp(0.0, 1.0) } else { 0.0 };
    // The hill: a broad dome, walked up any way.
    let hill = HILL_TOP * smooth(1.0 - p.distance(HILL) / HILL_REACH);
    plateau.max(ramp).max(deck).max(deck_ramp).max(hill)
}

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Yard, SIZE, edge, beyond);
    map.relief = relief;
    // The lane from the gate to the far end, the square, and the side lanes to the stall and
    // the inn.
    map.fill(Vec2::new(-42.5, -1.5), Vec2::new(42.5, 1.5), Tile::Path);
    map.fill(SQUARE - Vec2::splat(PAVING), SQUARE + Vec2::splat(PAVING - 1.0), Tile::Path);
    map.fill(Vec2::new(-0.5, 6.5), Vec2::new(1.5, 8.5), Tile::Path);
    map.fill(Vec2::new(3.5, -8.5), Vec2::new(5.5, -6.5), Tile::Path);
    map.fill(Vec2::new(-14.5, -3.5), Vec2::new(-13.5, -2.5), Tile::Path);
    map.fill(Vec2::new(-14.5, 1.5), Vec2::new(-13.5, 2.5), Tile::Path);
    map.fill(Vec2::new(11.5, 1.5), Vec2::new(12.5, 2.5), Tile::Path);
    map.fill(Vec2::new(11.5, -3.5), Vec2::new(12.5, -2.5), Tile::Path);
    // The stream, with the lane's bridge and a ford.
    map.fill(Vec2::new(STREAM.0 as f32 + 0.5, -40.0), Vec2::new(STREAM.1 as f32 - 0.5, 40.0), Tile::Water);
    map.fill(Vec2::new(STREAM.0 as f32 + 0.5, -1.5), Vec2::new(STREAM.1 as f32 - 0.5, 1.5), Tile::Bridge);
    map.fill(Vec2::new(STREAM.0 as f32 + 0.5, FORD_Y), Vec2::new(STREAM.1 as f32 - 0.5, FORD_Y), Tile::Ford);
    for &(lo, size, door) in HOUSES {
        map.house(lo, size, door);
    }
    for &p in TREES {
        map.set(p, Tile::Tree);
    }
    for &p in ROCKS {
        map.set(p, Tile::Rock);
    }
    map.mirror_spots(WEST_SPAWNS, &[], &[]);
    map.dummies.extend(DUMMIES);
    map.fences.extend(FENCES);
    map.gardens.extend(GARDENS);
    map.torches.extend(LANTERNS);
    // The well in the square's middle.
    map.set(SQUARE, Tile::Rock);
    map.well = Some(SQUARE);
    // The merchant's stall on the square's north side, facing the square: a counter behind them
    // with a torch at each end.
    map.shop = Some(SHOP);
    let counter = SHOP + Vec2::Y;
    map.line(counter - Vec2::X, counter + Vec2::X, Tile::Wall);
    map.counter = [-1.0, 0.0, 1.0].map(|x| Map::tile_of(counter + Vec2::X * x)).to_vec();
    map.torches.extend([counter - Vec2::X * 2.0, counter + Vec2::X * 2.0]);
    map
}
