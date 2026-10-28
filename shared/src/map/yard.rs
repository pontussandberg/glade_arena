//! Practice Yard: a long field of grass with open meadow all around (no forest to fill the
//! view). Two target dummies stand on a paved square near the west end from the start, with the
//! merchant's stall at its north side; the field runs on far to the east past a ruined wall and
//! a few rocks to lob over, and more dummies out along it, for long shots. And high ground to
//! fight from and at (`relief`): a plateau with sheer sides and a ramp up its south face, a low
//! scaffold deck with its own ramp, and a broad hill, each with a dummy on top. For trying a
//! hero's kit, aim and movement out, not for fighting over.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, squircle};

const SIZE: IVec2 = IVec2::new(128, 44);

/// Meters inside the field's edge (negative outside): a rounded rectangle, no wobble.
fn edge(p: Vec2) -> f32 {
    (1.0 - squircle(Vec2::new(p.x / 60.5, p.y / 18.5))) * 20.0
}

fn beyond(_: Vec2) -> Tile {
    Tile::Forest
}

/// Where the paving (and the near dummies) are: toward the west end, so most of the field lies
/// east of it.
const YARD: Vec2 = Vec2::new(-31.5, 0.0);
/// How far the paving reaches from `YARD`, each way.
const PAVING: f32 = 5.0;

/// Spawn points along the west side; each is mirrored onto the east one, so a fighter coming in
/// on the west has the dummies in front of them, and one on the east the whole field.
const WEST_SPAWNS: &[Vec2] = &[Vec2::new(-46.5, -6.5), Vec2::new(-46.5, -2.5), Vec2::new(-46.5, 1.5), Vec2::new(-46.5, 5.5)];

/// The two dummies on the paving, two far out along the field, and one on each high place (the
/// plateau's top, the scaffold's deck, the hill's crown).
const DUMMIES: &[Vec2] = &[
    Vec2::new(-31.0, 2.5),
    Vec2::new(-31.0, -2.5),
    Vec2::new(10.5, 0.5),
    Vec2::new(40.5, 0.5),
    Vec2::new(PLATEAU.x + 0.5, PLATEAU.y + 0.5),
    Vec2::new(SCAFFOLD.x + 0.5, SCAFFOLD.y + 0.5),
    Vec2::new(HILL.x + 0.5, HILL.y + 0.5),
];

/// The high ground: where the plateau stands (its middle) and how high and wide it is, with the
/// ramp up its south face; the scaffold deck likewise, its ramp from the east; the hill's crown,
/// height and reach.
const PLATEAU: Vec2 = Vec2::new(-18.0, 9.0);
const PLATEAU_TOP: f32 = 4.0;
const PLATEAU_RADIUS: f32 = 6.0;
const SCAFFOLD: Vec2 = Vec2::new(-3.0, -12.0);
const SCAFFOLD_TOP: f32 = 2.4;
const SCAFFOLD_HALF: f32 = 2.0;
const HILL: Vec2 = Vec2::new(26.0, 0.0);
const HILL_TOP: f32 = 5.0;
const HILL_REACH: f32 = 15.0;
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

/// How high the yard's ground is at `p`: flat but for the plateau, the scaffold and the hill.
fn relief(p: Vec2) -> f32 {
    // The plateau: its top inside its radius, sheer down just outside; a ramp up to it from the
    // south, rising over its run and then level onto the top.
    let plateau = PLATEAU_TOP * (1.0 - ((p.distance(PLATEAU) - PLATEAU_RADIUS) / SHEER).clamp(0.0, 1.0));
    let ramp_foot = PLATEAU.y - PLATEAU_RADIUS - PLATEAU_TOP * RAMP_RUN;
    let ramp = if (p.x - PLATEAU.x).abs() <= RAMP_HALF && p.y < PLATEAU.y { PLATEAU_TOP * ((p.y - ramp_foot) / (PLATEAU_TOP * RAMP_RUN)).clamp(0.0, 1.0) } else { 0.0 };
    // The scaffold: a square deck, sheer-sided, its ramp running up from the east.
    let out = (p - SCAFFOLD).abs().max_element();
    let deck = SCAFFOLD_TOP * (1.0 - ((out - SCAFFOLD_HALF) / SHEER).clamp(0.0, 1.0));
    let deck_foot = SCAFFOLD.x + SCAFFOLD_HALF + SCAFFOLD_TOP * RAMP_RUN;
    let deck_ramp = if (p.y - SCAFFOLD.y).abs() <= RAMP_HALF && p.x > SCAFFOLD.x { SCAFFOLD_TOP * ((deck_foot - p.x) / (SCAFFOLD_TOP * RAMP_RUN)).clamp(0.0, 1.0) } else { 0.0 };
    // The hill: a broad dome, walked up any way.
    let hill = HILL_TOP * smooth(1.0 - p.distance(HILL) / HILL_REACH);
    plateau.max(ramp).max(deck).max(deck_ramp).max(hill)
}

const SHOP: Vec2 = Vec2::new(-31.5, 14.5);

/// A stretch of ruined wall across the field east of the paving, and rocks beyond it: things
/// to lob over (`Tile::top`), with room to walk around.
const WALL: (Vec2, Vec2) = (Vec2::new(-9.5, -3.5), Vec2::new(-9.5, 3.5));
const ROCKS: &[Vec2] = &[Vec2::new(0.5, -5.5), Vec2::new(1.5, -4.5), Vec2::new(0.5, 5.5), Vec2::new(-0.5, 6.5)];

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Yard, SIZE, edge, beyond);
    map.relief = relief;
    map.paint(|c, tile| if (c.x - YARD.x).abs() < PAVING && (c.y - YARD.y).abs() < PAVING { Tile::Path } else { tile });
    map.mirror_spots(WEST_SPAWNS, &[], &[]);
    map.dummies.extend(DUMMIES);
    map.line(WALL.0, WALL.1, Tile::Wall);
    for rock in ROCKS {
        map.line(*rock, *rock, Tile::Rock);
    }
    // The merchant's stall at the north end, facing the yard: a counter behind them with a torch
    // at each end, the only light out here.
    map.shop = Some(SHOP);
    let counter = SHOP + Vec2::Y;
    map.line(counter - Vec2::X, counter + Vec2::X, Tile::Wall);
    map.counter = [-1.0, 0.0, 1.0].map(|x| Map::tile_of(counter + Vec2::X * x)).to_vec();
    map.torches.extend([counter - Vec2::X * 2.0, counter + Vec2::X * 2.0]);
    map
}
