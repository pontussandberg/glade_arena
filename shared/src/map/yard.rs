//! Practice Yard: a cozy village in open meadow (no forest to fill the view), at sunset, on
//! several levels. A cobbled lane runs in from the west gate, where fighters come in, past the
//! watchtower (a switchback ramp up to its top), through the village square (a well in its
//! middle, the merchant's stall on its north side, lanterns at its corners) and on east over an
//! arched stone bridge across the stream to the open ground and the hill beyond. North of the
//! square the upper village stands on a terrace, reached by two stone stairs, with the plateau
//! rising behind it; a causeway runs out from the terrace's east end over the stream on a high
//! footbridge. Timber-framed houses with lit windows stand along the lane, round the square and
//! up on the terrace, with fences and flower beds between them, trees here and there. Every
//! sheer edge is faced with stone (`scene::Props::retaining`). Target dummies stand in the
//! square, down the far lane, and on each high place. For trying a hero's kit, aim and movement
//! out, not for fighting over.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, squircle};

const SIZE: IVec2 = IVec2::new(96, 64);

/// Meters inside the field's edge (negative outside): a rounded rectangle, no wobble.
fn edge(p: Vec2) -> f32 {
    (1.0 - squircle(Vec2::new(p.x / 44.5, p.y / 28.5))) * 20.0
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
const SHOP: Vec2 = Vec2::new(0.5, 6.5);

/// The stream: these columns of tiles; the lane's bridge over it, the footbridge from the
/// causeway, and a ford further south.
const STREAM: (i32, i32) = (20, 22);
const FORD_Y: f32 = -12.5;

/// The terrace the upper village stands on: its box (meters, inclusive), its height, and where
/// its stairs come down its south face (x, each `RAMP_HALF` wide). Every ramp's middle is a
/// tile's middle, so its sides are tile edges, where the ground and its stone faces are drawn.
const TERRACE: (Vec2, Vec2) = (Vec2::new(-22.0, 10.0), Vec2::new(16.0, 27.0));
const TERRACE_TOP: f32 = 2.5;
const STAIRS: [f32; 2] = [-9.5, 7.5];
/// The lower terrace south of the square, the inn's: its box, its height, its stair up from the
/// square's south lane (x).
const SOUTH: (Vec2, Vec2) = (Vec2::new(-4.0, -20.0), Vec2::new(16.0, -9.0));
const SOUTH_TOP: f32 = 1.5;
const SOUTH_STAIR: f32 = 4.5;

/// The causeway from the terrace's east end over the stream: its strip (y, half-width), how far
/// east it runs level before sloping down to the ground.
const CAUSEWAY_Y: f32 = 12.5;
const CAUSEWAY_END: f32 = 26.0;

/// The watchtower at the square's south-west corner, overlooking it: its top's box (tile
/// edges), and its height.
const TOWER: (Vec2, Vec2) = (Vec2::new(-12.0, -12.0), Vec2::new(-9.0, -6.0));
const TOWER_TOP: f32 = 6.0;

/// The houses: lower-left tile, size (tiles), which way the door faces.
const HOUSES: &[(IVec2, IVec2, IVec2)] = &[
    (IVec2::new(-17, 3), IVec2::new(5, 4), IVec2::NEG_Y),
    (IVec2::new(-17, -8), IVec2::new(4, 4), IVec2::Y),
    (IVec2::new(9, 3), IVec2::new(6, 4), IVec2::NEG_Y),
    (IVec2::new(10, -14), IVec2::new(4, 5), IVec2::NEG_X),
    (IVec2::new(-26, -15), IVec2::new(4, 4), IVec2::X),
    (IVec2::new(-2, -16), IVec2::new(7, 5), IVec2::Y),
    (IVec2::new(-28, 5), IVec2::new(4, 3), IVec2::NEG_Y),
    // Up on the terrace.
    (IVec2::new(-20, 13), IVec2::new(5, 4), IVec2::NEG_Y),
    (IVec2::new(-6, 13), IVec2::new(6, 4), IVec2::NEG_Y),
    (IVec2::new(2, 13), IVec2::new(4, 5), IVec2::NEG_Y),
    (IVec2::new(9, 14), IVec2::new(5, 4), IVec2::NEG_X),
];

/// Stacks of crates (tile, how many high): steps up to a roof.
const CRATES: &[(IVec2, u8)] = &[(IVec2::new(15, 2), 1), (IVec2::new(14, 2), 2), (IVec2::new(13, 2), 3)];

/// Fences (from, to), round the yards and along the terrace's edge and the tower's top.
const FENCES: &[(Vec2, Vec2)] = &[
    (Vec2::new(-18.0, 2.0), Vec2::new(-18.0, 8.0)),
    (Vec2::new(-18.0, -9.0), Vec2::new(-13.0, -9.0)),
    (Vec2::new(16.0, 8.0), Vec2::new(16.0, 2.0)),
    (Vec2::new(-4.0, -8.7), Vec2::new(2.0, -8.7)),
    (Vec2::new(6.0, -8.7), Vec2::new(16.0, -8.7)),
    (Vec2::new(-22.0, 10.3), Vec2::new(-13.0, 10.3)),
    (Vec2::new(-7.0, 10.3), Vec2::new(5.0, 10.3)),
    (Vec2::new(9.0, 10.3), Vec2::new(16.0, 10.3)),
    (Vec2::new(-21.0, 18.0), Vec2::new(-14.0, 18.0)),
];

/// Flower beds (tile centers).
const GARDENS: &[Vec2] = &[
    Vec2::new(-16.5, 2.5), Vec2::new(-15.5, 2.5), Vec2::new(-12.5, 2.5),
    Vec2::new(-16.5, -3.5), Vec2::new(-13.5, -3.5),
    Vec2::new(9.5, 2.5), Vec2::new(10.5, 7.5), Vec2::new(13.5, 7.5),
    Vec2::new(8.5, -11.5), Vec2::new(8.5, -13.5), Vec2::new(-3.5, -10.5), Vec2::new(5.5, -10.5),
    Vec2::new(-7.5, 7.5), Vec2::new(8.5, -7.5), Vec2::new(8.5, 7.5),
    Vec2::new(-19.5, 12.5), Vec2::new(-15.5, 12.5), Vec2::new(-5.5, 12.5), Vec2::new(-0.5, 12.5), Vec2::new(3.5, 12.5),
];

/// Trees, here and there.
const TREES: &[Vec2] = &[
    Vec2::new(-30.5, -4.5), Vec2::new(-33.5, 9.5), Vec2::new(-20.5, -20.5), Vec2::new(-34.5, -9.5),
    Vec2::new(-8.5, -19.5), Vec2::new(12.5, -18.5), Vec2::new(17.5, 22.5), Vec2::new(6.5, 20.5),
    Vec2::new(-35.5, -16.5), Vec2::new(25.5, -8.5), Vec2::new(27.5, 18.5), Vec2::new(38.5, -14.5),
    Vec2::new(-26.5, 22.5), Vec2::new(-12.5, 24.5), Vec2::new(10.5, 24.5),
];

/// Boulders on the outskirts.
const ROCKS: &[Vec2] = &[Vec2::new(27.5, -4.5), Vec2::new(-32.5, 16.5), Vec2::new(30.5, 22.5), Vec2::new(-30.5, -20.5), Vec2::new(25.5, 4.5)];

/// Lanterns: the square's corners, the gate, the bridge, the stairs' feet, the tower's top.
const LANTERNS: &[Vec2] = &[
    Vec2::new(-5.5, -5.5), Vec2::new(6.5, -5.5), Vec2::new(-5.5, 6.5), Vec2::new(6.5, 6.5),
    Vec2::new(-36.5, 2.5), Vec2::new(-36.5, -1.5),
    Vec2::new(18.5, 2.5), Vec2::new(18.5, -1.5),
    Vec2::new(-18.5, 1.5),
    Vec2::new(-12.5, 8.5), Vec2::new(9.5, 8.5),
    Vec2::new(2.5, -7.5), Vec2::new(5.5, -7.5),
];

/// The dummies: two in the square, one far down the lane, and one on each high place (the
/// terrace, the plateau, the tower, the causeway's footbridge, the hill).
const DUMMIES: &[Vec2] = &[
    Vec2::new(4.5, -3.5),
    Vec2::new(-3.5, 4.5),
    Vec2::new(30.5, 0.5),
    Vec2::new(-14.5, 11.5),
    Vec2::new(12.5, -17.5),
    Vec2::new(PLATEAU.x, PLATEAU.y + 0.5),
    Vec2::new(-10.5, -10.5),
    Vec2::new(21.5, CAUSEWAY_Y),
    Vec2::new(HILL.x + 0.5, HILL.y + 0.5),
];

/// The high ground beyond the terrace: the plateau's middle, height and radius, with the ramp
/// up its south face from the terrace; the hill's crown, height and reach.
const PLATEAU: Vec2 = Vec2::new(-9.5, 23.0);
const PLATEAU_TOP: f32 = 6.0;
const PLATEAU_RADIUS: f32 = 4.5;
const HILL: Vec2 = Vec2::new(36.0, 2.0);
const HILL_TOP: f32 = 5.0;
const HILL_REACH: f32 = 12.0;
/// How wide a ramp or stair is (each way from its middle), and how far it runs for its height:
/// a slope a fighter walks up (under `map::CLIMB`).
const RAMP_HALF: f32 = 1.5;
const RAMP_RUN: f32 = 1.5;
/// Over how many meters a sheer side drops: a cliff, not walked up.
const SHEER: f32 = 0.5;

/// 0 to 1 smoothly as `t` goes 0 to 1.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far inside a box `p` is (meters; negative outside).
fn inside(p: Vec2, lo: Vec2, hi: Vec2) -> f32 {
    (p.x - lo.x).min(hi.x - p.x).min(p.y - lo.y).min(hi.y - p.y)
}

/// A flat top `height` high over `inside` (meters inside its edge), sheer just outside it.
fn ledge(inside: f32, height: f32) -> f32 {
    height * (1.0 - (-inside / SHEER).clamp(0.0, 1.0))
}

/// A ramp rising to `height` along `along` (0 at its foot, 1 at the top), `across` from its
/// middle: within `RAMP_HALF` of it.
fn ramp(along: f32, across: f32, height: f32) -> f32 {
    if across.abs() <= RAMP_HALF { height * along.clamp(0.0, 1.0) } else { 0.0 }
}

/// How high the yard's ground is at `p`: flat but for its levels.
fn relief(p: Vec2) -> f32 {
    let mut h: f32 = 0.0;
    // The south terrace, the inn's, with its stair up from the square's south lane.
    let (slo, shi) = SOUTH;
    h = h.max(ledge(inside(p, slo, shi), SOUTH_TOP));
    if p.y > shi.y {
        h = h.max(ramp(((shi.y + SOUTH_TOP * RAMP_RUN) - p.y) / (SOUTH_TOP * RAMP_RUN), p.x - SOUTH_STAIR, SOUTH_TOP));
    }
    // The terrace, with its two stairs down the south face.
    let (lo, hi) = TERRACE;
    h = h.max(ledge(inside(p, lo, hi), TERRACE_TOP));
    for stair in STAIRS {
        if p.y < lo.y {
            h = h.max(ramp((p.y - (lo.y - TERRACE_TOP * RAMP_RUN)) / (TERRACE_TOP * RAMP_RUN), p.x - stair, TERRACE_TOP));
        }
    }
    // The plateau behind it, its ramp up from the terrace.
    h = h.max(ledge(PLATEAU_RADIUS - p.distance(PLATEAU), PLATEAU_TOP));
    let run = (PLATEAU_TOP - TERRACE_TOP) * RAMP_RUN;
    let foot = PLATEAU.y - PLATEAU_RADIUS - run;
    if (p.x - PLATEAU.x).abs() <= RAMP_HALF && p.y >= foot && p.y < PLATEAU.y {
        h = h.max(TERRACE_TOP + (PLATEAU_TOP - TERRACE_TOP) * ((p.y - foot) / run).clamp(0.0, 1.0));
    }
    // The causeway from the terrace's east end over the stream, sloping down past it.
    if (p.y - CAUSEWAY_Y).abs() <= RAMP_HALF && p.x > hi.x - 1.0 {
        let down = (p.x - CAUSEWAY_END) / (TERRACE_TOP * RAMP_RUN);
        h = h.max(TERRACE_TOP * (1.0 - down.clamp(0.0, 1.0)));
    }
    // The watchtower: its top, and the switchback round its south and east sides, each a
    // ramp's width out from the tower.
    let (tlo, thi) = TOWER;
    h = h.max(ledge(inside(p, tlo, thi), TOWER_TOP));
    let width = 2.0 * RAMP_HALF;
    // The landing at the south-east corner is as high as the second leg, along the east side's
    // south half, leaves to climb.
    let landing = TOWER_TOP - width / RAMP_RUN;
    // First leg: along the south side, west to east, from the ground up to the landing.
    if p.y >= tlo.y - width && p.y <= tlo.y && p.x <= thi.x + width {
        let foot = thi.x - landing * RAMP_RUN;
        h = h.max(landing * ((p.x - foot) / (landing * RAMP_RUN)).clamp(0.0, 1.0));
    }
    // Second leg: north from the landing along the east side, up to the top, then level beside
    // the tower's north half, onto it.
    if p.x >= thi.x && p.x <= thi.x + width && p.y >= tlo.y && p.y <= thi.y {
        h = h.max(landing + (TOWER_TOP - landing) * ((p.y - tlo.y) / width).clamp(0.0, 1.0));
    }
    // The lane's bridge over the stream: an arch, up from either bank.
    let (west, east_bank) = (STREAM.0 as f32, STREAM.1 as f32);
    if p.y.abs() <= 2.0 {
        let rise = 2.0;
        let up = ((p.x - (west - rise * RAMP_RUN)) / (rise * RAMP_RUN)).clamp(0.0, 1.0);
        let down = ((p.x - east_bank) / (rise * RAMP_RUN)).clamp(0.0, 1.0);
        h = h.max(rise * up.min(1.0 - down));
    }
    // The hill: a broad dome, walked up any way.
    h = h.max(HILL_TOP * smooth(1.0 - p.distance(HILL) / HILL_REACH));
    h
}

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Yard, SIZE, edge, beyond);
    map.relief = relief;
    // The lane from the gate to the far end, the square, and the side lanes to the stall and
    // the inn; the terrace's own lane and its stairs; the causeway.
    map.fill(Vec2::new(-42.5, -1.5), Vec2::new(42.5, 1.5), Tile::Path);
    map.fill(SQUARE - Vec2::splat(PAVING), SQUARE + Vec2::splat(PAVING - 1.0), Tile::Path);
    map.fill(Vec2::new(SOUTH_STAIR - 1.0, -11.5), Vec2::new(SOUTH_STAIR + 1.0, -6.5), Tile::Path);
    map.fill(Vec2::new(-3.5, -10.5), Vec2::new(15.5, -9.5), Tile::Path);
    map.fill(Vec2::new(-14.5, -3.5), Vec2::new(-13.5, -2.5), Tile::Path);
    map.fill(Vec2::new(-14.5, 1.5), Vec2::new(-13.5, 2.5), Tile::Path);
    map.fill(Vec2::new(11.5, 1.5), Vec2::new(12.5, 2.5), Tile::Path);
    map.fill(Vec2::new(11.5, -3.5), Vec2::new(12.5, -2.5), Tile::Path);
    map.fill(Vec2::new(-21.5, 11.5), Vec2::new(15.5, 12.5), Tile::Path);
    for stair in STAIRS {
        map.fill(Vec2::new(stair - 1.0, 6.5), Vec2::new(stair + 1.0, 11.5), Tile::Path);
    }
    map.fill(Vec2::new(15.5, CAUSEWAY_Y - 1.0), Vec2::new(29.5, CAUSEWAY_Y + 1.0), Tile::Path);
    // The stream, with the lane's bridge, the causeway's footbridge and a ford.
    map.fill(Vec2::new(STREAM.0 as f32 + 0.5, -40.0), Vec2::new(STREAM.1 as f32 - 0.5, 40.0), Tile::Water);
    map.fill(Vec2::new(STREAM.0 as f32 + 0.5, -1.5), Vec2::new(STREAM.1 as f32 - 0.5, 1.5), Tile::Bridge);
    map.fill(Vec2::new(STREAM.0 as f32 + 0.5, CAUSEWAY_Y - 1.0), Vec2::new(STREAM.1 as f32 - 0.5, CAUSEWAY_Y + 1.0), Tile::Bridge);
    map.fill(Vec2::new(STREAM.0 as f32 + 0.5, FORD_Y), Vec2::new(STREAM.1 as f32 - 0.5, FORD_Y), Tile::Ford);
    for &(lo, size, door) in HOUSES {
        map.house(lo, size, door);
    }
    // Crates stacked up the east house's south wall, a step at a time, onto its roof.
    for &(t, high) in CRATES {
        map.crates(t, high);
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
