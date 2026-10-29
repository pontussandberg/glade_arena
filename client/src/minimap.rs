//! The minimap in the bottom-right corner: our room's map drawn tile by tile from the shared map
//! (so it's exactly what blocks movement), every fighter as a dot in its ring color, every
//! pickup as its icon while it's lying there or the seconds until it's back, every waygate end
//! as a ring (dim while it's closed), where we're walking to as a small ring in our color, and
//! a frame showing what the camera sees. Drawn again when
//! the map changes.

use arena_shared::config::TICK_HZ;
use arena_shared::map::Map;
use arena_shared::protocol::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::Me;
use crate::camera::{CameraMode, CameraPlaced};
use crate::feedback::AttackClock;
use crate::arena::{self, palette};
use crate::render::{HudButton, Relation, button_fill, clicked, ground_at, key_chip, pixel_art, shown, ui_text};
use crate::tooltip::{Side, hover_shows, tip, tip_text, tip_title};
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
                (click_lock, show_lock).chain(),
                (left_fullscreen, click_fullscreen, show_fullscreen).chain(),
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
/// A monster's (a zombie's) dot: smaller, so players stand out.
const MONSTER_DOT: f32 = 7.0;
/// The frame round the picture.
const FRAME: f32 = 2.0;
const MARKER: f32 = 22.0;
const MARKER_TEXT: f32 = 14.0;
const GATE_MARK: f32 = 14.0;

/// The minimap's picture; dots and the view frame are its children. A right click on it walks
/// there (`render::read_local_input`).
#[derive(Component)]
pub(crate) struct Minimap;

/// A fighter's dot, `size` across (a monster's smaller).
#[derive(Component)]
struct Dot {
    player: Entity,
    size: f32,
}

/// What the camera sees.
#[derive(Component)]
struct ViewFrame;

/// Where we're walking to (`DesiredInput::move_to`), shown with the ground's marker
/// (`render::input::show_destination`).
#[derive(Component)]
pub(crate) struct DestinationDot;

/// The destination's ring's size.
const DESTINATION: f32 = 9.0;

/// The camera's lock, just left of the minimap's bottom-left corner: clicking it (or Y) locks the
/// camera on your fighter or unlocks it (`CameraMode::unlocked`). Its padlock is drawn in `art`.
#[derive(Component)]
struct CameraLock {
    art: Entity,
}

/// The lock's size (as big as the passive's icon) and its gap from the minimap.
const LOCK: f32 = 46.0;
const LOCK_GAP: f32 = 8.0;
/// The padlock, old-school pixel art: shut, and open (its shackle swung up off the right).
const PADLOCK: [&[&str]; 2] = [
    &["..###..", ".#...#.", ".#...#.", "#######", "###.###", "###.###", "#######"],
    &["..###..", ".#...#.", ".#.....", "#######", "###.###", "###.###", "#######"],
];
const PADLOCK_PIXEL: f32 = 3.0;

/// The full screen button, just left of the camera's lock: clicking it fills the screen with the
/// game, or gives it back. Its picture is drawn in `art`.
#[derive(Component)]
struct FullscreenButton {
    art: Entity,
}

/// Its picture, old-school pixel art: four corners pointing out (to fill the screen) or in (to
/// leave it).
const FULLSCREEN_ART: [&[&str]; 2] = [
    &["###.###", "#.....#", "#.....#", ".......", "#.....#", "#.....#", "###.###"],
    &[".#...#.", ".#...#.", "##...##", ".......", "##...##", ".#...#.", ".#...#."],
];

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

/// Where on `map` the screen point `at` (physical pixels) is, if it's on the minimap's picture
/// (`node`, at `transform`): `on_minimap` the other way.
pub(crate) fn ground_on_minimap(node: &ComputedNode, transform: UiGlobalTransform, map: &Map, at: Vec2) -> Option<Vec2> {
    let from_corner = (node.normalize_point(transform, at)? + 0.5) * node.size() - node.border().min_inset;
    let on_picture = from_corner * node.inverse_scale_factor;
    if on_picture.cmplt(Vec2::ZERO).any() || on_picture.cmpgt(picture_size(map)).any() {
        return None;
    }
    let (half, tiles) = (map.half_extents(), on_picture / scale(map));
    Some(Vec2::new(tiles.x - half.x, half.y - tiles.y))
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
        pixels[at..at + 4].copy_from_slice(&arena::tile_color(arena::Theme::of(map.id), kind).to_srgba().to_u8_array());
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
                border: UiRect::all(px(FRAME)),
                ..default()
            },
            BorderColor::all(palette::ui::PANEL),
            ImageNode::default(),
        ))
        .with_child((
            ViewFrame,
            Node { position_type: PositionType::Absolute, border: UiRect::all(px(1.0)), ..default() },
            BorderColor::all(palette::ui::TEXT),
        ))
        .with_child((
            DestinationDot,
            Node {
                position_type: PositionType::Absolute,
                width: px(DESTINATION),
                height: px(DESTINATION),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BorderColor::all(palette::YOU),
            Visibility::Hidden,
        ))
        .with_children(|minimap| {
            let tip = minimap
                .spawn(tip(Side::Above, 0.0, 220.0))
                .with_children(|tip| {
                    tip.spawn(tip_title("Camera"));
                    tip.spawn(tip_text("Locked, it follows you. Unlocked, it stays put: move the cursor to the screen's edges to look around."));
                })
                .id();
            let art = minimap.spawn(Node { width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).id();
            let full_art = minimap.spawn(Node { width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).id();
            minimap
                .spawn((
                    FullscreenButton { art: full_art },
                    HudButton,
                    button_fill(palette::ui::PANEL),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(-2.0 * (LOCK + LOCK_GAP)),
                        bottom: px(0.0),
                        width: px(LOCK),
                        height: px(LOCK),
                        border: UiRect::all(px(2.0)),
                        ..default()
                    },
                    BorderColor::all(palette::ui::MUTED),
                ))
                .add_child(full_art);
            minimap
                .spawn((
                    CameraLock { art },
                    HudButton,
                    button_fill(palette::ui::PANEL),
                    hover_shows(tip),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(-(LOCK + LOCK_GAP)),
                        bottom: px(0.0),
                        width: px(LOCK),
                        height: px(LOCK),
                        border: UiRect::all(px(2.0)),
                        ..default()
                    },
                    BorderColor::all(palette::ui::MUTED),
                ))
                .add_children(&[art, tip])
                .with_child((
                    Node { position_type: PositionType::Absolute, left: px(2.0), top: px(1.0), ..default() },
                    children![key_chip("Y", 9.0, palette::ui::TEXT, Color::NONE)],
                ));
        });
}

/// A click on the full screen button fills the screen with the game, or gives it back.
fn click_fullscreen(button: Single<Ref<Interaction>, With<FullscreenButton>>, mut window: Single<&mut Window, With<bevy::window::PrimaryWindow>>) {
    if clicked(button.into_inner()) {
        window.mode = match window.mode {
            bevy::window::WindowMode::Windowed => bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current),
            _ => bevy::window::WindowMode::Windowed,
        };
    }
}

/// Full screen left by the browser itself (Esc, its own controls) isn't told to the window: once
/// the page that was full screen no longer is, the window is windowed again. (Going full screen
/// takes the browser a moment, so a page not full screen yet is left be.)
fn left_fullscreen(mut window: Single<&mut Window, With<bevy::window::PrimaryWindow>>, mut was_full: Local<bool>) {
    #[cfg(target_family = "wasm")]
    {
        let full = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|document| js_sys::Reflect::get(&document, &"fullscreenElement".into()).ok())
            .is_some_and(|element| !element.is_null() && !element.is_undefined());
        if *was_full && !full && window.mode != bevy::window::WindowMode::Windowed {
            window.mode = bevy::window::WindowMode::Windowed;
        }
        *was_full = full;
    }
    #[cfg(not(target_family = "wasm"))]
    let _ = (&mut window, &mut was_full);
}

/// The full screen button shows which way a click goes: out to full screen, or back in.
fn show_fullscreen(
    mut commands: Commands,
    window: Single<Ref<Window>, With<bevy::window::PrimaryWindow>>,
    button: Single<Ref<FullscreenButton>>,
) {
    if !window.is_changed() && !button.is_added() {
        return;
    }
    let full = window.mode != bevy::window::WindowMode::Windowed;
    commands.entity(button.art).despawn_children().with_children(|art| pixel_art(art, FULLSCREEN_ART[full as usize], PADLOCK_PIXEL, palette::ui::TEXT));
}

/// A click on the lock locks or unlocks the camera.
fn click_lock(lock: Single<Ref<Interaction>, With<CameraLock>>, mut mode: ResMut<CameraMode>) {
    if clicked(lock.into_inner()) {
        mode.unlocked = !mode.unlocked;
    }
}

/// The lock shows the camera's: shut and plain while it follows you, open and lit while it's
/// unlocked.
fn show_lock(mut commands: Commands, mode: Res<CameraMode>, lock: Single<(Ref<CameraLock>, &mut BorderColor)>) {
    let (lock, mut border) = lock.into_inner();
    if !mode.is_changed() && !lock.is_added() {
        return;
    }
    let color = if mode.unlocked { palette::ui::ACCENT } else { palette::ui::TEXT };
    border.set_if_neq(BorderColor::all(if mode.unlocked { palette::ui::ACCENT } else { palette::ui::MUTED }));
    commands.entity(lock.art).despawn_children().with_children(|art| pixel_art(art, PADLOCK[mode.unlocked as usize], PADLOCK_PIXEL, color));
}

/// Puts the destination's ring on `at`, on `map`'s minimap.
pub(crate) fn place_destination(node: &mut Node, map: &Map, at: Vec2) {
    place(node, map, at, DESTINATION);
}

/// The map, drawn in the minimap at the size it takes there: the node is that and its frame (a
/// node's size takes its border in), so the picture is drawn at exactly `picture_size`, as dots,
/// marks and clicks (`ground_on_minimap`) count on.
fn draw_map(current: Res<CurrentMap>, mut images: ResMut<Assets<Image>>, minimap: Single<(&mut Node, &mut ImageNode), With<Minimap>>) {
    let map = current.map();
    let (mut node, mut image) = minimap.into_inner();
    let size = picture_size(map);
    node.width = px(size.x + 2.0 * FRAME);
    node.height = px(size.y + 2.0 * FRAME);
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

/// `place` for a mark that's always on the move (a fighter's dot), by its `UiTransform`, which
/// Bevy applies after laying the UI out: a crowd moving doesn't make it lay everything out again
/// every frame. On whole pixels, as `place`'s are.
fn place_moving(transform: &mut Mut<UiTransform>, map: &Map, at: Vec2, size: f32) {
    let at = (on_minimap(map, at) - Vec2::splat(size / 2.0)).round();
    transform.set_if_neq(UiTransform { translation: Val2::px(at.x, at.y), ..default() });
}

/// A dot in its own color whoever its fighter is to us (the merchant's gold).
#[derive(Component)]
struct KeepsColor;

/// Each fighter gets a dot once it has a body.
fn spawn_dots(
    mut commands: Commands,
    minimap: Single<Entity, With<Minimap>>,
    new: Query<(Entity, &ClassId, Has<Me>, Has<Shopkeeper>), (With<PlayerId>, Added<Mesh3d>)>,
) {
    for (player, class, is_me, shopkeeper) in &new {
        // The merchant and bosses stand out: bigger, the merchant in gold.
        let boss = class.def().is_boss();
        let size = match () {
            _ if shopkeeper || boss => DOT * 1.6,
            _ if class.def().playable => DOT,
            _ => MONSTER_DOT,
        };
        let fill = if shopkeeper { palette::GOLD } else { Relation::of(is_me, false).color() };
        let dot = commands
            .spawn((
                Dot { player, size },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    width: px(size),
                    height: px(size),
                    border: UiRect::all(px(1.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(fill),
                BorderColor::all(palette::ui::PANEL),
            ))
            .id();
        if shopkeeper {
            commands.entity(dot).insert(KeepsColor);
        }
        commands.entity(*minimap).add_child(dot);
    }
}

/// A dot is in its fighter's `Relation` color: ours, an ally's, an enemy's or a monster's.
fn color_dots(
    players: Query<&Relation>,
    changed: Query<(), Changed<Relation>>,
    mut dots: Query<(Ref<Dot>, &mut BackgroundColor), Without<KeepsColor>>,
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
    mut dots: Query<(Entity, &Dot, &mut UiTransform, &mut Visibility)>,
) {
    for (dot, owner, mut transform, mut visibility) in &mut dots {
        let Ok((pos, health)) = players.get(owner.player) else {
            commands.entity(dot).despawn();
            continue;
        };
        visibility.set_if_neq(shown(health.is_none_or(Health::alive)));
        place_moving(&mut transform, current.map(), pos.0, owner.size);
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
    camera: Single<(&Camera, &GlobalTransform), (With<crate::camera::CameraRig>, Changed<GlobalTransform>)>,
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
