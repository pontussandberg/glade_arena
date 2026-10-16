//! Every item's icon: a little pixel picture of it (a copper coil crackling, a heart with embers
//! in its cracks, a keg with its fuse lit), drawn here as rows of characters, each standing for a
//! color in that icon's own palette ('.' is clear, 'k' its outline), and made into an image once,
//! at startup, drawn with crisp pixels.

use std::collections::HashMap;

use arena_shared::items::Item;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// How many pixels an icon is, each way.
pub(crate) const ICON_PIXELS: usize = 13;

/// Every item's icon.
#[derive(Resource)]
pub(crate) struct ItemIcons(HashMap<Item, Handle<Image>>);

impl ItemIcons {
    pub(crate) fn of(&self, item: Item) -> Handle<Image> {
        self.0[&item].clone()
    }
}

pub(crate) fn load_item_icons(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let icons = Item::ALL.into_iter().map(|item| (item, images.add(paint(art(item))))).collect();
    commands.insert_resource(ItemIcons(icons));
}

/// The outline every icon is drawn round with.
const OUTLINE: u32 = 0x1C1412;

/// An icon's picture and its palette: each character in the rows, the color it is (as 0xRRGGBB).
type Art = (&'static [&'static str], &'static [(char, u32)]);

/// `art` as an image.
fn paint((rows, colors): Art) -> Image {
    let mut pixels = Vec::with_capacity(ICON_PIXELS * ICON_PIXELS * 4);
    for row in rows {
        for c in row.chars() {
            let rgba = match c {
                '.' => [0, 0, 0, 0],
                'k' => rgb(OUTLINE),
                _ => colors.iter().find(|(key, _)| *key == c).map_or([255, 0, 255, 255], |(_, color)| rgb(*color)),
            };
            pixels.extend(rgba);
        }
    }
    let size = Extent3d { width: ICON_PIXELS as u32, height: ICON_PIXELS as u32, depth_or_array_layers: 1 };
    let mut image = Image::new(size, TextureDimension::D2, pixels, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    // Crisp pixels, not blurred.
    image.sampler = ImageSampler::nearest();
    image
}

fn rgb(color: u32) -> [u8; 4] {
    [(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]
}

/// What `item` looks like.
fn art(item: Item) -> Art {
    match item {
        // A copper coil, a bolt of lightning crackling down through it.
        Item::StormCoil => (
            &[
                ".....by......",
                "....byy......",
                "..kkkyykkkk..",
                ".kCCcyycccck.",
                "..kkkyykkkk..",
                ".kCCccyyccck.",
                "..kkkkyykkk..",
                ".kCCcccyycck.",
                "..kkkkkyykk..",
                ".......yyb...",
                "......yyb....",
                "......yb.....",
                ".....b.......",
            ],
            &[('c', 0xB8692E), ('C', 0xF0A868), ('y', 0xFFF4B0), ('b', 0x8CC8FF)],
        ),
        // A heart, still beating, embers glowing in its cracks.
        Item::EmberHeart => (
            &[
                ".............",
                "..kkk...kkk..",
                ".kRrRk.kRRRk.",
                "kRryRRkRRoRRk",
                "kRrRRRRRoRRRk",
                "kRRRRRRoRRRRk",
                "kRRRRRyRRRRRk",
                ".kRRRRoRRRRk.",
                "..kRRoRRRRk..",
                "...kRoRRRk...",
                "....kRRRk....",
                ".....kRk.....",
                "......k......",
            ],
            &[('R', 0xB02A22), ('r', 0xE8705A), ('o', 0xFF9A30), ('y', 0xFFE070)],
        ),
        // A sigil of frost: a snowflake, ice at its heart.
        Item::RimeSigil => (
            &[
                "......w......",
                "...w..w..w...",
                "....w.w.w....",
                ".....dwd.....",
                ".w..dbwbd..w.",
                "..w.bbwbb.w..",
                "wwwwwwwwwwwww",
                "..w.bbwbb.w..",
                ".w..dbwbd..w.",
                ".....dwd.....",
                "....w.w.w....",
                "...w..w..w...",
                "......w......",
            ],
            &[('w', 0xF0FAFF), ('b', 0x8CC8F0), ('d', 0x4A7AAA)],
        ),
        // A fang, blood running off its point.
        Item::Bloodfang => (
            &[
                "..kkkkkkkkk..",
                ".kiiiiiiiIIk.",
                ".kiiiiiiiIk..",
                "..kiiiiiIIk..",
                "..kiiiiiIk...",
                "...kiiirIk...",
                "...kiirRk....",
                "....kirRk....",
                "....krRk.....",
                ".....kRk.....",
                ".....r.......",
                ".....R.......",
                ".....r.......",
            ],
            &[('i', 0xF2E8D0), ('I', 0xC0B090), ('R', 0xB0181E), ('r', 0xE03A3A)],
        ),
        // A ring of bramble, pale thorns sticking out all round.
        Item::BrambleMantle => (
            &[
                ".............",
                "....t...t....",
                "...kgggggk...",
                "..kgGkkkGgk.t",
                "t.kgk...kgk..",
                ".kgk.....kgk.",
                "tkgk.....kgkt",
                ".kgk.....kgk.",
                "..kgk...kgk..",
                "t.kgGkkkGgk..",
                "...kgggggk.t.",
                "....t...t....",
                ".............",
            ],
            &[('g', 0x4A6A2A), ('G', 0x7A9A3A), ('t', 0xE0D4A0)],
        ),
        // A stone, its echoes rippling out round it.
        Item::EchoStone => (
            &[
                ".............",
                "..p.......p..",
                ".p..ppppp..p.",
                ".p.p.....p.p.",
                "p.p..kkk..p.p",
                "p.p.kSSsk.p.p",
                "p.p.kSssk.p.p",
                "p.p.ksssk.p.p",
                "p.p..kkk..p.p",
                ".p.p.....p.p.",
                ".p..ppppp..p.",
                "..p.......p..",
                ".............",
            ],
            &[('s', 0x8A8478), ('S', 0xC8C2B4), ('p', 0x9CE0F0)],
        ),
        // A keg, hooped in iron, its fuse spitting sparks.
        Item::PowderKeg => (
            &[
                ".......y.o...",
                "......ky.....",
                ".....k.......",
                "...kkkkkkk...",
                "..kWwWwWwWk..",
                "..khhhhhhhk..",
                ".kWwWwWwWwWk.",
                ".kWwWwWwWwWk.",
                ".kWwWwWwWwWk.",
                "..khhhhhhhk..",
                "..kWwWwWwWk..",
                "...kkkkkkk...",
                ".............",
            ],
            &[('w', 0x7A4E28), ('W', 0xB07A44), ('h', 0x5C5C64), ('y', 0xFFE070), ('o', 0xFF8A30)],
        ),
        // A lantern, a wisp glowing inside it.
        Item::WispLantern => (
            &[
                "......k......",
                ".....k.k.....",
                "....kmmmk....",
                "...kmmmmmk...",
                "...km.G.mk...",
                "...k.GgG.k...",
                "...kGgggGk...",
                "...k.GgG.k...",
                "...km.G.mk...",
                "...kmmmmmk...",
                "....kmmmk....",
                ".....kkk.....",
                ".............",
            ],
            &[('m', 0x4A4A56), ('g', 0xE8FFF0), ('G', 0x7AE0A8)],
        ),
        // A boot, frost on its cuff and under its sole.
        Item::HoarfrostBoots => (
            &[
                ".............",
                "...kkkkk.....",
                "...kfffk.....",
                "...kLLLk.....",
                "...kLlLk.....",
                "...kLlLk.....",
                "...kLlLk.....",
                "...kLlLkkkk..",
                "..kLlllLLLLk.",
                "..kLllllllLk.",
                "..kkkkkkkkkk.",
                ".bfbfbfbfbfb.",
                "..f.f.f.f.f..",
            ],
            &[('l', 0x6A4A30), ('L', 0x8E6E4C), ('f', 0xE0F4FF), ('b', 0x8CC8F0)],
        ),
        // A bell of black iron, the void glowing under its lip.
        Item::DoomBell => (
            &[
                "....kkkkk....",
                "....kk.kk....",
                "...kkVVVkk...",
                "..kVVvvvvvk..",
                "..kVvvvvvvk..",
                "..kVvvvvvvk..",
                "..kVvvvvvvk..",
                ".kVvvvvvvvvk.",
                ".kVvvvvvvvvk.",
                "kVvvvvvvvvvvk",
                "kkkkkkkkkkkkk",
                ".....pkp.....",
                "....p.p.p....",
            ],
            &[('v', 0x3A2A4A), ('V', 0x7A5A9A), ('p', 0xB070FF)],
        ),
        // A gold coin, stamped.
        Item::MidasCoin => (
            &[
                ".............",
                "....kkkkk....",
                "..kkYYYYykk..",
                "..kYyyyyyok..",
                ".kYyyoooyyok.",
                ".kYyoyYyyyok.",
                ".kYyyooYyyok.",
                ".kYyyyyoyyok.",
                ".kYyoooyyyok.",
                "..kYyyyyyok..",
                "..kkyyyyokk..",
                "....kkkkk....",
                ".............",
            ],
            &[('y', 0xE8B830), ('Y', 0xFFE68A), ('o', 0xA87818)],
        ),
        // A crown of gold, lightning striking it.
        Item::ThunderCrown => (
            &[
                "......bb.....",
                ".....bb......",
                "......bb.....",
                ".k...kbk...k.",
                "kYk.kYyYk.kYk",
                "kyYkyyyyykYyk",
                "kyyyyyyyyyyyk",
                "kyyRyyByyRyyk",
                "kyyyyyyyyyyyk",
                "koooooooooook",
                ".kkkkkkkkkkk.",
                ".............",
                ".............",
            ],
            &[('y', 0xE8B830), ('Y', 0xFFE68A), ('o', 0xA87818), ('R', 0xD03030), ('B', 0x5AA0FF), ('b', 0xDCEEFF)],
        ),
        // An orb, the dark of the void swirling in it.
        Item::VoidOrb => (
            &[
                ".............",
                "....kkkkk....",
                "..kkpppppkk..",
                "..kpPpdppPk..",
                ".kpPdddddppk.",
                ".kpdddPdddpk.",
                ".kppddPPddpk.",
                ".kpddddddPpk.",
                ".kppddddpPpk.",
                "..kpPpppPpk..",
                "..kkpppppkk..",
                "....kkkkk....",
                ".............",
            ],
            &[('d', 0x1A0E26), ('p', 0x7A40C0), ('P', 0xC090FF)],
        ),
        // A shard of mirror, light along its edge.
        Item::MirrorShard => (
            &[
                "........k....",
                ".......kSk...",
                "......kSsk...",
                "......kSsck..",
                ".....kSssck..",
                ".....kSscck..",
                "....kSsscck..",
                "....kSsccck..",
                "...kSssccck..",
                "...kSscccck..",
                "..kSsscccck..",
                "..kkkkkkkkk..",
                ".............",
            ],
            &[('s', 0xC8D8E8), ('S', 0xFFFFFF), ('c', 0x7AA8C8)],
        ),
        // A totem's carved face, war paint on it, eyes burning.
        Item::BerserkerTotem => (
            &[
                "...kkkkkkk...",
                "..kWwWwWwWk..",
                "..kwrwwwrwk..",
                "..kweWwWewk..",
                "..kwwwwwwwk..",
                "..kwrrrrrwk..",
                "..kwrWWWrwk..",
                "..kkkkkkkkk..",
                "....kwWwk....",
                "....kwWwk....",
                "....kwWwk....",
                "....kwWwk....",
                "....kkkkk....",
            ],
            &[('w', 0x7A4E28), ('W', 0xB07A44), ('r', 0xC82A22), ('e', 0xFFD25A)],
        ),
        // A shot bouncing off the ground, on its way to the next.
        Item::RicochetCharm => (
            &[
                ".............",
                "a.........AAA",
                ".a.........AA",
                "..a.......a.A",
                "...a.....a...",
                "....a...a....",
                ".....a.a.....",
                "......a......",
                ".............",
                "ddddddddddddd",
                "dDdDdDdDdDdDd",
                ".............",
                ".............",
            ],
            &[('a', 0xE8D8B0), ('A', 0xFFFFFF), ('d', 0x8A8070), ('D', 0x5A5048)],
        ),
        // Javelins, three at once, fanning out.
        Item::HydraSpear => (
            &[
                "S.....S.....S",
                "Ss...sSs...sS",
                ".ss..sSs..ss.",
                "..ss.sSs.ss..",
                "...w..w..w...",
                "....w.w.w....",
                ".....www.....",
                "......w......",
                "......w......",
                "......w......",
                "......w......",
                "......w......",
                "......w......",
            ],
            &[('s', 0xA8B0BC), ('S', 0xFFFFFF), ('w', 0xB07A44)],
        ),
        // The ground torn open, the dead's light rising out of it.
        Item::GravecallerRift => (
            &[
                "...g.....p...",
                "....p...g....",
                "..g..p.p..g..",
                ".....g.p.....",
                "...p..g..p...",
                ".............",
                "EEEEEk.kEEEEE",
                "eeeekpgpkeeee",
                "eeekpgggpkeee",
                "eeekpgggpkeee",
                "eeeekpgpkeeee",
                "eeeeekgkeeeee",
                "eeeeeekeeeeee",
            ],
            &[('e', 0x2A2018), ('E', 0x5A7A34), ('p', 0x9A5CF0), ('g', 0x9AE880)],
        ),
        // A heart of ice, a snowflake caught in it.
        Item::HeartOfWinter => (
            &[
                ".............",
                "..kkk...kkk..",
                ".kBBbk.kbbbk.",
                "kBBbbbkbbbbbk",
                "kBbbbwbwbbbbk",
                "kbbbbbwbbbbbk",
                "kbbbbwbwbbbbk",
                ".kbbbbbbbbdk.",
                "..kbbbbbbdk..",
                "...kbbbbdk...",
                "....kbbdk....",
                ".....kdk.....",
                "......k......",
            ],
            &[('b', 0x8CC8F0), ('B', 0xE0F4FF), ('w', 0xFFFFFF), ('d', 0x4A7AAA)],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_square_and_in_its_palette() {
        for item in Item::ALL {
            let (rows, colors) = art(item);
            assert_eq!(rows.len(), ICON_PIXELS, "{item:?}: {} rows", rows.len());
            for row in rows {
                assert_eq!(row.chars().count(), ICON_PIXELS, "{item:?}: {row:?}");
                for c in row.chars() {
                    assert!(c == '.' || c == 'k' || colors.iter().any(|(key, _)| *key == c), "{item:?}: '{c}' isn't in its palette");
                }
            }
        }
    }
}
