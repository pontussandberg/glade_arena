//! The minimap in the bottom-right corner: the clearing drawn tile by tile from the shared map
//! (so it's exactly what blocks movement), every fighter as a dot in its ring color, and a frame
//! showing what the camera sees.

use arena_shared::map::{MAP_HALF_EXTENTS, MAP_TILES, Tile, map};
use arena_shared::protocol::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use lightyear::prelude::*;

use crate::camera::CameraPlaced;
use crate::glade::{self, palette};
use crate::render::{ground_at, player_color, shown};

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_minimap);
        app.add_systems(Update, (spawn_dots, place_dots, frame_view.in_set(CameraPlaced)));
    }
}

/// Screen pixels per map tile, the gap from the screen corner, and a dot's size.
const SCALE: f32 = 3.0;
const MARGIN: f32 = 12.0;
const DOT: f32 = 8.0;

/// The minimap's picture; dots and the view frame are its children.
#[derive(Component)]
struct Minimap;

/// A fighter's dot.
#[derive(Component)]
struct Dot {
    player: Entity,
}

/// What the camera sees.
#[derive(Component)]
struct ViewFrame;

/// Where a gameplay position is on the minimap, in whole pixels from its top-left corner (so a
/// dot or the view frame only moves, and the UI is only laid out again, when it moves a pixel).
fn on_minimap(p: Vec2) -> Vec2 {
    (Vec2::new(p.x + MAP_HALF_EXTENTS.x, MAP_HALF_EXTENTS.y - p.y) * SCALE).round()
}

/// The minimap's size in pixels.
fn picture_size() -> Vec2 {
    MAP_TILES.as_vec2() * SCALE
}

fn spawn_minimap(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = MAP_TILES.as_uvec2();
    // Deep forest everywhere the map doesn't list a tile.
    let forest = glade::tile_color(Tile::Forest).to_srgba().to_u8_array();
    let mut pixels = forest.repeat((size.x * size.y) as usize);
    for (tile, kind) in map().tiles() {
        // The map's +y is up; the picture's rows go down.
        let row = MAP_TILES.y - 1 - tile.y;
        let at = ((row * MAP_TILES.x + tile.x) * 4) as usize;
        pixels[at..at + 4].copy_from_slice(&glade::tile_color(kind).to_srgba().to_u8_array());
    }
    let mut image = Image::new(
        Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Crisp tiles, not blurred.
    image.sampler = ImageSampler::nearest();
    let picture = picture_size();
    commands
        .spawn((
            Minimap,
            crate::render::GameUi,
            Node {
                position_type: PositionType::Absolute,
                right: px(MARGIN),
                bottom: px(MARGIN),
                width: px(picture.x),
                height: px(picture.y),
                border: UiRect::all(px(2.0)),
                ..default()
            },
            BorderColor::all(palette::INK),
            ImageNode::new(images.add(image)),
        ))
        .with_child((
            ViewFrame,
            Node { position_type: PositionType::Absolute, border: UiRect::all(px(1.0)), ..default() },
            BorderColor::all(palette::HAZE),
        ));
}

/// Each fighter gets a dot once it has a body.
fn spawn_dots(
    mut commands: Commands,
    minimap: Single<Entity, With<Minimap>>,
    new: Query<(Entity, &PlayerId, Has<Predicted>), Added<Mesh3d>>,
) {
    for (player, id, is_me) in &new {
        let dot = commands
            .spawn((
                Dot { player },
                Node {
                    position_type: PositionType::Absolute,
                    width: px(DOT),
                    height: px(DOT),
                    border: UiRect::all(px(1.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(player_color(id.0, is_me)),
                BorderColor::all(palette::INK),
            ))
            .id();
        commands.entity(*minimap).add_child(dot);
    }
}

/// Keep dots on their fighters (only writing what changed); the dead's are hidden, the gone's
/// removed.
fn place_dots(
    mut commands: Commands,
    players: Query<(&Pos, Option<&Health>)>,
    mut dots: Query<(Entity, &Dot, &mut Node, &mut Visibility)>,
) {
    for (dot, owner, mut node, mut visibility) in &mut dots {
        let Ok((pos, health)) = players.get(owner.player) else {
            commands.entity(dot).despawn();
            continue;
        };
        visibility.set_if_neq(shown(health.is_none_or(Health::alive)));
        let at = on_minimap(pos.0) - Vec2::splat(DOT / 2.0);
        let (left, top) = (px(at.x), px(at.y));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
    }
}

/// Frame the ground the camera sees: where the screen's corners hit the ground, boxed.
fn frame_view(
    camera: Single<(&Camera, &GlobalTransform), Changed<GlobalTransform>>,
    mut frame: Single<&mut Node, With<ViewFrame>>,
) {
    let (camera, transform) = *camera;
    let Some(size) = camera.logical_viewport_size() else { return };
    let corners = [Vec2::ZERO, Vec2::new(size.x, 0.0), size, Vec2::new(0.0, size.y)];
    let (mut min, mut max) = (Vec2::MAX, Vec2::MIN);
    for corner in corners {
        let Some(ground) = ground_at(camera, transform, corner) else { return };
        let at = on_minimap(ground);
        (min, max) = (min.min(at), max.max(at));
    }
    let (min, max) = (min.clamp(Vec2::ZERO, picture_size()), max.clamp(Vec2::ZERO, picture_size()));
    let wanted = [px(min.x), px(min.y), px(max.x - min.x), px(max.y - min.y)];
    if [frame.left, frame.top, frame.width, frame.height] != wanted {
        [frame.left, frame.top, frame.width, frame.height] = wanted;
    }
}
