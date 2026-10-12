//! The minimap in the bottom-right corner: our room's map drawn tile by tile from the shared map
//! (so it's exactly what blocks movement), every fighter as a dot in its ring color, every
//! pickup as its icon while it's lying there or the seconds until it's back, every waygate end
//! as a ring (dim while it's closed), and a frame showing what the camera sees. Drawn again when
//! the map changes.

use arena_shared::config::TICK_HZ;
use arena_shared::map::Map;
use arena_shared::protocol::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::Me;
use crate::camera::CameraPlaced;
use crate::feedback::AttackClock;
use crate::arena::{self, palette};
use crate::render::{Relation, ground_at, shown, ui_text};
use crate::rooms::CurrentMap;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_minimap);
        app.add_systems(
            Update,
            (
                draw_map.run_if(resource_changed::<CurrentMap>),
                (spawn_dots, color_dots).chain(),
                place_dots,
                (spawn_markers, update_markers).chain(),
                (spawn_gate_marks, update_gate_marks).chain(),
                frame_view.in_set(CameraPlaced),
            ),
        );
    }
}

/// The most room the minimap's picture takes, in screen pixels: a map is drawn as big as fits.
const ROOM: Vec2 = Vec2::new(384.0, 264.0);
/// The gap from the screen corner, a dot's size, a pickup marker's (and its text's), and a
/// waygate end's ring.
const MARGIN: f32 = 12.0;
const DOT: f32 = 12.0;
const MARKER: f32 = 22.0;
const MARKER_TEXT: f32 = 14.0;
const GATE_MARK: f32 = 14.0;

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

/// A pickup's marker (this entity, a round chip) and its text, and what it shows now: the
/// pickup's icon (`None`: it's lying there), else the seconds until it's back.
#[derive(Component)]
struct PickupMarker {
    pickup: Entity,
    text: Entity,
    showing: Option<u32>,
}

/// Screen pixels per tile for `map`: as many as fit the minimap's room.
fn scale(map: &Map) -> f32 {
    (ROOM / map.size.as_vec2()).min_element()
}

/// Where a gameplay position is on `map`'s minimap, in whole pixels from its top-left corner (so
/// a dot or the view frame only moves, and the UI is only laid out again, when it moves a pixel).
fn on_minimap(map: &Map, p: Vec2) -> Vec2 {
    let half = map.half_extents();
    (Vec2::new(p.x + half.x, half.y - p.y) * scale(map)).round()
}

/// The minimap's size in pixels.
fn picture_size(map: &Map) -> Vec2 {
    (map.size.as_vec2() * scale(map)).round()
}

/// `map` drawn a pixel per tile, in the scene's colors.
fn picture(map: &Map) -> Image {
    let size = map.size.as_uvec2();
    let mut pixels = vec![0; (size.x * size.y * 4) as usize];
    let corner = -map.size / 2;
    for (tile, kind) in map.tiles() {
        // The map's +y is up; the picture's rows go down.
        let (column, row) = (tile.x - corner.x, map.size.y - 1 - (tile.y - corner.y));
        let at = ((row * map.size.x + column) * 4) as usize;
        pixels[at..at + 4].copy_from_slice(&arena::tile_color(kind).to_srgba().to_u8_array());
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
    image
}

/// The minimap, empty: `draw_map` puts the map in it.
fn spawn_minimap(mut commands: Commands) {
    commands
        .spawn((
            Minimap,
            crate::render::GameUi,
            Node {
                position_type: PositionType::Absolute,
                right: px(MARGIN),
                bottom: px(MARGIN),
                border: UiRect::all(px(2.0)),
                ..default()
            },
            BorderColor::all(palette::ui::PANEL),
            ImageNode::default(),
        ))
        .with_child((
            ViewFrame,
            Node { position_type: PositionType::Absolute, border: UiRect::all(px(1.0)), ..default() },
            BorderColor::all(palette::ui::TEXT),
        ));
}

/// The map, drawn in the minimap at the size it takes there.
fn draw_map(current: Res<CurrentMap>, mut images: ResMut<Assets<Image>>, minimap: Single<(&mut Node, &mut ImageNode), With<Minimap>>) {
    let map = current.map();
    let (mut node, mut image) = minimap.into_inner();
    let size = picture_size(map);
    node.width = px(size.x);
    node.height = px(size.y);
    image.image = images.add(picture(map));
}

/// Puts a mark `size` across centered on `at` on `map`'s minimap, writing only what changed (so
/// the UI is only laid out again when it moves a pixel). Every frame: the map can change after a
/// mark arrives.
fn place(node: &mut Node, map: &Map, at: Vec2, size: f32) {
    let at = on_minimap(map, at) - Vec2::splat(size / 2.0);
    let (left, top) = (px(at.x), px(at.y));
    if node.left != left || node.top != top {
        node.left = left;
        node.top = top;
    }
}

/// Each fighter gets a dot once it has a body.
fn spawn_dots(
    mut commands: Commands,
    minimap: Single<Entity, With<Minimap>>,
    new: Query<(Entity, Has<Me>), (With<PlayerId>, Added<Mesh3d>)>,
) {
    for (player, is_me) in &new {
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
                BackgroundColor(Relation::of(is_me, false).color()),
                BorderColor::all(palette::ui::PANEL),
            ))
            .id();
        commands.entity(*minimap).add_child(dot);
    }
}

/// A dot is in its fighter's `Relation` color: ours, an ally's or an enemy's.
fn color_dots(
    players: Query<&Relation>,
    changed: Query<(), Changed<Relation>>,
    mut dots: Query<(Ref<Dot>, &mut BackgroundColor)>,
) {
    for (dot, mut fill) in &mut dots {
        if !(changed.contains(dot.player) || dot.is_added()) {
            continue;
        }
        if let Ok(relation) = players.get(dot.player) {
            fill.set_if_neq(BackgroundColor(relation.color()));
        }
    }
}

/// Keep dots on their fighters (only writing what changed); the dead's are hidden, the gone's
/// removed.
fn place_dots(
    mut commands: Commands,
    current: Res<CurrentMap>,
    players: Query<(&Pos, Option<&Health>)>,
    mut dots: Query<(Entity, &Dot, &mut Node, &mut Visibility)>,
) {
    for (dot, owner, mut node, mut visibility) in &mut dots {
        let Ok((pos, health)) = players.get(owner.player) else {
            commands.entity(dot).despawn();
            continue;
        };
        visibility.set_if_neq(shown(health.is_none_or(Health::alive)));
        place(&mut node, current.map(), pos.0, DOT);
    }
}

/// Each pickup gets a marker on its spot once it arrives. Above the fighters' dots, so a fighter
/// standing on it doesn't hide its timer.
fn spawn_markers(mut commands: Commands, minimap: Single<Entity, With<Minimap>>, new: Query<(Entity, &Pickup), Added<Pickup>>) {
    for (pickup, info) in &new {
        let color = crate::pickups::color(info.kind);
        // Spawned showing its icon, as lying there; `update_markers` turns it into a timer.
        let text = commands.spawn(ui_text(crate::pickups::icon(info.kind), MARKER_TEXT, palette::ui::PANEL)).id();
        let marker = commands
            .spawn((
                PickupMarker { pickup, text, showing: None },
                Node {
                    position_type: PositionType::Absolute,
                    width: px(MARKER),
                    height: px(MARKER),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::MAX,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(color),
                BorderColor::all(color),
                ZIndex(1),
            ))
            .add_child(text)
            .id();
        commands.entity(*minimap).add_child(marker);
    }
}

/// A pickup lying there shows its icon ("+" a heal, ">>" a haste) in its color; a taken one, the
/// whole seconds until it's back (rounded up), dimmed. Only writes what changed.
fn update_markers(
    mut commands: Commands,
    clock: AttackClock,
    current: Res<CurrentMap>,
    pickups: Query<&Pickup>,
    mut markers: Query<(Entity, &mut PickupMarker, &mut Node, &mut BackgroundColor)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let now = clock.now(true);
    for (entity, mut marker, mut node, mut background) in &mut markers {
        let Ok(pickup) = pickups.get(marker.pickup) else {
            commands.entity(entity).despawn();
            continue;
        };
        place(&mut node, current.map(), pickup.at, MARKER);
        let wanted = pickup.back_at.map(|back_at| ((back_at as f32 - now) / TICK_HZ as f32).ceil().max(1.0) as u32);
        if marker.showing == wanted {
            continue;
        }
        marker.showing = wanted;
        let (label, text_color, fill) = match wanted {
            Some(seconds) => (seconds.to_string(), palette::ui::TEXT, palette::ui::PANEL),
            None => (crate::pickups::icon(pickup.kind).to_string(), palette::ui::PANEL, crate::pickups::color(pickup.kind)),
        };
        background.set_if_neq(BackgroundColor(fill));
        if let Ok((mut text, mut current)) = texts.get_mut(marker.text) {
            text.0 = label;
            current.set_if_neq(TextColor(text_color));
        }
    }
}

/// A waygate's end on the minimap: a ring, lit while the gate is open.
#[derive(Component)]
struct GateMark {
    gate: Entity,
    end: usize,
}

/// Each waygate gets a ring on both its ends once it arrives, under the fighters' dots.
fn spawn_gate_marks(mut commands: Commands, minimap: Single<Entity, With<Minimap>>, new: Query<Entity, Added<Waygate>>) {
    for gate in &new {
        for end in 0..2 {
            let mark = commands
                .spawn((
                    GateMark { gate, end },
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(GATE_MARK),
                        height: px(GATE_MARK),
                        border: UiRect::all(px(3.0)),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BorderColor::all(palette::SPIRIT),
                    ZIndex(-1),
                ))
                .id();
            commands.entity(*minimap).add_child(mark);
        }
    }
}

/// A gate's rings are lit while it's open and dim while it's closed; gone with it.
fn update_gate_marks(
    mut commands: Commands,
    clock: AttackClock,
    current: Res<CurrentMap>,
    gates: Query<&Waygate>,
    mut marks: Query<(Entity, &GateMark, &mut Node, &mut BorderColor)>,
) {
    let now = clock.now(true);
    for (entity, mark, mut node, mut border) in &mut marks {
        let Ok(gate) = gates.get(mark.gate) else {
            commands.entity(entity).despawn();
            continue;
        };
        place(&mut node, current.map(), gate.ends[mark.end], GATE_MARK);
        let open = gate.closed_for(now) == 0.0;
        let color = if open { palette::SPIRIT } else { palette::SPIRIT.with_alpha(0.3) };
        border.set_if_neq(BorderColor::all(color));
    }
}

/// Frame the ground the camera sees: where the screen's corners hit the ground, boxed.
fn frame_view(
    current: Res<CurrentMap>,
    camera: Single<(&Camera, &GlobalTransform), Changed<GlobalTransform>>,
    mut frame: Single<&mut Node, With<ViewFrame>>,
) {
    let map = current.map();
    let (camera, transform) = *camera;
    let Some(size) = camera.logical_viewport_size() else { return };
    let corners = [Vec2::ZERO, Vec2::new(size.x, 0.0), size, Vec2::new(0.0, size.y)];
    let (mut min, mut max) = (Vec2::MAX, Vec2::MIN);
    for corner in corners {
        let Some(ground) = ground_at(camera, transform, corner) else { return };
        let at = on_minimap(map, ground);
        (min, max) = (min.min(at), max.max(at));
    }
    let picture = picture_size(map);
    let (min, max) = (min.clamp(Vec2::ZERO, picture), max.clamp(Vec2::ZERO, picture));
    let wanted = [px(min.x), px(min.y), px(max.x - min.x), px(max.y - min.y)];
    if [frame.left, frame.top, frame.width, frame.height] != wanted {
        [frame.left, frame.top, frame.width, frame.height] = wanted;
    }
}
