//! Practice Yard: a long, flat field of grass with open meadow all around (no forest to fill
//! the view). Two target dummies stand on a paved square near the west end from the start, with
//! the merchant's stall at its north side; the field runs on far to the east past a ruined wall
//! and a few rocks to lob over, and two more dummies out along it, for long shots. For trying
//! a hero's kit, aim and movement out, not for fighting over.

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

/// The two dummies on the paving, and two far out along the field.
const DUMMIES: &[Vec2] = &[Vec2::new(-31.0, 2.5), Vec2::new(-31.0, -2.5), Vec2::new(10.5, 0.5), Vec2::new(40.5, 0.5)];

const SHOP: Vec2 = Vec2::new(-31.5, 14.5);

/// A stretch of ruined wall across the field east of the paving, and rocks beyond it: things
/// to lob over (`Tile::top`), with room to walk around.
const WALL: (Vec2, Vec2) = (Vec2::new(-9.5, -3.5), Vec2::new(-9.5, 3.5));
const ROCKS: &[Vec2] = &[Vec2::new(0.5, -5.5), Vec2::new(1.5, -4.5), Vec2::new(0.5, 5.5), Vec2::new(-0.5, 6.5)];

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Yard, SIZE, edge, beyond);
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
