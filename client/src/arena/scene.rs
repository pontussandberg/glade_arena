//! The map's scene: ground, terrain, water and chasms, the dais, crags and props, the sun and
//! torches, and the plugin that builds it.

use super::*;

use arena_shared::map::{Map, Tile};
use bevy::color::Mix;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::platform::collections::HashMap;
use super::palette::*;

use crate::rooms::CurrentMap;
use crate::sculpt::bevel_box;

/// A tile's color on the minimap: the palette of the scene, flattened.
pub fn tile_color(theme: &Theme, tile: Tile) -> Color {
    match tile {
        Tile::Grass => theme.grass,
        Tile::Path => theme.path,
        Tile::Marsh => theme.marsh,
        Tile::Ford => theme.riverbed.mix(&theme.pond, 0.6),
        Tile::Bridge => theme.stone,
        Tile::Planks => theme.bark.mix(&theme.path, 0.3),
        Tile::Water => theme.pond,
        Tile::Chasm => theme.abyss.mix(&INK, 0.3),
        Tile::Wall => theme.wall,
        Tile::Rock => theme.slate,
        Tile::Tree => theme.pine,
        Tile::Forest => match theme.wilds {
            Wilds::Forest => theme.pine.mix(&INK, 0.4),
            Wilds::Fen => theme.pond.mix(&INK, 0.45),
            Wilds::Highlands => theme.wall.mix(&INK, 0.35),
        },
    }
}

/// Haze that fades the forest ring into the sky color; goes on the camera, and takes the map's
/// color in `light_scene`.
pub fn haze() -> DistanceFog {
    DistanceFog { color: HAZE, falloff: FogFalloff::Linear { start: 40.0, end: 95.0 }, ..default() }
}

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(HAZE));
        app.insert_resource(GlobalAmbientLight { color: SKY, brightness: 160.0, ..default() });
        app.add_systems(Startup, spawn_sun);
        // The scene is built from our room's map, and built again when it changes.
        app.add_systems(Update, ((build_arena, light_scene).run_if(resource_exists_and_changed::<CurrentMap>), flicker_torches));
    }
}

const WATER_LEVEL: f32 = -0.3;
const RIVERBED_LEVEL: f32 = -0.65;
/// The bottom of a chasm.
const CHASM_FLOOR: f32 = -6.0;

/// The lobby's dais, on top of its crag.
pub const DAIS_RADIUS: f32 = 1.3;
pub const DAIS_HEIGHT: f32 = 0.16;

/// The crag the lobby's stage stands on, `height` meters over the land, in `theme`'s colors: a
/// pinnacle with the dais on its narrow grassy top (the top level at the origin), tufts and a
/// flower or two around it, and under it a core of rock widening down to the ground, its sides
/// broken up by boulders darkening as they go down. Narrow, so it hides little of the land
/// below and beyond. Vertex-colored, for one matte white material.
pub fn crag_mesh(theme: &'static Theme, height: f32) -> Mesh {
    let mut rng = Lcg(11);
    let mut props = Props::new(theme);
    let dais = faceted(Cylinder::new(DAIS_RADIUS, DAIS_HEIGHT).mesh().resolution(24).build());
    props.push(dais.transformed_by(Transform::from_xyz(0.0, DAIS_HEIGHT / 2.0, 0.0)), theme.stone.mix(&theme.wall, 0.4));
    let top = faceted(Cylinder::new(CRAG_TOP, 0.5).mesh().resolution(9).build());
    props.push(top.transformed_by(Transform::from_xyz(0.0, -0.25, 0.0)), theme.grass.mix(&theme.meadow, 0.3));
    let depth = height + 4.0;
    let core = faceted(ConicalFrustum { radius_top: CRAG_TOP - 0.1, radius_bottom: 4.5, height: depth }.mesh().resolution(8).build());
    props.push(core.transformed_by(Transform::from_xyz(0.0, -depth / 2.0 - 0.3, 0.0)), theme.slate.darker(0.12));
    let mut down = 0.9;
    while down < depth {
        let count = 7;
        for k in 0..count {
            let a = (k as f32 + rng.range(-0.3, 0.3)) / count as f32 * std::f32::consts::TAU + down;
            let at = Vec2::from_angle(a) * (1.7 + down * 0.15);
            let s = rng.range(1.0, 1.4) + down * 0.06;
            let color = theme.slate.mix(&theme.wall, rng.next()).darker(0.25 * down / depth);
            let t = Transform::from_translation(to_world(at, -down))
                .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
                .with_scale(Vec3::new(s, s * rng.range(0.7, 1.0), s));
            props.add(Shape::Rock, color, t);
        }
        down += 1.3;
    }
    for k in 0..7 {
        let a = k as f32 / 7.0 * std::f32::consts::TAU + rng.range(-0.3, 0.3);
        let at = Vec2::from_angle(a) * rng.range(DAIS_RADIUS + 0.2, CRAG_TOP - 0.15);
        if k % 3 == 0 {
            props.flower(&mut rng, at);
        } else {
            props.bog_reeds(&mut rng, at, theme.fern, theme.path.mix(&theme.fern, 0.5));
        }
    }
    props.into_mesh()
}

/// The crag's top's radius: a little past the dais.
const CRAG_TOP: f32 = DAIS_RADIUS + 0.6;

/// A torch flame; flickers its light.
#[derive(Component)]
struct Torch {
    pub(super) phase: f32,
}

pub(super) const TORCH_INTENSITY: f32 = 170_000.0;

fn flicker_torches(time: Res<Time>, mut torches: Query<(&Torch, &mut PointLight, &Children)>, mut flames: Query<&mut Transform>) {
    let t = time.elapsed_secs();
    for (torch, mut light, children) in &mut torches {
        let f = 1.0 + 0.12 * (t * 7.3 + torch.phase).sin() * (t * 12.1 + torch.phase * 1.7).sin();
        light.intensity = TORCH_INTENSITY * f;
        for child in children {
            if let Ok(mut flame) = flames.get_mut(*child) {
                flame.scale = Vec3::new(1.0, f * 1.15, 1.0);
            }
        }
    }
}

/// The low sun, from the same side over every map (its color and strength are the map's).
fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight { color: SUN, illuminance: 4_000.0, shadow_maps_enabled: true, ..default() },
        // Low dusk sun from the upper left, so shadows are long and fall toward the camera's right.
        Transform::from_xyz(-32.0, 14.0, -18.0).looking_at(Vec3::ZERO, Vec3::Y),
        sun_shadows(SUN_SHADOWS),
    ));
}

/// How far from the camera the sun casts shadows in the arena: enough to cover what it sees at
/// full zoom-out; the far forest needs none.
pub const SUN_SHADOWS: f32 = 50.0;

/// The sun's shadows, out to `reach` meters from the camera.
pub fn sun_shadows(reach: f32) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: reach, ..default() }.build()
}

/// The map's light and air: the sun's color and strength, the sky's fill, and the haze (and the
/// clear color behind it, the same).
fn light_scene(
    current: Res<CurrentMap>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut suns: Query<&mut DirectionalLight>,
    mut fogs: Query<&mut DistanceFog>,
) {
    let theme = Theme::of(current.0);
    clear.0 = theme.haze;
    ambient.color = theme.sky;
    ambient.brightness = theme.sky_brightness;
    for mut sun in &mut suns {
        sun.color = theme.sun;
        sun.illuminance = theme.sun_illuminance;
    }
    for mut fog in &mut fogs {
        fog.color = theme.haze;
    }
}

/// Everything built from the map: despawned when it changes, for the new map's.
#[derive(Component)]
struct ArenaScene;

fn build_arena(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    current: Res<CurrentMap>,
    old: Query<Entity, With<ArenaScene>>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let mut rng = Lcg(7);
    let m = current.map();
    let theme = Theme::of(current.0);

    let scene = SceneTiles::new(m);
    let vertex_colored = materials.add(matte(Color::WHITE));
    commands.spawn((ArenaScene, Mesh3d(meshes.add(terrain_mesh(m, &scene, theme, &mut rng))), MeshMaterial3d(vertex_colored.clone())));
    commands.spawn((ArenaScene, Mesh3d(meshes.add(ground_mesh(m, theme, &mut rng))), MeshMaterial3d(vertex_colored.clone())));
    if let Some(water) = water_mesh(&scene, theme.wilds == Wilds::Fen) {
        let material = StandardMaterial { perceptual_roughness: 0.18, reflectance: 0.6, ..matte(theme.pond) };
        commands.spawn((ArenaScene, Mesh3d(meshes.add(water)), MeshMaterial3d(materials.add(material))));
    }

    let mut props = Props::new(theme);
    let wall_tops = props.walls(m, &mut rng);
    props.stall(m);
    props.bridges(m, &mut rng);
    props.rope_bridges(m, &mut rng);
    props.fords(m, &mut rng);
    if theme.wilds == Wilds::Fen {
        props.flagstones(m, &mut rng);
    }
    for (t, tile) in m.tiles() {
        let c = Map::center(t);
        match tile {
            Tile::Rock if theme.wilds == Wilds::Highlands => props.sandstone(&mut rng, c),
            Tile::Rock => props.boulder(&mut rng, c),
            Tile::Grass if theme.wilds == Wilds::Fen => props.fen_grass(&mut rng, c),
            Tile::Grass => props.floor_detail(m, &mut rng, t, c),
            Tile::Path if rng.next() < 0.12 => props.pebbles(&mut rng, c),
            Tile::Tree => props.map_tree(&mut rng, c),
            Tile::Marsh => props.bog(&mut rng, c),
            _ => {}
        }
    }
    // The wilds around the clearing, kept out of water and chasms.
    let extent = scene.extent.as_vec2() - Vec2::splat(4.0);
    let open = |p: Vec2| [Vec2::ZERO, Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y].iter().all(|d| scene.get(Map::tile_of(p + *d * 1.5)) == Tile::Forest);
    let density = match theme.wilds {
        Wilds::Forest => 0.055,
        Wilds::Fen => 0.04,
        Wilds::Highlands => 0.02,
    };
    for _ in 0..(extent.x * extent.y * 4.0 * density) as usize {
        let p = Vec2::new(rng.range(-extent.x, extent.x), rng.range(-extent.y, extent.y));
        if m.edge_margin(p) > -1.2 || !open(p) {
            continue;
        }
        props.wild(&mut rng, p, wild_height(theme, m, p));
    }

    // Torches (posts go into the prop mesh; flames and lights don't).
    let flame = meshes.add(faceted(Cone::new(0.16, 0.42).mesh().resolution(5).build()));
    let flame_mat = materials.add(glow(TORCH_FLAME, 8.0));
    for (i, &p) in m.torches.iter().enumerate() {
        let base = wall_tops.get(&Map::tile_of(p)).copied().unwrap_or(0.0);
        let post_height = if base > 0.0 { 0.5 } else { 1.5 };
        props.block(Vec3::new(0.14, post_height, 0.14), theme.bark, Transform::from_translation(to_world(p, base + post_height / 2.0)));
        commands
            .spawn((
                ArenaScene,
                Torch { phase: i as f32 * 1.7 },
                PointLight { color: TORCH_LIGHT, intensity: TORCH_INTENSITY, range: 12.0, shadow_maps_enabled: false, ..default() },
                Transform::from_translation(to_world(p, base + post_height + 0.5)),
            ))
            .with_child((Mesh3d(flame.clone()), MeshMaterial3d(flame_mat.clone()), Transform::from_xyz(0.0, -0.3, 0.0)));
    }

    // All static props as one mesh: ~1,200 trees, stones and wall blocks draw as one object.
    commands.spawn((ArenaScene, Mesh3d(meshes.add(props.into_mesh())), MeshMaterial3d(vertex_colored)));
}

/// The land of the whole scene (`Map::scene_tile`) out to `extent` tiles each way from the
/// middle: the map and a ring of forest around it, out to where the haze hides everything.
/// Worked out once, as building the scene asks for each tile many times over.
struct SceneTiles {
    extent: IVec2,
    pub(super) tiles: Vec<Tile>,
}

impl SceneTiles {
    pub(super) fn new(m: &Map) -> SceneTiles {
        let extent = m.size / 2 + IVec2::new(30, 24);
        let tiles = (-extent.y..extent.y).flat_map(|y| (-extent.x..extent.x).map(move |x| m.scene_tile(IVec2::new(x, y)))).collect();
        SceneTiles { extent, tiles }
    }

    /// Forest past the scene's edge.
    pub(super) fn get(&self, t: IVec2) -> Tile {
        let u = t + self.extent;
        let width = 2 * self.extent.x;
        if u.x < 0 || u.y < 0 || u.x >= width || u.y >= 2 * self.extent.y {
            return Tile::Forest;
        }
        self.tiles[(u.y * width + u.x) as usize]
    }
}

/// The four directions, in tiles.
const SIDES: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

/// Water or a chasm: what bridges cross, and banks or cliffs drop into.
fn is_gap(tile: Tile) -> bool {
    matches!(tile, Tile::Water | Tile::Ford | Tile::Chasm)
}

#[derive(Clone, Copy)]
enum Shape {
    Rock,
    Trunk,
    Cone(usize),
    Flower,
    Reed,
    Puddle,
    /// A faceted ball: canopies, scrub.
    Ball,
    /// A thin branch, a meter long.
    Stick,
}

/// Builds every static prop into one vertex-colored mesh.
struct Props {
    pub(super) theme: &'static Theme,
    pub(super) rock: Mesh,
    trunk: Mesh,
    cones: [Mesh; 3],
    pub(super) flower: Mesh,
    pub(super) reed: Mesh,
    puddle: Mesh,
    pub(super) ball: Mesh,
    stick: Mesh,
    pub(super) out: Option<Mesh>,
}

impl Props {
    pub(super) fn new(theme: &'static Theme) -> Self {
        Props {
            theme,
            rock: faceted(Sphere::new(0.6).mesh().ico(0).unwrap()),
            trunk: faceted(Cylinder::new(0.22, 1.2).mesh().resolution(5).build()),
            cones: [0, 1, 2].map(|k| faceted(Cone::new(1.5 - k as f32 * 0.35, 1.7).mesh().resolution(7).build())),
            flower: gem_mesh(0.14),
            reed: faceted(Cone::new(0.035, 1.0).mesh().resolution(3).build()),
            puddle: faceted(Cylinder::new(1.0, 0.02).mesh().resolution(7).build()),
            ball: faceted(Sphere::new(1.0).mesh().ico(0).unwrap()),
            stick: faceted(Cylinder::new(0.05, 1.0).mesh().resolution(4).build()),
            out: None,
        }
    }

    pub(super) fn add(&mut self, shape: Shape, color: Color, t: Transform) {
        let base = match shape {
            Shape::Rock => &self.rock,
            Shape::Trunk => &self.trunk,
            Shape::Cone(k) => &self.cones[k],
            Shape::Flower => &self.flower,
            Shape::Reed => &self.reed,
            Shape::Puddle => &self.puddle,
            Shape::Ball => &self.ball,
            Shape::Stick => &self.stick,
        };
        self.push(base.clone().transformed_by(t), color);
    }

    /// A cut block (stone, timber) of `size`, placed by `t` (rotation and translation only), its
    /// edges bevelled so they catch the light.
    pub(super) fn block(&mut self, size: Vec3, color: Color, t: Transform) {
        let bevel = (size.min_element() * 0.12).clamp(0.02, 0.07);
        self.push(bevel_box(size, bevel, color).transformed_by(t), color);
    }

    pub(super) fn push(&mut self, mut mesh: Mesh, color: Color) {
        let c = color.to_linear();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[c.red, c.green, c.blue, 1.0]; mesh.count_vertices()]);
        match &mut self.out {
            Some(out) => merge_into(out, &mesh),
            None => self.out = Some(mesh),
        }
    }

    fn into_mesh(self) -> Mesh {
        self.out.expect("the arena has props")
    }

    /// Ruined walls: stacked stone blocks of uneven height, some with a block on top. Returns the
    /// top height per wall tile, so torches can stand on them.
    pub(super) fn walls(&mut self, m: &Map, rng: &mut Lcg) -> HashMap<IVec2, f32> {
        let mut tops = HashMap::default();
        for (t, tile) in m.tiles() {
            // The merchant's counter is their stall's (`stall`), not a ruin.
            if tile != Tile::Wall || m.counter.contains(&t) {
                continue;
            }
            let c = Map::center(t);
            let h = rng.range(0.9, 1.7);
            let color = self.theme.wall.mix(&self.theme.stone, rng.range(0.0, 0.25));
            self.block(Vec3::new(1.0, h, 1.0), color, Transform::from_translation(to_world(c, h / 2.0)));
            let mut top = h;
            if rng.next() < 0.45 {
                let s = rng.range(0.45, 0.7);
                let off = Vec2::new(rng.range(-0.2, 0.2), rng.range(-0.2, 0.2));
                let turn = Quat::from_rotation_y(rng.range(-0.3, 0.3));
                self.block(Vec3::splat(s), color, Transform::from_translation(to_world(c + off, h + s / 2.0)).with_rotation(turn));
                top += s;
            }
            tops.insert(t, top);
            // Rubble at the foot, below knee height, on dry ground only.
            for _ in 0..2 {
                let p = c + Vec2::new(rng.range(-0.9, 0.9), rng.range(-0.9, 0.9));
                if matches!(m.get(Map::tile_of(p)), Tile::Grass | Tile::Path | Tile::Marsh) {
                    self.add(Shape::Rock, self.theme.slate, Transform::from_translation(to_world(p, 0.05)).with_scale(Vec3::splat(rng.range(0.15, 0.3))));
                }
            }
        }
        tops
    }

    /// The merchant's stall, round their counter (`Map::counter`): a plank counter with a cloth
    /// hung down its front and wares on top (coin piles, a strongbox, sacks), four posts holding
    /// a striped awning that slopes down over it with a scalloped edge, shelves of bottles at the
    /// back, barrels and crates at its sides, and a sign with a gold coin on a post.
    pub(super) fn stall(&mut self, m: &Map) {
        if m.counter.is_empty() {
            return;
        }
        let tiles: Vec<Vec2> = m.counter.iter().map(|t| Map::center(*t)).collect();
        let c = tiles.iter().sum::<Vec2>() / tiles.len() as f32;
        // Its front faces where the merchant does; `side` runs along the counter.
        let front = m.shop_faces.normalize_or(Vec2::NEG_Y);
        let side = front.perp();
        let half = tiles.iter().map(|t| (*t - c).dot(side).abs()).fold(0.0f32, f32::max) + 0.5;
        // Turned so a block's X runs along the counter and its Z out to the front.
        let along = to_world(side, 0.0) - to_world(Vec2::ZERO, 0.0);
        let turn = Quat::from_rotation_y((-along.z).atan2(along.x));
        let out_z = (turn * Vec3::Z).dot(to_world(front, 0.0) - to_world(Vec2::ZERO, 0.0)).signum();
        let at = |p: Vec2, h: f32| Transform::from_translation(to_world(p, h)).with_rotation(turn);
        let (wood, plank) = (BARK, BARK.lighter(0.12));
        let len = half * 2.0;

        // The counter, its top overhanging, a striped cloth down its front.
        self.block(Vec3::new(len, 1.0, 0.85), wood.darker(0.1), at(c, 0.5));
        self.block(Vec3::new(len + 0.25, 0.08, 1.0), plank, at(c, 1.04));
        let stripes = 7;
        for i in 0..stripes {
            let x = -half + 0.1 + (len - 0.2) * (i as f32 + 0.5) / stripes as f32;
            let color = if i % 2 == 0 { MERCHANT_RUG } else { GOLD.darker(0.35) };
            self.block(Vec3::new((len - 0.2) / stripes as f32, 0.6, 0.03), color, at(c + side * x + front * 0.44, 0.62));
        }
        // Wares on the counter: coin piles, a strongbox, sacks.
        for (x, s) in [(-0.6, 0.16), (-0.35, 0.12), (0.7, 0.14)] {
            self.add(Shape::Ball, GOLD, Transform::from_translation(to_world(c + side * x + front * 0.15, 1.1)).with_scale(Vec3::new(s, s * 0.45, s)));
        }
        self.block(Vec3::new(0.42, 0.26, 0.3), wood.darker(0.25), at(c + side * 0.1 - front * 0.1, 1.21));
        self.block(Vec3::new(0.44, 0.06, 0.32), GOLD.darker(0.2), at(c + side * 0.1 - front * 0.1, 1.36));
        for x in [-half + 0.3, half - 0.35] {
            self.add(Shape::Ball, CANVAS, Transform::from_translation(to_world(c + side * x - front * 0.2, 1.25)).with_scale(Vec3::new(0.2, 0.24, 0.2)));
        }

        // Posts: two at the counter's front corners, two taller ones at the back.
        let back = -1.3;
        for (out, h) in [(0.5, 2.4), (back, 2.95)] {
            for s in [-1.0, 1.0] {
                self.block(Vec3::new(0.13, h, 0.13), wood, at(c + side * s * (half + 0.05) + front * out, h / 2.0));
            }
        }
        // The back: a board wall with two shelves of bottles.
        let wall = c + front * (back - 0.05);
        self.block(Vec3::new(len + 0.1, 2.3, 0.08), wood.darker(0.15), at(wall, 1.15));
        let colors = [MERCHANT_FEATHER, FROST_BLUE, HEAL, GOLD, SPIRIT, AWNING_RED];
        for (row, h) in [(0, 1.3), (1, 1.9)] {
            let shelf = c + front * (back + 0.15);
            self.block(Vec3::new(len, 0.05, 0.34), plank, at(shelf, h));
            for i in 0..7 {
                let x = -half + 0.25 + (len - 0.5) * i as f32 / 6.0;
                let color = colors[(i + row * 3) % colors.len()];
                self.block(Vec3::new(0.1, 0.22, 0.1), color, at(shelf + side * x, h + 0.14));
            }
        }

        // The awning, sloping from the back posts' tops down past the counter's front, in stripes,
        // and a scalloped edge hanging from its front.
        let (top, low): ((f32, f32), (f32, f32)) = ((back, 2.95), (0.95, 2.25));
        let run = top.0 - low.0;
        let slope = (top.1 - low.1).atan2(run.abs());
        let length = (run * run + (top.1 - low.1) * (top.1 - low.1)).sqrt() + 0.1;
        let middle = c + front * (top.0 + low.0) / 2.0;
        let stripes = 8;
        let width = (len + 0.5) / stripes as f32;
        for i in 0..stripes {
            let x = -(len + 0.5) / 2.0 + width * (i as f32 + 0.5);
            let color = if i % 2 == 0 { AWNING_RED } else { AWNING_CREAM };
            let t = Transform::from_translation(to_world(middle + side * x, (top.1 + low.1) / 2.0)).with_rotation(turn * Quat::from_rotation_x(slope * out_z));
            self.block(Vec3::new(width, 0.05, length), color, t);
            // A scallop under the front edge, the same stripe.
            self.add(Shape::Ball, color, Transform::from_translation(to_world(c + side * x + front * (low.0 + 0.02), low.1 - 0.06)).with_rotation(turn).with_scale(Vec3::new(width * 0.5, 0.14, 0.04)));
        }

        // Barrels and crates at its sides.
        for (s, out) in [(-1.0, 0.1), (1.0, -0.4)] {
            let p = c + side * s * (half + 0.55) + front * out;
            self.push(tinted(cylinder(0.3, 0.8, 12).translated_by(to_world(p, 0.4)), wood.darker(0.05)), wood.darker(0.05));
            self.push(tinted(cylinder(0.31, 0.06, 12).translated_by(to_world(p, 0.62)), IRON), IRON);
            self.push(tinted(cylinder(0.31, 0.06, 12).translated_by(to_world(p, 0.18)), IRON), IRON);
        }
        self.block(Vec3::new(0.5, 0.45, 0.5), plank, at(c - side * (half + 0.6) + front * (back + 0.4), 0.225));
        self.block(Vec3::new(0.4, 0.35, 0.4), plank.darker(0.1), at(c - side * (half + 0.6) + front * (back + 0.4), 0.62));

        // A sign on the front post: a board with a gold coin painted on it.
        let post = c + side * (half + 0.05) + front * 0.5;
        let sign = post + side * 0.42;
        self.block(Vec3::new(0.7, 0.05, 0.05), wood, at(post + side * 0.3, 2.15));
        self.block(Vec3::new(0.55, 0.4, 0.05), plank, at(sign, 1.85));
        let coin = Transform::from_translation(to_world(sign + front * 0.035 * out_z.abs(), 1.85)).with_rotation(turn * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
        self.push(tinted(cylinder(0.13, 0.03, 16).transformed_by(coin), GOLD), GOLD);
    }

    /// Stone bridges, tile by tile: paving slabs, a parapet along every side that drops into
    /// water or a chasm (a cap stone where it ends), and under them a deck down into the water,
    /// or over a chasm an arch, deepest by the rock it springs from.
    pub(super) fn bridges(&mut self, m: &Map, rng: &mut Lcg) {
        let bridge = |t: IVec2| m.get(t) == Tile::Bridge;
        for (t, tile) in m.tiles() {
            if tile != Tile::Bridge {
                continue;
            }
            let c = Map::center(t);
            let color = if rng.next() < 0.5 { self.theme.stone } else { self.theme.slate };
            self.block(Vec3::new(0.97, 0.36, 0.97), color, Transform::from_translation(to_world(c, -0.18)));
            // How far to the nearest bank, along the bridge.
            let to_bank = SIDES
                .iter()
                .filter_map(|&d| {
                    let steps = (1..12).find(|&k| !bridge(t + d * k))?;
                    m.walkable(t + d * steps).then_some(steps)
                })
                .min()
                .unwrap_or(6);
            let over_water = SIDES.iter().any(|&d| matches!(m.scene_tile(t + d), Tile::Water | Tile::Ford));
            let depth = if over_water { -RIVERBED_LEVEL } else { 0.4 + 2.2 / to_bank as f32 };
            self.block(Vec3::new(1.0, depth, 1.0), self.theme.wall, Transform::from_translation(to_world(c, -0.36 - depth / 2.0)));
            for d in SIDES {
                if !is_gap(m.scene_tile(t + d)) {
                    continue;
                }
                let along = IVec2::new(-d.y, d.x);
                let out = d.as_vec2();
                let size = if d.x == 0 { Vec3::new(1.02, 0.7, 0.36) } else { Vec3::new(0.36, 0.7, 1.02) };
                self.block(size, self.theme.wall.mix(&self.theme.stone, 0.12), Transform::from_translation(to_world(c + out * 0.68, 0.25)));
                for side in [along, -along] {
                    let runs_on = bridge(t + side) && is_gap(m.scene_tile(t + side + d));
                    if !runs_on {
                        let at = c + out * 0.68 + side.as_vec2() * 0.5;
                        self.block(Vec3::new(0.5, 1.1, 0.5), self.theme.wall.mix(&self.theme.stone, 0.24), Transform::from_translation(to_world(at, 0.45)));
                    }
                }
            }
        }
    }

    /// Rope bridges over a chasm, tile by tile: planks laid across the way over, two beams
    /// under them, rope rails along the sides, on posts where they meet the rock.
    fn rope_bridges(&mut self, m: &Map, rng: &mut Lcg) {
        let planks = |t: IVec2| m.get(t) == Tile::Planks;
        for (t, tile) in m.tiles() {
            if tile != Tile::Planks {
                continue;
            }
            let c = Map::center(t);
            // The way over: the axis whose neighbors aren't the drop.
            let along_x = m.scene_tile(t + IVec2::X) != Tile::Chasm && m.scene_tile(t - IVec2::X) != Tile::Chasm;
            let (way, across) = if along_x { (Vec2::X, Vec2::Y) } else { (Vec2::Y, Vec2::X) };
            // Blocks are laid out along x; this turns one to run along the way over.
            let turn = if along_x { Quat::IDENTITY } else { Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) };
            for k in 0..3 {
                let at = c + way * (k as f32 - 1.0) * 0.32 + across * rng.range(-0.04, 0.04);
                let color = self.theme.bark.mix(&self.theme.path, rng.range(0.0, 0.35));
                let tilt = Quat::from_rotation_y(rng.range(-0.06, 0.06));
                self.block(Vec3::new(0.26, 0.07, 1.0), color, Transform::from_translation(to_world(at, -0.04)).with_rotation(turn * tilt));
            }
            for side in [-1.0, 1.0] {
                let at = c + across * side * 0.38;
                self.block(Vec3::new(1.0, 0.1, 0.1), self.theme.bark, Transform::from_translation(to_world(at, -0.14)).with_rotation(turn));
            }
            for d in SIDES {
                if m.scene_tile(t + d) != Tile::Chasm {
                    continue;
                }
                let edge = c + d.as_vec2() * 0.46;
                for h in [0.45, 0.85] {
                    self.block(Vec3::new(1.0, 0.05, 0.05), self.theme.rope, Transform::from_translation(to_world(edge, h)).with_rotation(turn));
                }
                // A hanger from the top rope down to the planks.
                self.block(Vec3::new(0.04, 0.85, 0.04), self.theme.rope, Transform::from_translation(to_world(edge, 0.42)));
                let along = IVec2::new(-d.y, d.x);
                for side in [along, -along] {
                    if !planks(t + side) {
                        let at = edge + side.as_vec2() * 0.5;
                        self.block(Vec3::new(0.16, 1.15, 0.16), self.theme.bark, Transform::from_translation(to_world(at, 0.5)));
                    }
                }
            }
        }
    }

    /// Stepping stones across the fords.
    fn fords(&mut self, m: &Map, rng: &mut Lcg) {
        for (t, tile) in m.tiles() {
            if tile != Tile::Ford {
                continue;
            }
            for _ in 0..2 {
                let p = Map::center(t) + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
                let s = rng.range(0.55, 0.75);
                let stone = Transform::from_translation(to_world(p, -0.12))
                    .with_scale(Vec3::new(s, 0.3, s))
                    .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)));
                self.add(Shape::Rock, self.theme.stone, stone);
            }
        }
    }

    /// A marsh tile's look: here and there a still puddle, and tufts of reeds, all below the knee.
    pub(super) fn bog(&mut self, rng: &mut Lcg, c: Vec2) {
        if rng.next() < 0.4 {
            let p = c + Vec2::new(rng.range(-0.25, 0.25), rng.range(-0.25, 0.25));
            let size = Vec3::new(rng.range(0.25, 0.45), 1.0, rng.range(0.2, 0.35));
            let t = Transform::from_translation(to_world(p, 0.012)).with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0))).with_scale(size);
            self.add(Shape::Puddle, self.theme.pond.mix(&self.theme.riverbed, 0.35), t);
        }
        if rng.next() < 0.55 {
            let p = c + Vec2::new(rng.range(-0.35, 0.35), rng.range(-0.35, 0.35));
            self.bog_reeds(rng, p, self.theme.fern, self.theme.path.mix(&self.theme.fern, 0.5));
        }
    }

    /// A tuft of reeds at `p`, below the knee, each one or the other color.
    fn bog_reeds(&mut self, rng: &mut Lcg, p: Vec2, one: Color, other: Color) {
        for _ in 0..4 {
            let at = p + Vec2::new(rng.range(-0.12, 0.12), rng.range(-0.12, 0.12));
            let h = rng.range(0.25, 0.48);
            let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.25, 0.25), 0.0, rng.range(-0.25, 0.25));
            let color = if rng.next() < 0.5 { one } else { other };
            self.add(Shape::Reed, color, Transform::from_translation(to_world(at, h / 2.0)).with_rotation(lean).with_scale(Vec3::new(1.0, h, 1.0)));
        }
    }

    fn boulder(&mut self, rng: &mut Lcg, c: Vec2) {
        let t = Transform::from_translation(to_world(c, 0.25))
            .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
            .with_scale(Vec3::new(rng.range(1.2, 1.5), rng.range(0.9, 1.3), rng.range(1.2, 1.5)));
        self.add(Shape::Rock, self.theme.slate, t);
    }

    fn tree(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        self.add(Shape::Trunk, self.theme.bark, Transform::from_translation(to_world(p, ground + 0.6 * s)).with_scale(Vec3::splat(s)));
        let tiers = 2 + (rng.next() * 2.0) as usize;
        // Each tree leans its own way between the two greens, so the stand isn't a print.
        let lean = rng.next() * 0.2;
        for k in 0..tiers {
            let t = Transform::from_translation(to_world(p, ground + (1.5 + k as f32 * 0.85) * s))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::splat(s));
            let leaf = if k % 2 == 0 { self.theme.fern } else { self.theme.pine };
            self.add(Shape::Cone(k), leaf.mix(&self.theme.meadow, lean), t);
        }
    }

    /// What grows at a forest tree's feet: a fern clump or two, and now and then a branch of
    /// deadfall lying in the needles.
    fn understory(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        for _ in 0..1 + (rng.next() * 2.0) as usize {
            let at = p + Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
            let s = rng.range(0.25, 0.5);
            let t = Transform::from_translation(to_world(at, ground + s * 0.4))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::new(s * 1.5, s, s * 1.5));
            self.add(Shape::Ball, self.theme.fern.mix(&self.theme.grass, rng.next() * 0.4), t);
        }
        if rng.next() < 0.5 {
            let at = p + Vec2::new(rng.range(-1.2, 1.2), rng.range(-1.2, 1.2));
            let yaw = rng.range(0.0, std::f32::consts::TAU);
            let long = rng.range(0.8, 1.6);
            let t = Transform::from_translation(to_world(at, ground + 0.05))
                .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2 + rng.range(-0.1, 0.1)))
                .with_scale(Vec3::new(1.6, long, 1.6));
            self.add(Shape::Stick, self.theme.bark.mix(&self.theme.slate, 0.3), t);
        }
    }

    pub(super) fn flower(&mut self, rng: &mut Lcg, c: Vec2) {
        let p = c + Vec2::new(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4));
        self.add(Shape::Flower, self.theme.bloom, Transform::from_translation(to_world(p, 0.1)));
    }

    /// A map's tree (a `Tile::Tree`), as its wilds grow them.
    fn map_tree(&mut self, rng: &mut Lcg, c: Vec2) {
        match self.theme.wilds {
            Wilds::Forest => {
                self.tree(rng, c, 0.0, 1.25);
                if rng.next() < 0.5 {
                    self.understory(rng, c, 0.0);
                }
            }
            Wilds::Fen => self.willow(rng, c, 0.0, 1.1),
            Wilds::Highlands => self.round_tree(rng, c, 0.0, 1.0),
        }
    }

    /// Something out in the wilds at `p`, on land `ground` high.
    fn wild(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let pick = rng.next();
        let s = rng.range(0.9, 1.8);
        match self.theme.wilds {
            Wilds::Forest => {
                self.tree(rng, p, ground, s);
                if rng.next() < 0.45 {
                    self.understory(rng, p, ground);
                }
            }
            Wilds::Fen if pick < 0.1 => self.snag(rng, p, ground, s),
            Wilds::Fen if pick < 0.18 => self.willow(rng, p, ground, s * 0.8),
            Wilds::Fen => self.reed_bed(rng, p, ground),
            Wilds::Highlands if pick < 0.2 => self.round_tree(rng, p, ground, s * 0.75),
            Wilds::Highlands if pick < 0.32 => self.pillar(rng, p, ground),
            Wilds::Highlands if pick < 0.37 => self.spire(rng, p, ground),
            Wilds::Highlands if pick < 0.4 => self.arch(rng, p, ground),
            Wilds::Highlands if pick < 0.44 => self.slab(rng, p, ground),
            Wilds::Highlands if pick < 0.47 => self.butte(rng, p, ground),
            Wilds::Highlands if pick < 0.55 => {
                let t = Transform::from_translation(to_world(p, ground + 0.2 * s))
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
                    .with_scale(Vec3::new(1.3, 0.9, 1.2) * s);
                self.add(Shape::Rock, self.theme.slate.mix(&self.theme.wall, rng.next()), t);
            }
            Wilds::Highlands => self.shrub(rng, p, ground),
        }
    }

    /// A dead tree: a grey, bare trunk with a few crooked branches.
    fn snag(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        let wood = self.theme.bark.mix(&self.theme.slate, 0.5);
        let tall = 2.0 * s;
        let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.12, 0.12), 0.0, rng.range(-0.12, 0.12));
        self.add(Shape::Trunk, wood, Transform::from_translation(to_world(p, ground + 0.6 * tall)).with_rotation(lean).with_scale(Vec3::new(0.7 * s, tall, 0.7 * s)));
        for _ in 0..2 + (rng.next() * 2.0) as usize {
            let from = to_world(p, ground + tall * rng.range(0.6, 1.1));
            let out = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU)) * Quat::from_rotation_z(rng.range(0.6, 1.1));
            let long = rng.range(0.7, 1.2) * s;
            let t = Transform::from_translation(from + out * Vec3::Y * long / 2.0).with_rotation(out).with_scale(Vec3::new(s, long, s));
            self.add(Shape::Stick, wood, t);
        }
    }

    /// A low, wide willow: a short trunk under a heap of dark, drooping foliage.
    fn willow(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        self.add(Shape::Trunk, self.theme.bark, Transform::from_translation(to_world(p, ground + 0.6 * s)).with_scale(Vec3::new(1.2 * s, s, 1.2 * s)));
        for k in 0..3 {
            let off = Vec2::from_angle(k as f32 * 2.1 + rng.range(0.0, 1.0)) * 0.5 * s;
            let t = Transform::from_translation(to_world(p + off, ground + rng.range(1.3, 1.7) * s))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::new(1.2, 0.85, 1.2) * s);
            self.add(Shape::Ball, if k % 2 == 0 { self.theme.pine } else { self.theme.fern }, t);
        }
    }

    /// A clump of tall reeds standing in the shallows.
    fn reed_bed(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        for _ in 0..3 + (rng.next() * 4.0) as usize {
            let c = p + Vec2::new(rng.range(-0.9, 0.9), rng.range(-0.9, 0.9));
            for _ in 0..5 {
                let at = c + Vec2::new(rng.range(-0.15, 0.15), rng.range(-0.15, 0.15));
                let h = rng.range(0.9, 1.5);
                let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.2, 0.2), 0.0, rng.range(-0.2, 0.2));
                let color = [self.theme.fern, self.theme.path.mix(&self.theme.fern, 0.5), self.theme.marsh][(rng.next() * 3.0) as usize % 3];
                self.add(Shape::Reed, color, Transform::from_translation(to_world(at, ground + h / 2.0)).with_rotation(lean).with_scale(Vec3::new(1.4, h, 1.4)));
            }
        }
    }

    /// A round broadleaf in autumn: a trunk under a few faceted balls of brown, rust and gold.
    fn round_tree(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        self.add(Shape::Trunk, self.theme.bark, Transform::from_translation(to_world(p, ground + 0.9 * s)).with_scale(Vec3::new(0.8 * s, 1.5 * s, 0.8 * s)));
        let leaves = [self.theme.fern, self.theme.bloom, self.theme.meadow.mix(&self.theme.bloom, 0.35)];
        for k in 0..3 {
            let off = Vec2::from_angle(k as f32 * 2.1 + rng.range(0.0, 1.0)) * 0.45 * s;
            let t = Transform::from_translation(to_world(p + off, ground + rng.range(2.1, 2.7) * s))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::splat(rng.range(0.85, 1.1) * s));
            self.add(Shape::Ball, leaves[(rng.next() * 3.0) as usize % 3], t);
        }
    }

    /// A low bush of dry scrub.
    fn shrub(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let s = rng.range(0.35, 0.6);
        let t = Transform::from_translation(to_world(p, ground + 0.15)).with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0))).with_scale(Vec3::new(s, s * 0.7, s));
        self.add(Shape::Ball, self.theme.fern.mix(&self.theme.grass, rng.next() * 0.5), t);
    }

    /// A hoodoo: boulders stacked, narrowing and jostled, in bands of color, under a wider cap.
    fn pillar(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let count = 3 + (rng.next() * 3.0) as usize;
        let first = (rng.next() * 4.0) as usize;
        let mut at = p;
        for k in 0..count {
            let w = 1.9 - k as f32 * rng.range(0.15, 0.3);
            at += Vec2::new(rng.range(-0.12, 0.12), rng.range(-0.12, 0.12));
            let t = Transform::from_translation(to_world(at, ground + 0.5 + k as f32 * 1.05))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::new(w, 1.3, w));
            self.add(Shape::Rock, rock_band(self.theme, first + k), t);
        }
        let cap = Transform::from_translation(to_world(at, ground + 0.6 + count as f32 * 1.05))
            .with_rotation(Quat::from_rotation_z(rng.range(-0.15, 0.15)))
            .with_scale(Vec3::new(rng.range(1.8, 2.4), 0.7, rng.range(1.8, 2.4)));
        self.add(Shape::Rock, self.theme.slate.darker(0.1), cap);
    }

    /// A tall, thin needle of rock, leaning a little.
    fn spire(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let tall = rng.range(4.0, 6.5);
        let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.1, 0.1), rng.range(0.0, 3.0), rng.range(-0.1, 0.1));
        let first = (rng.next() * 4.0) as usize;
        for k in 0..3 {
            let up = tall * (0.2 + k as f32 * 0.3);
            let w = 1.3 - k as f32 * 0.35;
            let t = Transform::from_translation(to_world(p, ground) + lean * Vec3::Y * up).with_rotation(lean).with_scale(Vec3::new(w, tall * 0.32, w));
            self.add(Shape::Rock, rock_band(self.theme, first + k), t);
        }
    }

    /// A natural arch: two stacks of rock with a long one laid across their tops.
    pub(super) fn arch(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let across = Vec2::from_angle(rng.range(0.0, std::f32::consts::PI));
        let (span, tall) = (rng.range(1.8, 2.6), rng.range(2.6, 3.4));
        for side in [-1.0, 1.0] {
            let foot = p + across * side * span;
            for k in 0..3 {
                let t = Transform::from_translation(to_world(foot, ground + 0.4 + k as f32 * tall / 3.0))
                    .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                    .with_scale(Vec3::new(1.4 - k as f32 * 0.1, tall / 2.6, 1.4 - k as f32 * 0.1));
                self.add(Shape::Rock, rock_band(self.theme, k), t);
            }
        }
        let turn = Quat::from_rotation_y(-across.to_angle());
        let lintel = Transform::from_translation(to_world(p, ground + tall + 0.5)).with_rotation(turn).with_scale(Vec3::new(span * 2.2, 0.9, 1.3));
        self.add(Shape::Rock, rock_band(self.theme, 3), lintel);
    }

    /// A slab of rock stood on end, tipped over to one side.
    fn slab(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let tilt = Quat::from_euler(EulerRot::YXZ, rng.range(0.0, 3.0), rng.range(-0.35, 0.35), rng.range(-0.2, 0.2));
        let t = Transform::from_translation(to_world(p, ground + 1.3)).with_rotation(tilt).with_scale(Vec3::new(0.7, rng.range(3.5, 4.8), rng.range(2.4, 3.2)));
        self.add(Shape::Rock, rock_band(self.theme, (rng.next() * 4.0) as usize), t);
    }

    /// A squat butte: a wide, flat-topped heap of banded rock with dry grass on top.
    fn butte(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let w = rng.range(3.5, 5.0);
        for k in 0..3 {
            let t = Transform::from_translation(to_world(p, ground + 0.5 + k as f32 * 0.9))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::new(w - k as f32 * 0.4, 1.0, w - k as f32 * 0.4));
            self.add(Shape::Rock, rock_band(self.theme, k), t);
        }
        let top = Transform::from_translation(to_world(p, ground + 2.65)).with_scale(Vec3::new(w * 0.5, 0.18, w * 0.5));
        self.add(Shape::Ball, self.theme.meadow, top);
    }

    /// A rock on the highlands' fight floor: sandstone in one of a few forms, all about a tile
    /// across like any rock there.
    pub(super) fn sandstone(&mut self, rng: &mut Lcg, c: Vec2) {
        let turn = Quat::from_rotation_y(rng.range(0.0, 3.0));
        match (rng.next() * 3.0) as usize {
            // Flat-topped, with a smaller one balanced on it.
            0 => {
                self.add(Shape::Rock, rock_band(self.theme, 0), Transform::from_translation(to_world(c, 0.3)).with_rotation(turn).with_scale(Vec3::new(1.5, 0.9, 1.4)));
                self.add(Shape::Rock, rock_band(self.theme, 2), Transform::from_translation(to_world(c, 0.9)).with_rotation(turn.inverse()).with_scale(Vec3::new(0.8, 0.7, 0.9)));
            }
            // Split in two, a crack between them.
            1 => {
                let side = Vec2::from_angle(rng.range(0.0, 3.0)) * 0.28;
                for (k, s) in [(1, 1.0), (3, -1.0)] {
                    let t = Transform::from_translation(to_world(c + side * s, 0.35)).with_rotation(turn).with_scale(Vec3::new(0.9, 1.3, 1.4));
                    self.add(Shape::Rock, rock_band(self.theme, k), t);
                }
            }
            // A stack of two, banded.
            _ => {
                self.add(Shape::Rock, rock_band(self.theme, 1), Transform::from_translation(to_world(c, 0.3)).with_rotation(turn).with_scale(Vec3::new(1.4, 0.9, 1.4)));
                self.add(Shape::Rock, rock_band(self.theme, 0), Transform::from_translation(to_world(c, 0.95)).with_rotation(turn).with_scale(Vec3::new(1.1, 0.8, 1.1)));
            }
        }
    }

    /// What litters the fight floor's grass (forest and highlands; the fen has `fen_grass`):
    /// tufts, pebbles, a flower, a patch of bare earth — and in the lee of cover (trees, walls,
    /// rock, the forest's edge) thicker growth, mushrooms and a stump or two, so the clearing
    /// reads as a lived-in place and not an empty board. Everything is below the ankle: detail,
    /// never obstacle.
    pub(super) fn floor_detail(&mut self, m: &Map, rng: &mut Lcg, t: IVec2, c: Vec2) {
        let near_cover = SIDES.iter().any(|&d| matches!(m.scene_tile(t + d), Tile::Tree | Tile::Forest | Tile::Wall | Tile::Rock));
        let woods = self.theme.wilds == Wilds::Forest;
        let pick = rng.next();
        if near_cover {
            match () {
                _ if pick < 0.3 => self.grass_tuft(rng, c),
                _ if pick < 0.36 && woods => self.mushrooms(rng, c),
                _ if pick < 0.4 && woods => self.stump(rng, c),
                _ if pick < 0.46 => self.pebbles(rng, c),
                _ if pick < 0.52 => self.earth_patch(rng, c),
                _ if pick < 0.56 => self.flower(rng, c),
                _ => {}
            }
        } else {
            match () {
                _ if pick < 0.14 => self.grass_tuft(rng, c),
                _ if pick < 0.18 => self.pebbles(rng, c),
                _ if pick < 0.24 => self.earth_patch(rng, c),
                _ if pick < 0.27 => self.flower(rng, c),
                _ => {}
            }
        }
    }

    /// A tuft of grass blades, a shade lusher or drier than the lawn they stand on.
    fn grass_tuft(&mut self, rng: &mut Lcg, c: Vec2) {
        let p = c + Vec2::new(rng.range(-0.35, 0.35), rng.range(-0.35, 0.35));
        let lush = self.theme.fern.mix(&self.theme.grass, 0.45);
        let dry = self.theme.meadow.mix(&self.theme.path, 0.35);
        for _ in 0..3 + (rng.next() * 2.0) as usize {
            let at = p + Vec2::new(rng.range(-0.12, 0.12), rng.range(-0.12, 0.12));
            let h = rng.range(0.12, 0.26);
            let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.35, 0.35), 0.0, rng.range(-0.35, 0.35));
            let color = if rng.next() < 0.5 { lush } else { dry };
            self.add(Shape::Reed, color, Transform::from_translation(to_world(at, h / 2.0)).with_rotation(lean).with_scale(Vec3::new(1.8, h, 1.8)));
        }
    }

    /// A pebble or two, half sunk in the turf.
    pub(super) fn pebbles(&mut self, rng: &mut Lcg, c: Vec2) {
        for _ in 0..1 + (rng.next() * 2.0) as usize {
            let p = c + Vec2::new(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4));
            let s = rng.range(0.07, 0.16);
            let t = Transform::from_translation(to_world(p, s * 0.25))
                .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
                .with_scale(Vec3::new(s, s * 0.6, s));
            self.add(Shape::Rock, self.theme.stone.mix(&self.theme.path, rng.next() * 0.5), t);
        }
    }

    /// A worn patch of bare earth showing through the grass.
    fn earth_patch(&mut self, rng: &mut Lcg, c: Vec2) {
        let p = c + Vec2::new(rng.range(-0.25, 0.25), rng.range(-0.25, 0.25));
        let size = Vec3::new(rng.range(0.3, 0.55), 1.0, rng.range(0.25, 0.45));
        let color = self.theme.path.mix(&self.theme.grass, rng.range(0.2, 0.5)).darker(0.03);
        let t = Transform::from_translation(to_world(p, 0.008)).with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0))).with_scale(size);
        self.add(Shape::Puddle, color, t);
    }

    /// A few toadstools huddled together: pale stems under caps the map's bloom color.
    fn mushrooms(&mut self, rng: &mut Lcg, c: Vec2) {
        let p = c + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
        let stem_color = self.theme.stone.lighter(0.15);
        for _ in 0..2 + (rng.next() * 3.0) as usize {
            let at = p + Vec2::new(rng.range(-0.18, 0.18), rng.range(-0.18, 0.18));
            let h = rng.range(0.07, 0.15);
            self.add(Shape::Stick, stem_color, Transform::from_translation(to_world(at, h / 2.0)).with_scale(Vec3::new(h * 0.7, h, h * 0.7)));
            let cap = Transform::from_translation(to_world(at, h))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::new(h * 0.9, h * 0.45, h * 0.9));
            self.add(Shape::Ball, self.theme.bloom.darker(0.08), cap);
        }
    }

    /// A stump, cut low, its top rings showing.
    fn stump(&mut self, rng: &mut Lcg, c: Vec2) {
        let p = c + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
        let h = rng.range(0.18, 0.3);
        let s = rng.range(0.8, 1.1);
        self.add(Shape::Trunk, self.theme.bark, Transform::from_translation(to_world(p, h / 2.0)).with_scale(Vec3::new(s, h / 1.2, s)));
        let top = Transform::from_translation(to_world(p, h + 0.005)).with_scale(Vec3::new(0.2 * s, 1.0, 0.2 * s));
        self.add(Shape::Puddle, self.theme.path.lighter(0.08), top);
    }

    /// What grows on the fen's fight floor here and there: tufts of sedge, bog cotton and pads of
    /// moss, all below the ankle.
    fn fen_grass(&mut self, rng: &mut Lcg, c: Vec2) {
        let pick = rng.next();
        let p = c + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
        if pick < 0.06 {
            self.bog_reeds(rng, p, self.theme.fern, self.theme.marsh);
        } else if pick < 0.1 {
            self.flower(rng, c);
        } else if pick < 0.15 {
            let size = Vec3::new(rng.range(0.3, 0.5), 0.05, rng.range(0.25, 0.4));
            let t = Transform::from_translation(to_world(p, 0.0)).with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0))).with_scale(size);
            self.add(Shape::Ball, self.theme.fern.mix(&self.theme.meadow, 0.3), t);
        }
    }

    /// Worn paving on the fen's path tiles: each tile two to four slabs, uneven, some sunk or
    /// gone, moss showing between them.
    fn flagstones(&mut self, m: &Map, rng: &mut Lcg) {
        for (t, tile) in m.tiles() {
            if tile != Tile::Path {
                continue;
            }
            let c = Map::center(t);
            let split = if rng.next() < 0.5 { Vec2::X } else { Vec2::Y };
            let pieces = 1 + (rng.next() * 2.0) as usize;
            for k in 0..pieces {
                if rng.next() < 0.12 {
                    continue;
                }
                let len = 1.0 / pieces as f32;
                let along = (k as f32 + 0.5) * len - 0.5;
                let size = split * (len - 0.07) + split.perp().abs() * 0.92;
                let color = self.theme.stone.mix(&self.theme.wall, rng.range(0.2, 0.7)).darker(rng.range(0.0, 0.12));
                let turn = Quat::from_rotation_y(rng.range(-0.04, 0.04));
                let at = to_world(c + split * along, rng.range(0.005, 0.025));
                self.block(Vec3::new(size.x, 0.05, size.y), color, Transform::from_translation(at).with_rotation(turn));
            }
        }
    }
}

/// Height of the land outside the clearing (0 at its edge), before roughness: the forest rolls
/// up into low hills, a fen sinks under the water with tussocks breaking it, highlands climb in
/// terraces.
fn wild_height(theme: &Theme, m: &Map, p: Vec2) -> f32 {
    let out = -m.edge_margin(p);
    match theme.wilds {
        Wilds::Forest => (out * 0.3).clamp(0.0, 3.0),
        Wilds::Fen => {
            // Broad, low tussocks: where slow waves running four ways crest together.
            let hump = (waves(p, [0.31, 0.27, 0.19, 0.23], [0.7, 1.3, 0.0, 2.0]) - 0.15).clamp(0.0, 0.5) * 2.0;
            let deep = (out / 3.0).clamp(0.0, 1.0);
            (-0.7 + hump * 0.95) * deep
        }
        Wilds::Highlands => {
            let rise = (out * 0.32).clamp(0.0, 14.0);
            let f = (rise / TERRACE).fract();
            (rise / TERRACE).floor() * TERRACE + TERRACE * ((f - 0.6) / 0.25).clamp(0.0, 1.0)
        }
    }
}

/// The bands of color the highlands' rock is laid down in, from `k` up.
fn rock_band(theme: &Theme, k: usize) -> Color {
    [theme.wall, theme.slate, theme.stone, theme.bank][k % 4]
}

/// How high each of the highlands' terraces climbs.
const TERRACE: f32 = 3.0;

/// The color of a facet of the wilds `height` high, `flat` (its normal's upward part: 1 flat, 0
/// sheer), `pick` a random number for variety.
fn wild_color(theme: &Theme, height: f32, flat: f32, pick: f32) -> Color {
    match theme.wilds {
        Wilds::Forest if height > 1.4 => {
            if pick < 0.5 { theme.fern } else { theme.pine }
        }
        Wilds::Forest if pick < 0.15 => theme.path,
        Wilds::Forest if pick < 0.6 => theme.grass,
        Wilds::Forest => theme.fern,
        Wilds::Fen if pick < 0.4 => theme.marsh,
        Wilds::Fen if pick < 0.8 => theme.grass.mix(&theme.marsh, 0.4),
        Wilds::Fen => theme.fern,
        // Sheer faces in bands of rock by height; dry grass on the shelves.
        Wilds::Highlands if flat < 0.8 => rock_band(theme, (height / 1.3).floor().max(0.0) as usize),
        Wilds::Highlands if pick < 0.5 => theme.grass,
        Wilds::Highlands if pick < 0.8 => theme.meadow,
        Wilds::Highlands => theme.path,
    }
}

/// Meters between the terrain's vertices. Twice a tile: faceted like low-poly land, not like
/// the tile grid.
const TERRAIN_CELL: i32 = 2;

/// The land under and around the map, on a grid of `TERRAIN_CELL` meters, its vertices on tile
/// corners: flat just under the ground tiles, down to the riverbed under water and to the floor
/// under a chasm (a vertex goes down if any tile around it within a cell does, so the whole of
/// every water or chasm tile is low), and out into the wilds around it (`wild_height`), its
/// vertices jostled where there's nothing but wilds around them.
fn terrain_mesh(m: &Map, scene: &SceneTiles, theme: &Theme, rng: &mut Lcg) -> Mesh {
    let cells = scene.extent / TERRAIN_CELL;
    let width = (2 * cells.x + 1) as usize;
    let mut grid = Vec::with_capacity(width * (2 * cells.y + 1) as usize);
    for y in -cells.y..=cells.y {
        for x in -cells.x..=cells.x {
            let v = IVec2::new(x, y) * TERRAIN_CELL;
            let side = 2 * TERRAIN_CELL;
            let around: [Tile; (4 * TERRAIN_CELL * TERRAIN_CELL) as usize] =
                std::array::from_fn(|k| scene.get(v - IVec2::splat(TERRAIN_CELL) + IVec2::new(k as i32 % side, k as i32 / side)));
            let mut p = v.as_vec2();
            let h = if around.contains(&Tile::Chasm) {
                CHASM_FLOOR + rng.range(-0.3, 0.3)
            } else if around.iter().any(|t| matches!(t, Tile::Water | Tile::Ford)) {
                RIVERBED_LEVEL
            } else if around.iter().all(|t| *t != Tile::Forest) {
                -0.05
            } else if around.iter().all(|t| *t == Tile::Forest) && m.edge_margin(p) < -1.0 {
                // Forest floor is rough; the fen's tussocks and the highlands' shelves smoother.
                let (shift, rough) = match theme.wilds {
                    Wilds::Forest => (0.5, (-0.3, 0.6)),
                    Wilds::Fen => (0.3, (-0.05, 0.05)),
                    Wilds::Highlands => (0.3, (-0.15, 0.15)),
                };
                p += Vec2::new(rng.range(-shift, shift), rng.range(-shift, shift));
                wild_height(theme, m, p) + rng.range(rough.0, rough.1)
            } else {
                wild_height(theme, m, p)
            };
            grid.push(to_world(p, h));
        }
    }
    let at = |i: usize, j: usize| grid[j * width + i];
    let mut b = FlatMesh::default();
    for j in 0..(2 * cells.y) as usize {
        for i in 0..width - 1 {
            let (a, r, u, ru) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            for tri in [[a, r, u], [r, ru, u]] {
                let height = (tri[0].y + tri[1].y + tri[2].y) / 3.0;
                let flat = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero().y.abs();
                let base = if height < -1.2 {
                    // Down a chasm: rock darkening toward the floor.
                    theme.slate.mix(&theme.abyss, ((-height - 1.2) / 3.5).min(1.0))
                } else if height < -0.3 {
                    theme.riverbed
                } else if height < 0.05 {
                    theme.grass
                } else {
                    wild_color(theme, height, flat, rng.next())
                };
                b.tri(tri, Color::from(base.to_linear() * (1.0 + rng.range(-0.04, 0.04))));
            }
        }
    }
    b.build()
}

/// 0 to 1 over the ground in broad, wandering patches.
fn patches(p: Vec2) -> f32 {
    (waves(p, [0.37, 0.41, 0.23, 0.29], [1.1, 0.4, 0.0, 2.3]) / 2.0 + 0.5).clamp(0.0, 1.0)
}

/// -1 to 1: slow waves running four ways (along x, y and both diagonals, at `freq`, shifted by
/// `phase`), high where they crest together.
pub(super) fn waves(p: Vec2, freq: [f32; 4], phase: [f32; 4]) -> f32 {
    let along = [p.x, p.y, p.x + p.y, p.x - p.y];
    (0..4).map(|i| (along[i] * freq[i] + phase[i]).sin()).sum::<f32>() / 4.0
}

/// The walkable ground of the clearing: one quad per tile, one color per tile (a faint checker
/// keeps the grid readable), plus earthen banks down to the water, and cliffs down a chasm in
/// bands of rock.
fn ground_mesh(m: &Map, theme: &Theme, rng: &mut Lcg) -> Mesh {
    let mut b = FlatMesh::default();
    for (t, tile) in m.tiles() {
        if matches!(tile, Tile::Water | Tile::Bridge | Tile::Planks | Tile::Ford | Tile::Chasm | Tile::Forest) {
            continue;
        }
        let c = Map::center(t);
        // Faint: just enough for distances to read, well under the patches' wander.
        let checker = if (t.x + t.y) % 2 == 0 { 1.012 } else { 0.988 };
        let base = match tile {
            // Paved: the slabs go on top (`Props::flagstones`); this is the moss between them.
            Tile::Path if theme.wilds == Wilds::Fen => theme.fern.mix(&theme.marsh, rng.next() * 0.4),
            Tile::Path => theme.path.mix(&theme.grass, rng.next() * 0.25),
            Tile::Marsh => theme.marsh.mix(&theme.bank, rng.next() * 0.35),
            // Damp ground: wet hollows and drier rises wandering across the grass.
            _ if theme.wilds == Wilds::Fen => theme.meadow.mix(&theme.marsh, patches(c)).mix(&theme.grass, rng.next() * 0.3),
            // Lusher and drier sweeps wandering across the lawn, not a tile-by-tile dither.
            _ => theme.grass.mix(&theme.meadow, (patches(c) * 0.75 + rng.next() * 0.25).clamp(0.0, 1.0) * 0.6),
        };
        // Grass in the lee of the trees sits in their shade.
        let shaded = tile == Tile::Grass && SIDES.iter().any(|&d| matches!(m.scene_tile(t + d), Tile::Forest | Tile::Tree));
        let base = if shaded { base.mix(&theme.fern, 0.3) } else { base };
        let color = Color::from(base.to_linear() * checker);
        let (lo, hi) = (c - Vec2::splat(0.5), c + Vec2::splat(0.5));
        b.quad([to_world(lo, 0.0), to_world(Vec2::new(hi.x, lo.y), 0.0), to_world(hi, 0.0), to_world(Vec2::new(lo.x, hi.y), 0.0)], color);
        // Bank or cliff faces toward the gaps around it.
        for (dir, a, z) in [
            (IVec2::X, Vec2::new(hi.x, lo.y), Vec2::new(hi.x, hi.y)),
            (-IVec2::X, Vec2::new(lo.x, hi.y), Vec2::new(lo.x, lo.y)),
            (IVec2::Y, Vec2::new(hi.x, hi.y), Vec2::new(lo.x, hi.y)),
            (-IVec2::Y, Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y)),
        ] {
            match m.scene_tile(t + dir) {
                Tile::Water | Tile::Ford => {
                    b.quad([to_world(a, 0.0), to_world(a, RIVERBED_LEVEL), to_world(z, RIVERBED_LEVEL), to_world(z, 0.0)], theme.bank);
                }
                Tile::Chasm => {
                    // Topsoil, then grey rock in uneven bands, darkening down into the dark.
                    let bands = [
                        (0.0, -0.5, theme.bank),
                        (-0.5, -1.6, theme.slate),
                        (-1.6, -2.4, theme.wall),
                        (-2.4, -4.0, theme.slate.mix(&theme.abyss, 0.5)),
                        (-4.0, CHASM_FLOOR - 0.3, theme.abyss),
                    ];
                    for (top, bottom, color) in bands {
                        let shade = Color::from(color.to_linear() * (1.0 + rng.range(-0.06, 0.06)));
                        b.quad([to_world(a, top), to_world(a, bottom), to_world(z, bottom), to_world(z, top)], shade);
                    }
                }
                _ => {}
            }
        }
    }
    b.build()
}

/// The water's surface: over every water tile of the scene (the map's and the wilds' beyond it),
/// and over the wilds within a terrain cell of those too, so where the land rises out of it the
/// water runs in under its slope; if the wilds are `flooded`, one sheet over the whole scene (the
/// land rises out of it). `None` if there's no water.
fn water_mesh(scene: &SceneTiles, flooded: bool) -> Option<Mesh> {
    let covers = |tile: Tile| matches!(tile, Tile::Water | Tile::Ford);
    let extent = scene.extent;
    let mut b = FlatMesh::default();
    if flooded {
        let (lo, hi) = ((-extent).as_vec2(), extent.as_vec2());
        b.quad([lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)].map(|p| to_world(p, WATER_LEVEL)), Color::WHITE);
        return Some(b.build());
    }
    for y in -extent.y..extent.y {
        for x in -extent.x..extent.x {
            let t = IVec2::new(x, y);
            let tile = scene.get(t);
            let near = || tile == Tile::Forest && (-2..=2).any(|dy| (-2..=2).any(|dx| covers(scene.get(t + IVec2::new(dx, dy)))));
            if !(covers(tile) || near()) {
                continue;
            }
            let (lo, hi) = (t.as_vec2(), t.as_vec2() + Vec2::ONE);
            let corners = [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)];
            b.quad(corners.map(|p| to_world(p, WATER_LEVEL)), Color::WHITE);
        }
    }
    (!b.positions.is_empty()).then(|| b.build())
}
