//! The minimap in the bottom-right corner: our room's map drawn tile by tile from the shared map
//! (so it's exactly what blocks movement), every fighter as a dot in its ring color, every
//! pickup as its icon while it's lying there or the seconds until it's back, every waygate end
//! as a ring (dim while it's closed), where we're walking to as a small ring in our color, and
//! which way we look, as a cone out from our dot. Drawn again when the map changes.
//!
//! In the corner it shows the ground round us, zoomed in, following us (`MinimapView`); M opens
//! the whole map big in the middle of the screen, and closes it again.

use arena_shared::config::TICK_HZ;
use arena_shared::map::Map;
use arena_shared::protocol::*;
use bevy::prelude::*;

use crate::Me;
use crate::camera::CameraPlaced;
use crate::feedback::AttackClock;
use crate::frames::{Frame, OpenFrame};
use crate::arena::{self, palette};
use crate::render::{HudButton, Relation, button_fill, clicked, crisp_image, pixel_art, ui_text};
use crate::rooms::CurrentMap;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MinimapView>();
        app.add_systems(Startup, spawn_minimap);
        app.add_systems(
            Update,
            (
                draw_map.run_if(resource_changed::<CurrentMap>),
                (frame_minimap, (draw_dots, point_cone)).chain().in_set(CameraPlaced),
                (spawn_markers, update_markers).chain().after(frame_minimap),
                (spawn_gate_marks, update_gate_marks).chain().after(frame_minimap),
                (left_fullscreen, click_fullscreen, show_fullscreen).chain(),
            ),
        );
    }
}

/// The most room the minimap's picture takes in the corner, in screen pixels (as big as the
/// whole map fits), and how much closer than that whole map it shows the ground round us there.
const ROOM: Vec2 = Vec2::new(384.0, 264.0);
const ZOOM: f32 = 2.5;
/// How much of the screen the whole map (M) takes, at most.
const FULL_ROOM: f32 = 0.85;
/// The cone showing which way we look: how big its picture is (screen pixels), and how wide it
/// opens (radians either side of straight ahead).
const CONE: f32 = 64.0;
const CONE_HALF: f32 = 0.6;

/// What of the map the minimap shows, and how big: in the corner, the ground round us, zoomed
/// in; or (M) the whole of it, big, in the middle of the screen. Worked out every frame
/// (`frame_minimap`); dots, marks and clicks (`ground_on_minimap`) go by it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct MinimapView {
    /// Screen pixels per tile.
    scale: f32,
    /// The top-left corner of what it shows, in tiles from the map's top-left corner.
    corner: Vec2,
    /// How big it shows it, in screen pixels.
    size: Vec2,
    /// The whole map, big (M).
    full: bool,
}

impl MinimapView {
    /// Where a gameplay position is on the minimap, in whole pixels from its top-left corner (so
    /// a dot or the view frame only moves, and the UI is only laid out again, when it moves a
    /// pixel).
    fn on_minimap(&self, map: &Map, p: Vec2) -> Vec2 {
        let half = map.half_extents();
        ((Vec2::new(p.x + half.x, half.y - p.y) - self.corner) * self.scale).round()
    }

    /// Whether a point on the minimap (`on_minimap`) is on what it shows.
    fn shows(&self, at: Vec2) -> bool {
        at.cmpge(Vec2::ZERO).all() && at.cmple(self.size).all()
    }
}
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

/// The minimap's picture; dots and marks are its children. A click on it is the HUD's, not the
/// field's (`render::read_local_input`).
#[derive(Component)]
pub(crate) struct Minimap;

/// Which way we look: a cone out from our dot.
#[derive(Component)]
struct LookCone;

/// Where we're walking to (`DesiredInput::move_to`), shown with the ground's marker
/// (`render::input::show_destination`).
#[derive(Component)]
pub(crate) struct DestinationDot;

/// The destination's ring's size.
const DESTINATION: f32 = 9.0;

/// The full screen button's size (as big as the passive's icon), its gap from the minimap, and
/// how big a pixel of its picture is.
const BUTTON: f32 = 46.0;
const BUTTON_GAP: f32 = 8.0;
const ART_PIXEL: f32 = 3.0;

/// The full screen button, just left of the minimap's bottom-left corner: clicking it fills the
/// screen with the game, or gives it back. Its picture is drawn in `art`.
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

/// Where on `map` the screen point `at` (physical pixels) is, if it's on the minimap's picture
/// (`node`, at `transform`, showing `view`): `MinimapView::on_minimap` the other way.
pub(crate) fn ground_on_minimap(node: &ComputedNode, transform: UiGlobalTransform, map: &Map, view: &MinimapView, at: Vec2) -> Option<Vec2> {
    let from_corner = (node.normalize_point(transform, at)? + 0.5) * node.size() - node.border().min_inset;
    let on_picture = from_corner * node.inverse_scale_factor;
    if !view.shows(on_picture) {
        return None;
    }
    let (half, tiles) = (map.half_extents(), on_picture / view.scale + view.corner);
    Some(Vec2::new(tiles.x - half.x, half.y - tiles.y))
}

/// The cone's picture: pointing up from its middle, fading out toward its rim and its sides.
fn cone_picture() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let side = 64u32;
    let mut pixels = vec![0; (side * side * 4) as usize];
    let middle = Vec2::splat(side as f32 / 2.0);
    for y in 0..side {
        for x in 0..side {
            let out = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - middle;
            let (reach, off) = (out.length() / middle.x, Vec2::NEG_Y.angle_to(out).abs());
            let inside = (1.0 - reach).clamp(0.0, 1.0) * ((CONE_HALF - off) / 0.15).clamp(0.0, 1.0);
            let at = ((y * side + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[255, 255, 255, (inside * 150.0) as u8]);
        }
    }
    let size = Extent3d { width: side, height: side, depth_or_array_layers: 1 };
    Image::new(size, TextureDimension::D2, pixels, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
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
    crisp_image(size.x, size.y, pixels)
}

/// The minimap, empty: `draw_map` puts the map in it.
fn spawn_minimap(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let cone = images.add(cone_picture());
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
            LookCone,
            Node { position_type: PositionType::Absolute, width: px(CONE), height: px(CONE), ..default() },
            ImageNode { image: cone, color: palette::YOU.with_alpha(0.9), ..default() },
            UiTransform::default(),
            Visibility::Hidden,
            ZIndex(-1),
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
            let full_art = minimap.spawn(Node { width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).id();
            minimap
                .spawn((
                    FullscreenButton { art: full_art },
                    HudButton,
                    button_fill(palette::ui::PANEL),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(-(BUTTON + BUTTON_GAP)),
                        bottom: px(0.0),
                        width: px(BUTTON),
                        height: px(BUTTON),
                        border: UiRect::all(px(2.0)),
                        ..default()
                    },
                    BorderColor::all(palette::ui::MUTED),
                ))
                .add_child(full_art);
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

/// The full screen button shows which way a click goes: out to full screen, or back in. Drawn
/// anew only when that changes (the window is changed for much else, every frame).
fn show_fullscreen(
    mut commands: Commands,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    button: Single<Ref<FullscreenButton>>,
    mut shown: Local<Option<bool>>,
) {
    let full = window.mode != bevy::window::WindowMode::Windowed;
    if *shown == Some(full) && !button.is_added() {
        return;
    }
    *shown = Some(full);
    commands.entity(button.art).despawn_children().with_children(|art| pixel_art(art, FULLSCREEN_ART[full as usize], ART_PIXEL, palette::ui::TEXT));
}

/// Puts the destination's ring on `at`, on `map`'s minimap showing `view`.
pub(crate) fn place_destination(node: &mut Node, map: &Map, view: &MinimapView, at: Vec2) {
    place(node, map, view, at, DESTINATION);
}

/// The map, drawn into the minimap (what of it it shows, and how big, is `frame_minimap`'s).
fn draw_map(current: Res<CurrentMap>, mut images: ResMut<Assets<Image>>, mut image: Single<&mut ImageNode, With<Minimap>>) {
    image.image = images.add(picture(current.map()));
}

/// What the minimap shows (`MinimapView`), every frame: the ground round us in the corner, or
/// (M, again to close it) the whole map big in the middle of the screen. The node is the picture
/// and its frame (a node's size takes its border in), so the picture is drawn at exactly the
/// view's size, as dots, marks and clicks count on; the picture shows just the tiles in view.
#[allow(clippy::too_many_arguments)]
fn frame_minimap(
    frame: Res<OpenFrame>,
    current: Res<CurrentMap>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    me: Query<&Pos, (With<Me>, With<PlayerId>)>,
    mut view: ResMut<MinimapView>,
    minimap: Single<(&mut Node, &mut ImageNode), With<Minimap>>,
) {
    let map = current.map();
    let tiles = map.size.as_vec2();
    // M opens and shuts it through the frames (`frames::route_keys`).
    let full = frame.is(Frame::Map);
    let screen = Vec2::new(window.width(), window.height());
    let wanted = if full {
        let scale = (screen * FULL_ROOM / tiles).min_element();
        MinimapView { scale, corner: Vec2::ZERO, size: (tiles * scale).round(), full }
    } else {
        let fit = (ROOM / tiles).min_element();
        let (size, scale) = ((tiles * fit).round(), fit * ZOOM);
        let shown = size / scale;
        let half = map.half_extents();
        let middle = me.single().map_or(tiles / 2.0, |pos| Vec2::new(pos.0.x + half.x, half.y - pos.0.y));
        // On whole screen pixels, so the picture and the dots on it step together.
        let corner = ((middle - shown / 2.0).clamp(Vec2::ZERO, (tiles - shown).max(Vec2::ZERO)) * scale).round() / scale;
        MinimapView { scale, corner, size, full }
    };
    view.set_if_neq(wanted);
    let (mut node, mut image) = minimap.into_inner();
    let outer = wanted.size + 2.0 * FRAME;
    let (left, top, right, bottom) = if full {
        let at = ((screen - outer) / 2.0).round();
        (px(at.x), px(at.y), Val::Auto, Val::Auto)
    } else {
        (Val::Auto, Val::Auto, px(MARGIN), px(MARGIN))
    };
    if [node.width, node.height, node.left, node.top, node.right, node.bottom] != [px(outer.x), px(outer.y), left, top, right, bottom] {
        [node.width, node.height, node.left, node.top, node.right, node.bottom] = [px(outer.x), px(outer.y), left, top, right, bottom];
    }
    let rect = Some(Rect::from_corners(wanted.corner, wanted.corner + wanted.size / wanted.scale));
    if image.rect != rect {
        image.rect = rect;
    }
}

/// Which way we look: the cone out from our dot, turned the way the camera faces. Hidden while
/// we have no fighter, or off what the minimap shows.
fn point_cone(
    current: Res<CurrentMap>,
    view: Res<MinimapView>,
    camera: Single<&GlobalTransform, With<crate::camera::CameraRig>>,
    me: Query<&Pos, (With<Me>, With<PlayerId>)>,
    cone: Single<(&mut Node, &mut UiTransform, &mut Visibility), With<LookCone>>,
) {
    let (mut node, mut turn, mut visibility) = cone.into_inner();
    let Ok(pos) = me.single() else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    place(&mut node, current.map(), &view, pos.0, CONE);
    // The camera's facing on the ground; up the picture is north, and the cone turns clockwise.
    let ahead = camera.forward();
    let rotation = Rot2::radians(ahead.x.atan2(-ahead.z));
    if turn.rotation != rotation {
        turn.rotation = rotation;
    }
}

/// Puts a mark `size` across centered on `at` on `map`'s minimap showing `view` (hidden while
/// that's off what it shows), writing only what changed (so the UI is only laid out again when it
/// moves a pixel). Every frame: the map can change after a mark arrives.
fn place(node: &mut Node, map: &Map, view: &MinimapView, at: Vec2, size: f32) {
    let middle = view.on_minimap(map, at);
    let display = if view.shows(middle) { Display::Flex } else { Display::None };
    let at = middle - Vec2::splat(size / 2.0);
    let (left, top) = (px(at.x), px(at.y));
    if node.left != left || node.top != top || node.display != display {
        node.left = left;
        node.top = top;
        node.display = display;
    }
}

/// Every living fighter with a body as a dot on the minimap, round, in its `Relation` color
/// (ours, an ally's, an enemy's or a monster's; the merchant's gold), ringed: on whole pixels, as
/// `place`'s are, ours over the rest. Drawn as round rectangles (`Rects`) just over the minimap's
/// picture, under its marks: a crowd's dots as nodes made Bevy go over every one of them every
/// frame.
#[allow(clippy::type_complexity)]
fn draw_dots(
    current: Res<CurrentMap>,
    view: Res<MinimapView>,
    minimap: Single<(Entity, &ComputedNode, &UiGlobalTransform, &InheritedVisibility), With<Minimap>>,
    fighters: Query<(&Pos, &ClassId, Option<&Health>, Option<&Relation>, Has<Me>, Has<Shopkeeper>), (With<PlayerId>, With<Mesh3d>)>,
    mut rects: ResMut<crate::rects::Rects>,
) {
    let (minimap, node, transform, visible) = *minimap;
    if !visible.get() {
        return;
    }
    rects.over(minimap);
    let scale = node.inverse_scale_factor.recip();
    let corner = transform.translation - node.size() / 2.0 + node.border().min_inset;
    let ring = scale.round().max(1.0);
    let mut dot = |at: Vec2, size: f32, fill: Color| {
        let middle = view.on_minimap(current.map(), at);
        if !view.shows(middle) {
            return;
        }
        let from = corner + (middle - Vec2::splat(size / 2.0)).round() * scale;
        let to = from + Vec2::splat(size * scale);
        rects.round(from.into(), to.into(), size * scale, palette::ui::PANEL);
        rects.round((from + ring).into(), (to - ring).into(), size * scale, fill);
    };
    let mut me = None;
    for (pos, class, health, relation, is_me, shopkeeper) in &fighters {
        if !health.is_none_or(Health::alive) {
            continue;
        }
        // The merchant and bosses stand out: bigger, the merchant in gold.
        let size = match () {
            _ if shopkeeper || class.def().is_boss() => DOT * 1.6,
            _ if class.def().playable => DOT,
            _ => MONSTER_DOT,
        };
        let fill = if shopkeeper { palette::GOLD } else { relation.copied().unwrap_or(Relation::of(is_me, false)).color() };
        if is_me {
            me = Some((pos.0, size, fill));
        } else {
            dot(pos.0, size, fill);
        }
    }
    if let Some((at, size, fill)) = me {
        dot(at, size, fill);
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
    view: Res<MinimapView>,
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
        place(&mut node, current.map(), &view, pickup.at, MARKER);
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
    view: Res<MinimapView>,
    gates: Query<&Waygate>,
    mut marks: Query<(Entity, &GateMark, &mut Node, &mut BorderColor)>,
) {
    let now = clock.now(true);
    for (entity, mark, mut node, mut border) in &mut marks {
        let Ok(gate) = gates.get(mark.gate) else {
            commands.entity(entity).despawn();
            continue;
        };
        place(&mut node, current.map(), &view, gate.ends[mark.end], GATE_MARK);
        let open = gate.closed_for(now) == 0.0;
        let color = if open { palette::SPIRIT } else { palette::SPIRIT.with_alpha(0.3) };
        border.set_if_neq(BorderColor::all(color));
    }
}
