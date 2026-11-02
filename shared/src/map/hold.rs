//! Ravenhold: the waves mode's first map, and a PvP one too (`Mode::maps`): a fortress on its
//! crag over a village, built in levels.
//! Not mirrored: there are no sides here.
//!
//! - South, the village on the low ground: a square with the well and the merchant, where the
//!   party starts; streets of houses close enough to run along their roofs (crates stacked up
//!   onto the first); the chapel up on its terrace in the west with its bell tower; a watchtower
//!   in the east, a switchback up its sides.
//! - Across the middle, the river, shallow enough to wade anywhere: the stone bridge on the road, a
//!   ford in the west, and in the east a broken bridge, both halves ending over the water with a
//!   gap between them to leap.
//! - North, the fortress: its curtain walls a walk along the top, towers on the corners and
//!   either side of the gate; the outer court inside, up the road through the gate, through a
//!   breach in the south wall where the walk's broken off (a gap to leap up there too) and through
//!   a postern in the east wall; stairs up onto the walks, crates stacked up to them; and the
//!   inner ward high over the court, up a great ramp, the keep standing on it, the north walls
//!   rising onto it from the walks.
//! - West, a ravine under the walls, crossed high up by a bridge from the west wall to the
//!   barbican on its crag beyond.
//! - East, the orchard outside the walls, an old ruin on the bank.
//! - Out past the west edge, the land rises sheer into high forest, and the river comes down off
//!   it over a waterfall.
//!
//! Monsters come out of the woods all round (`Map::lairs`) and off the barbican, and every high
//! place can be walked up to, so no one up there is out of their reach.

use bevy::math::{IVec2, Vec2};

use super::{Falls, Map, MapId, Tile, squircle};
use crate::protocol::PickupKind;

const SIZE: IVec2 = IVec2::new(132, 120);

/// Where the playable ground stops in the north: the fortress's back is up against the forest.
const NORTH_EDGE: f32 = 52.0;

/// Meters inside the field's edge (negative outside): a rounded rectangle, cut off straight
/// along the fortress's back.
fn edge(p: Vec2) -> f32 {
    ((1.0 - squircle(Vec2::new(p.x / 62.5, p.y / 55.5))) * 20.0).min(NORTH_EDGE - p.y)
}

/// The river runs on out of the map east and west, the ravine on north into the forest.
fn beyond(p: Vec2) -> Tile {
    if in_ravine(p) {
        Tile::Chasm
    } else if (RIVER.0..RIVER.1).contains(&p.y) {
        Tile::Water
    } else {
        Tile::Forest
    }
}

/// The river: the rows it runs between (y, tile edges), and how deep it is where a body wades
/// it (meters under the bank: its feet on the bed, the water about its shins), and how far in
/// from the bank its bed slopes down to that.
const RIVER: (f32, f32) = (-12.0, -5.0);
const WADE_DEPTH: f32 = 0.6;
const WADE_SHELF: f32 = 1.0;
/// The ford across it (x, tile edges).
const FORD: (f32, f32) = (-30.0, -27.0);

/// The cliff out past the field's west edge: where its foot runs (x), and how high the forest
/// on top of it is; it steps back west past the barbican's crag, from `CLIFF_NORTH` (y) on.
const CLIFF_FOOT: f32 = -64.0;
const CLIFF: f32 = 14.0;
const CLIFF_NORTH: f32 = 16.0;

/// The ravine west of the fortress: under the west wall north of the high bridge, further out
/// south of it, its south end short of the river.
fn in_ravine(p: Vec2) -> bool {
    let west = p.x >= -48.0;
    (west && p.x < -42.0 && p.y >= 2.0) || (west && p.x < -36.0 && p.y >= 24.0)
}

/// How high the levels are: the chapel's terrace, the watchtower's top and the landing on its
/// way up, the river's bridges, the outer court, the walks along the walls (and the barbican's
/// crag), and the inner ward.
const TERRACE: f32 = 1.8;
const LOOKOUT: f32 = 6.0;
const LANDING: f32 = 4.0;
const BRIDGE: f32 = 1.5;
const COURT: f32 = 2.5;
const WALK: f32 = 6.5;
const WARD: f32 = 9.0;

/// One piece of the lie of the land, over a box (meters, `lo` inclusive to `hi` exclusive; its
/// sides on tile edges, where its stone faces are drawn): flat, or a ramp rising along `up` (a
/// side of the grid) from `from` at its foot to `to` at its head.
#[derive(Clone, Copy)]
struct Ground {
    lo: Vec2,
    hi: Vec2,
    up: IVec2,
    from: f32,
    to: f32,
}

const fn level(lo: (f32, f32), hi: (f32, f32), top: f32) -> Ground {
    Ground { lo: Vec2::new(lo.0, lo.1), hi: Vec2::new(hi.0, hi.1), up: IVec2::ZERO, from: top, to: top }
}

const fn ramp(lo: (f32, f32), hi: (f32, f32), up: IVec2, from: f32, to: f32) -> Ground {
    Ground { lo: Vec2::new(lo.0, lo.1), hi: Vec2::new(hi.0, hi.1), up, from, to }
}

impl Ground {
    /// How high it is at `p`, if it's there.
    fn at(&self, p: Vec2) -> Option<f32> {
        if p.x < self.lo.x || p.y < self.lo.y || p.x >= self.hi.x || p.y >= self.hi.y {
            return None;
        }
        let size = self.hi - self.lo;
        let along = match self.up {
            IVec2::X => (p.x - self.lo.x) / size.x,
            IVec2::NEG_X => (self.hi.x - p.x) / size.x,
            IVec2::Y => (p.y - self.lo.y) / size.y,
            IVec2::NEG_Y => (self.hi.y - p.y) / size.y,
            _ => 0.0,
        };
        Some(self.from + (self.to - self.from) * along)
    }
}

const X: IVec2 = IVec2::X;
const NX: IVec2 = IVec2::NEG_X;
const Y: IVec2 = IVec2::Y;
const NY: IVec2 = IVec2::NEG_Y;

/// The lie of the land, piece by piece: where pieces overlap, the later one is the ground. Every
/// ramp is no steeper than a walker climbs (`map::CLIMB`).
const GROUND: &[Ground] = &[
    // The chapel's terrace in the west of the village, its stairs up from the lane and from the
    // east.
    level((-42.0, -50.0), (-24.0, -38.0), TERRACE),
    ramp((-34.0, -38.0), (-31.0, -35.0), NY, 0.0, TERRACE),
    ramp((-24.0, -46.0), (-21.0, -43.0), NX, 0.0, TERRACE),
    // The watchtower east of the village: its top, and the switchback up its south side and on
    // up its east side.
    level((36.0, -44.0), (39.0, -38.0), LOOKOUT),
    ramp((33.0, -47.0), (39.0, -44.0), X, 0.0, LANDING),
    level((39.0, -47.0), (42.0, -44.0), LANDING),
    ramp((39.0, -44.0), (42.0, -41.0), Y, LANDING, LOOKOUT),
    level((39.0, -41.0), (42.0, -38.0), LOOKOUT),
    // The stone bridge on the road, up off either bank.
    ramp((-3.0, -14.5), (3.0, -12.0), Y, 0.0, BRIDGE),
    level((-3.0, -12.0), (3.0, -5.0), BRIDGE),
    ramp((-3.0, -5.0), (3.0, -2.5), Y, BRIDGE, 0.0),
    // The broken bridge east of it: each half up off its bank, ending over the water.
    ramp((22.0, -14.5), (26.0, -12.0), Y, 0.0, BRIDGE),
    level((22.0, -12.0), (26.0, -10.0), BRIDGE),
    level((22.0, -7.0), (26.0, -5.0), BRIDGE),
    ramp((22.0, -5.0), (26.0, -2.5), Y, BRIDGE, 0.0),
    // The road up to the gate, and through it.
    ramp((-3.0, 0.0), (3.0, 6.0), Y, 0.0, COURT),
    level((-3.0, 6.0), (3.0, 9.0), COURT),
    // The outer court, and the yards either side of the inner ward.
    level((-33.0, 9.0), (33.0, 34.0), COURT),
    level((-33.0, 34.0), (-24.0, 49.0), COURT),
    level((24.0, 34.0), (33.0, 49.0), COURT),
    // The walks along the walls: the south wall either side of the gate, broken off west of it;
    // the west wall; the east wall either side of the postern; and the north walls, rising onto
    // the inner ward.
    level((-36.0, 6.0), (-24.0, 9.0), WALK),
    level((-21.0, 6.0), (-3.0, 9.0), WALK),
    level((3.0, 6.0), (36.0, 9.0), WALK),
    level((-36.0, 9.0), (-33.0, 52.0), WALK),
    level((33.0, 9.0), (36.0, 17.0), WALK),
    level((33.0, 20.0), (36.0, 52.0), WALK),
    ramp((-33.0, 49.0), (-24.0, 52.0), X, WALK, WARD),
    ramp((24.0, 49.0), (33.0, 52.0), NX, WALK, WARD),
    // The breach: the south wall's rubble heaped up into the court.
    ramp((-24.0, 2.0), (-21.0, 9.0), Y, 0.0, COURT),
    // The postern through the east wall, its ramp down outside.
    level((33.0, 17.0), (36.0, 20.0), COURT),
    ramp((36.0, 17.0), (42.0, 20.0), NX, 0.0, COURT),
    // Stairs up onto the walks from the court.
    ramp((-15.0, 9.0), (-12.0, 15.0), NY, COURT, WALK),
    ramp((12.0, 9.0), (15.0, 15.0), NY, COURT, WALK),
    ramp((-33.0, 24.0), (-27.0, 27.0), NX, COURT, WALK),
    ramp((27.0, 28.0), (33.0, 31.0), X, COURT, WALK),
    // The inner ward, and the great ramp up to it from the court.
    level((-24.0, 34.0), (24.0, 52.0), WARD),
    ramp((-3.0, 22.0), (3.0, 34.0), Y, COURT, WARD),
    // The crag running on behind the fortress into the forest, falling away east of it.
    level((-36.0, 52.0), (36.0, 64.0), WARD),
    ramp((36.0, 52.0), (46.0, 64.0), NX, 0.0, WARD),
    // The barbican on its crag across the ravine, its ramp up from the south, and the high
    // bridge from it to the west wall.
    level((-66.0, 16.0), (-48.0, 64.0), WALK),
    ramp((-56.0, 6.25), (-52.0, 16.0), Y, 0.0, WALK),
    level((-48.0, 24.0), (-36.0, 28.0), WALK),
    // The high forest out past the west edge, the river falling off it.
    level((-160.0, -160.0), (CLIFF_FOOT, CLIFF_NORTH), CLIFF),
    level((-160.0, CLIFF_NORTH), (-66.0, 160.0), CLIFF),
];

/// How high the ground is at `p`: the pieces of it (`GROUND`), and under the river (not its
/// bridges, nor its ford's stepping stones) its bed, sloping down from either bank and the ford,
/// so whoever wades in stands in the water and walks back out.
fn relief(p: Vec2) -> f32 {
    let h = GROUND.iter().fold(0.0, |h, ground| ground.at(p).unwrap_or(h));
    let ford = (FORD.0 - p.x).max(p.x - FORD.1);
    let inside = (p.y - RIVER.0).min(RIVER.1 - p.y).min(ford);
    if h <= 0.0 && inside > 0.0 { -WADE_DEPTH * (inside / WADE_SHELF).min(1.0) } else { h }
}

/// Every tile in the box from `lo` to `hi` (meters, on tile edges; `hi` exclusive) inside the
/// field's edge.
fn area(map: &mut Map, lo: (f32, f32), hi: (f32, f32), tile: Tile) {
    for y in lo.1 as i32..hi.1 as i32 {
        for x in lo.0 as i32..hi.0 as i32 {
            let c = Map::center(IVec2::new(x, y));
            if edge(c) >= 0.0 {
                map.set(c, tile);
            }
        }
    }
}

/// A box of tiles: its lower and upper corners (meters, on tile edges; the upper exclusive).
type Span = ((f32, f32), (f32, f32));

/// Paved: the village's streets, its square and its lanes; the ways up and through the fortress,
/// its walks and its inner ward; the barbican's path.
const PAVED: &[Span] = &[
    ((-2.0, -56.0), (2.0, -15.0)),
    ((-3.0, -15.0), (3.0, -12.0)),
    ((-11.0, -39.0), (11.0, -25.0)),
    ((-52.0, -33.0), (52.0, -30.0)),
    ((-34.0, -38.0), (-31.0, -35.0)),
    ((-24.0, -46.0), (-21.0, -43.0)),
    ((-30.0, -45.0), (-24.0, -43.0)),
    ((22.0, -15.0), (26.0, -12.0)),
    ((23.0, -30.0), (25.0, -15.0)),
    ((33.0, -47.0), (42.0, -38.0)),
    ((31.0, -47.0), (33.0, -33.0)),
    ((-3.0, -5.0), (3.0, 34.0)),
    ((22.0, -5.0), (26.0, 0.0)),
    ((-24.0, 2.0), (-21.0, 9.0)),
    ((-33.0, 17.0), (42.0, 20.0)),
    ((-36.0, 6.0), (36.0, 9.0)),
    ((-36.0, 9.0), (-33.0, 52.0)),
    ((33.0, 9.0), (36.0, 52.0)),
    ((-33.0, 49.0), (-24.0, 52.0)),
    ((24.0, 49.0), (33.0, 52.0)),
    ((-15.0, 9.0), (-12.0, 15.0)),
    ((12.0, 9.0), (15.0, 15.0)),
    ((-33.0, 24.0), (-27.0, 27.0)),
    ((27.0, 28.0), (33.0, 31.0)),
    ((-24.0, 34.0), (24.0, 52.0)),
    ((-56.0, 6.0), (-52.0, 28.0)),
    ((-52.0, 24.0), (-48.0, 28.0)),
];

/// Towers: lower-left tile, size (tiles), foot and top (meters), roofed or battlemented. Either
/// side of the gate; out on the south wall's corners, clear of the walks meeting there; clinging
/// to the west wall over the ravine and to the east wall over the orchard; on the inner ward's
/// corners; the keep; the chapel's bell tower; and the barbican's, on its crag.
const TOWERS: &[(IVec2, IVec2, f32, f32, bool)] = &[
    (IVec2::new(-8, 3), IVec2::new(5, 8), 0.0, 14.0, false),
    (IVec2::new(3, 3), IVec2::new(5, 8), 0.0, 14.0, false),
    (IVec2::new(-40, 2), IVec2::new(5, 5), 0.0, 12.5, true),
    (IVec2::new(35, 2), IVec2::new(5, 5), 0.0, 12.5, true),
    (IVec2::new(-41, 30), IVec2::new(5, 5), RAVINE_FOOT, 13.0, true),
    (IVec2::new(36, 36), IVec2::new(5, 5), 0.0, 13.0, true),
    (IVec2::new(-26, 32), IVec2::new(5, 5), COURT, 17.0, true),
    (IVec2::new(21, 32), IVec2::new(5, 5), COURT, 17.0, true),
    (IVec2::new(-9, 40), IVec2::new(18, 10), WARD, 23.0, true),
    (IVec2::new(-28, -48), IVec2::new(3, 3), TERRACE, 12.0, true),
    (IVec2::new(-60, 30), IVec2::new(5, 5), WALK, 15.5, true),
];

/// How far down the ravine a tower in it reaches from.
const RAVINE_FOOT: f32 = -8.0;

/// The houses: lower-left tile, size (tiles), which way the door faces, how many storeys. One
/// along the street north of the square and on toward the river, low enough to run along the
/// roofs of; two round the square and down the lanes, and for the court's barracks and
/// storehouse and the ward's hall and chapel.
const HOUSES: &[(IVec2, IVec2, IVec2, u8)] = &[
    // Along the street north of the square, a stride apart.
    (IVec2::new(-10, -24), IVec2::new(5, 4), X, 1),
    (IVec2::new(-10, -19), IVec2::new(5, 4), X, 1),
    (IVec2::new(5, -24), IVec2::new(5, 4), NX, 1),
    (IVec2::new(5, -19), IVec2::new(4, 4), NX, 1),
    // On toward the river.
    (IVec2::new(-18, -22), IVec2::new(5, 5), NY, 1),
    (IVec2::new(-26, -21), IVec2::new(4, 4), NY, 1),
    (IVec2::new(12, -22), IVec2::new(5, 4), NY, 1),
    (IVec2::new(18, -23), IVec2::new(4, 5), NY, 1),
    // Round the square and along the lanes.
    (IVec2::new(-20, -29), IVec2::new(6, 4), NY, 2),
    (IVec2::new(-20, -38), IVec2::new(6, 4), Y, 2),
    (IVec2::new(14, -29), IVec2::new(6, 4), NY, 2),
    (IVec2::new(14, -38), IVec2::new(5, 4), Y, 2),
    (IVec2::new(-12, -45), IVec2::new(6, 4), Y, 2),
    (IVec2::new(6, -45), IVec2::new(6, 4), Y, 2),
    (IVec2::new(-10, -53), IVec2::new(5, 4), X, 2),
    (IVec2::new(5, -53), IVec2::new(5, 4), NX, 2),
    (IVec2::new(26, -38), IVec2::new(5, 4), Y, 2),
    (IVec2::new(27, -28), IVec2::new(5, 4), NY, 2),
    (IVec2::new(-48, -28), IVec2::new(5, 4), NY, 2),
    (IVec2::new(-35, -28), IVec2::new(6, 4), NY, 2),
    // The chapel, up on its terrace.
    (IVec2::new(-40, -47), IVec2::new(10, 5), X, 1),
    // In the outer court: the barracks, the stables, the smithy, a storehouse.
    (IVec2::new(-32, 12), IVec2::new(5, 9), X, 2),
    (IVec2::new(25, 10), IVec2::new(6, 5), NX, 1),
    (IVec2::new(20, 24), IVec2::new(5, 4), NX, 1),
    (IVec2::new(-22, 24), IVec2::new(5, 4), X, 2),
    (IVec2::new(-32, 40), IVec2::new(4, 5), X, 1),
    // On the inner ward: the hall and the chapel.
    (IVec2::new(-21, 40), IVec2::new(8, 6), X, 2),
    (IVec2::new(14, 40), IVec2::new(7, 5), NX, 2),
];

/// Stacks of crates (tile, how many high): steps up onto a roof in the village, out from under
/// its eaves, and in the court up onto the south wall's walk.
const CRATES: &[(IVec2, u8)] = &[
    (IVec2::new(-10, -26), 1),
    (IVec2::new(-9, -26), 2),
    (IVec2::new(-8, -26), 3),
    (IVec2::new(-28, 9), 1),
    (IVec2::new(-29, 9), 2),
    (IVec2::new(-30, 9), 3),
    (IVec2::new(-31, 9), 4),
    (IVec2::new(23, 16), 1),
    (IVec2::new(24, 16), 2),
    (IVec2::new(-10, 46), 1),
    (IVec2::new(10, 46), 2),
    (IVec2::new(-30, 46), 1),
    (IVec2::new(31, 36), 1),
    (IVec2::new(-54, 40), 1),
    (IVec2::new(-53, 40), 2),
    (IVec2::new(-58, 22), 1),
];

/// Fences (from, to): round the village's yards, the court's training ring, along the lists on
/// the north bank.
const FENCES: &[(Vec2, Vec2)] = &[
    (Vec2::new(-22.0, -4.7), Vec2::new(-5.0, -4.7)),
    (Vec2::new(5.0, -4.7), Vec2::new(20.0, -4.7)),
    (Vec2::new(-14.0, 24.0), Vec2::new(-6.0, 24.0)),
    (Vec2::new(-14.0, 31.0), Vec2::new(-9.0, 31.0)),
    (Vec2::new(-14.0, 24.0), Vec2::new(-14.0, 31.0)),
    (Vec2::new(-6.0, 24.0), Vec2::new(-6.0, 28.0)),
    (Vec2::new(-27.0, -24.0), Vec2::new(-19.0, -24.0)),
    (Vec2::new(18.0, -24.0), Vec2::new(22.0, -24.0)),
    (Vec2::new(-44.0, -23.0), Vec2::new(-36.0, -23.0)),
    (Vec2::new(-14.0, -46.0), Vec2::new(-14.0, -41.0)),
    (Vec2::new(13.0, -46.0), Vec2::new(13.0, -41.0)),
    (Vec2::new(28.0, -23.0), Vec2::new(34.0, -23.0)),
];

/// Flower beds and kitchen plots (tile centers).
const GARDENS: &[Vec2] = &[
    Vec2::new(-24.5, -23.5), Vec2::new(-21.5, -23.5), Vec2::new(18.5, -23.5), Vec2::new(20.5, -23.5),
    Vec2::new(-13.5, -42.5), Vec2::new(-13.5, -44.5), Vec2::new(12.5, -42.5), Vec2::new(12.5, -44.5),
    Vec2::new(-42.5, -22.5), Vec2::new(-38.5, -22.5), Vec2::new(29.5, -22.5), Vec2::new(32.5, -22.5),
    Vec2::new(-38.5, -39.5), Vec2::new(-27.5, -39.5), Vec2::new(-26.5, 41.5), Vec2::new(-26.5, 42.5),
    Vec2::new(-25.5, 41.5), Vec2::new(29.5, 44.5), Vec2::new(27.5, 44.5), Vec2::new(-17.5, 47.5),
];

/// Trees: round the village's outskirts, along the river, and in rows in the orchard.
const TREES: &[Vec2] = &[
    Vec2::new(-26.5, -16.5), Vec2::new(-34.5, -18.5), Vec2::new(-44.5, -16.5), Vec2::new(-52.5, -26.5),
    Vec2::new(-50.5, -36.5), Vec2::new(-46.5, -42.5), Vec2::new(-19.5, -48.5), Vec2::new(-14.5, -52.5),
    Vec2::new(12.5, -50.5), Vec2::new(18.5, -45.5), Vec2::new(24.5, -50.5), Vec2::new(46.5, -34.5),
    Vec2::new(48.5, -26.5), Vec2::new(30.5, -16.5), Vec2::new(40.5, -18.5), Vec2::new(14.5, -15.5),
    Vec2::new(-12.5, 0.5), Vec2::new(14.5, 2.5), Vec2::new(-30.5, 0.5), Vec2::new(-50.5, 2.5),
    Vec2::new(-58.5, -8.5), Vec2::new(52.5, -8.5),
];

/// The orchard's rows: their x, and the y of each tree in them.
const ORCHARD: ([f32; 4], [f32; 9]) = ([44.5, 48.5, 52.5, 56.5], [8.5, 12.5, 24.5, 28.5, 32.5, 36.5, 40.5, 44.5, 48.5]);

/// Boulders: along the ravine's rim and the river's banks.
const ROCKS: &[Vec2] = &[
    Vec2::new(-38.5, 14.5), Vec2::new(-41.5, 20.5), Vec2::new(-50.5, 10.5),
    Vec2::new(-15.5, -3.5), Vec2::new(30.5, -3.5), Vec2::new(-36.5, -14.5), Vec2::new(44.5, -14.5),
];

/// The old ruin on the east bank: its walls (from, to).
const RUINS: &[(Vec2, Vec2)] = &[
    (Vec2::new(44.5, -1.5), Vec2::new(50.5, -1.5)),
    (Vec2::new(44.5, -1.5), Vec2::new(44.5, 3.5)),
    (Vec2::new(50.5, 1.5), Vec2::new(50.5, 3.5)),
];

/// Where the drops are left open to leap from, no parapet across them: the broken bridge's ends
/// over its gap, the walk's broken ends either side of the breach.
const LEAPS: &[(Vec2, Vec2)] = &[
    (Vec2::new(22.0, -10.2), Vec2::new(26.0, -9.8)),
    (Vec2::new(22.0, -7.2), Vec2::new(26.0, -6.8)),
    (Vec2::new(-24.2, 6.0), Vec2::new(-23.8, 9.0)),
    (Vec2::new(-21.2, 6.0), Vec2::new(-20.8, 9.0)),
];

/// Heaps of fallen stone (where, how far they spread, how many stones): either side of the
/// breach, outside the wall, on the walk's broken ends and up the sides of its slope; on the broken bridge's ends and in the
/// water under its gap; round the old ruin; at the barbican tower's foot.
const RUBBLE: &[(Vec2, f32, usize)] = &[
    (Vec2::new(-25.6, 4.6), 1.6, 9),
    (Vec2::new(-19.4, 4.6), 1.6, 9),
    (Vec2::new(-24.4, 7.5), 0.5, 3),
    (Vec2::new(-20.6, 7.5), 0.5, 3),
    (Vec2::new(-23.65, 5.5), 0.3, 3),
    (Vec2::new(-21.35, 4.0), 0.3, 3),
    (Vec2::new(24.0, -10.5), 1.4, 5),
    (Vec2::new(24.0, -6.5), 1.4, 5),
    (Vec2::new(24.0, -8.5), 2.2, 8),
    (Vec2::new(47.5, 1.0), 3.0, 10),
    (Vec2::new(-57.5, 28.5), 1.8, 6),
];

/// The stones of a heap of rubble round `c`: spread out to `spread` on a sunflower's spiral,
/// bigger toward its middle.
fn heap(c: Vec2, spread: f32, count: usize) -> impl Iterator<Item = (Vec2, f32)> {
    (0..count).map(move |k| {
        let u = (k as f32 + 0.5) / count as f32;
        let turn = k as f32 * 2.399_963;
        let size = 0.22 + 0.42 * (1.0 - u) * (0.6 + 0.4 * (k as f32 * 0.618_034).fract());
        (c + Vec2::new(turn.cos(), turn.sin()) * spread * u.sqrt(), size)
    })
}

/// The square's middle (the well), and the merchant on its south side, facing north over it.
const WELL: Vec2 = Vec2::new(0.5, -31.5);
const SHOP: Vec2 = Vec2::new(-5.5, -36.5);

/// Where the party (re)spawns: round the square.
const SPAWNS: &[Vec2] = &[
    Vec2::new(-6.5, -29.5),
    Vec2::new(6.5, -29.5),
    Vec2::new(-6.5, -33.5),
    Vec2::new(6.5, -33.5),
    Vec2::new(-3.5, -26.5),
    Vec2::new(3.5, -26.5),
    Vec2::new(3.5, -36.5),
    Vec2::new(0.5, -28.5),
];

/// Torches: the square's corners, the bridge's foot, the gate, the court and the ramp's top,
/// the barbican's end of the high bridge, the chapel's stair, the broken bridge. None up on the
/// walks or the watchtower's top: a post there only stands in the way of a jump along them.
const TORCHES: &[Vec2] = &[
    Vec2::new(-10.5, -38.5), Vec2::new(10.5, -38.5), Vec2::new(10.5, -25.5),
    Vec2::new(-3.5, -15.5), Vec2::new(3.5, -15.5),
    Vec2::new(-2.5, 7.5), Vec2::new(2.5, 7.5),
    Vec2::new(-10.5, 20.5), Vec2::new(10.5, 20.5),
    Vec2::new(-3.5, 35.5), Vec2::new(3.5, 35.5),
    Vec2::new(-48.5, 23.5), Vec2::new(-48.5, 28.5), Vec2::new(-54.5, 29.5), Vec2::new(-52.5, 44.5),
    Vec2::new(-20.5, -42.5),
    Vec2::new(21.5, -15.5),
];

/// Where monsters come out: the woods round the village, the far bank's ends, the orchard's
/// end, and the barbican.
const LAIRS: &[Vec2] = &[
    Vec2::new(-50.5, -44.5),
    Vec2::new(50.5, -44.5),
    Vec2::new(-55.5, -18.5),
    Vec2::new(55.5, -18.5),
    Vec2::new(55.5, 4.5),
    Vec2::new(50.5, 46.5),
    Vec2::new(-55.5, 40.5),
];

/// Where chests may turn up: up high and out of the way.
const CHESTS: &[Vec2] = &[
    Vec2::new(37.5, -40.5),
    Vec2::new(-35.5, -40.5),
    Vec2::new(-34.5, 15.5),
    Vec2::new(-54.5, 20.5),
    Vec2::new(-15.5, 48.5),
    Vec2::new(30.5, 22.5),
    Vec2::new(-29.5, 47.5),
    Vec2::new(24.5, -6.5),
    Vec2::new(46.5, 22.5),
    Vec2::new(-45.5, -34.5),
    Vec2::new(16.5, 50.5),
];

const PICKUPS: &[(Vec2, PickupKind)] = &[
    (Vec2::new(-12.5, -31.5), PickupKind::Heal),
    (Vec2::new(8.5, 15.5), PickupKind::Heal),
    (Vec2::new(-53.5, 30.5), PickupKind::Heal),
    (Vec2::new(-28.5, -2.5), PickupKind::Haste),
    (Vec2::new(46.5, -30.5), PickupKind::Haste),
    (Vec2::new(0.5, 38.5), PickupKind::Fury),
];

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Hold, SIZE, edge, beyond);
    map.relief = relief;
    map.wades = true;
    for &(lo, hi) in PAVED {
        area(&mut map, lo, hi, Tile::Path);
    }
    // The river, its ford, and the bridges over it; the ravine, and the high bridge over it.
    area(&mut map, (-70.0, RIVER.0), (70.0, RIVER.1), Tile::Water);
    area(&mut map, (FORD.0, RIVER.0), (FORD.1, RIVER.1), Tile::Ford);
    area(&mut map, (-3.0, RIVER.0), (3.0, RIVER.1), Tile::Bridge);
    area(&mut map, (22.0, RIVER.0), (26.0, -10.0), Tile::Bridge);
    area(&mut map, (22.0, -7.0), (26.0, RIVER.1), Tile::Bridge);
    map.paint(|c, tile| if in_ravine(c) { Tile::Chasm } else { tile });
    area(&mut map, (-48.0, 24.0), (-36.0, 28.0), Tile::Bridge);
    for &(lo, size, door, storeys) in HOUSES {
        map.house(lo, size, door, storeys);
    }
    for &(lo, size, base, top, roof) in TOWERS {
        map.tower(lo, size, base, top, roof);
    }
    for &(t, high) in CRATES {
        map.crates(t, high);
    }
    for &p in TREES {
        map.set(p, Tile::Tree);
    }
    for x in ORCHARD.0 {
        for y in ORCHARD.1 {
            map.set(Vec2::new(x, y), Tile::Tree);
        }
    }
    for &p in ROCKS {
        map.set(p, Tile::Rock);
    }
    for &(a, b) in RUINS {
        map.line(a, b, Tile::Wall);
    }
    map.fences.extend(FENCES);
    map.gardens.extend(GARDENS);
    map.leaps.extend_from_slice(LEAPS);
    map.rubble.extend(RUBBLE.iter().flat_map(|&(c, spread, count)| heap(c, spread, count)));
    map.falls = Some(Falls { lip: CLIFF_FOOT, river: RIVER, lookout: Vec2::new(CLIFF_FOOT - 2.0, RIVER.1 + 2.2) });
    map.torches.extend(TORCHES);
    map.spawn_points.extend_from_slice(SPAWNS);
    map.pickups.extend_from_slice(PICKUPS);
    // The well in the square's middle.
    map.set(WELL, Tile::Rock);
    map.well = Some(WELL);
    // The merchant's stall on the square's south side, facing north over it: a counter behind
    // them with a torch at each end.
    map.shop = Some(SHOP);
    map.shop_faces = Vec2::Y;
    let counter = SHOP - Vec2::Y;
    map.line(counter - Vec2::X, counter + Vec2::X, Tile::Wall);
    map.counter = [-1.0, 0.0, 1.0].map(|x| Map::tile_of(counter + Vec2::X * x)).to_vec();
    map.torches.extend([counter - Vec2::X * 2.0, counter + Vec2::X * 2.0]);
    // Lairs and chests on walkable ground, wherever the layout left them.
    let walkable_near = |map: &Map, p: Vec2| map.nearest_walkable(Map::tile_of(p), 4).map(Map::center).expect("open ground near a lair or chest");
    map.lairs = LAIRS.iter().map(|&p| walkable_near(&map, p)).collect();
    map.chests = CHESTS.iter().map(|&p| walkable_near(&map, p)).collect();
    map
}
