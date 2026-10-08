//! "The Glade" art direction: a low-poly forest clearing at dusk, with a river, a stone bridge,
//! ruined walls and torches. See `docs/art-direction.md`. The layout itself comes from the shared
//! `arena_shared::map`, so what you see is exactly what blocks movement and shots.
//!
//! Flat-shaded, untextured, one color per face. The world stays in a calm middle band of
//! saturation; only fighters, projectiles and torch flames go above it.

use arena_shared::map::{MAP_HALF_EXTENTS, Map, RIVER_HALF_WIDTH, Tile, clearing_margin, map, river_x};
use bevy::asset::RenderAssetUsages;
use bevy::color::Mix;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub mod palette {
    use bevy::color::Color;

    // The world: below ~50% saturation, 25-72% lightness.
    pub const MOSS: Color = Color::srgb_u8(0x6F, 0x8F, 0x4E);
    pub const MEADOW: Color = Color::srgb_u8(0x8A, 0xA0, 0x5A);
    pub const FERN: Color = Color::srgb_u8(0x4E, 0x77, 0x48);
    pub const PINE: Color = Color::srgb_u8(0x2E, 0x4B, 0x3C);
    pub const SLATE: Color = Color::srgb_u8(0x6C, 0x76, 0x84);
    pub const STONE: Color = Color::srgb_u8(0x84, 0x8A, 0x93);
    pub const WALL: Color = Color::srgb_u8(0x69, 0x71, 0x7D);
    pub const BARK: Color = Color::srgb_u8(0x5E, 0x48, 0x38);
    pub const PATH: Color = Color::srgb_u8(0x95, 0x8C, 0x6E);
    pub const BANK: Color = Color::srgb_u8(0x5A, 0x51, 0x41);
    pub const RIVERBED: Color = Color::srgb_u8(0x3D, 0x4A, 0x44);
    pub const POND: Color = Color::srgb_u8(0x4F, 0x86, 0x82);
    pub const HEATHER: Color = Color::srgb_u8(0x9A, 0x86, 0xB6);
    pub const HAZE: Color = Color::srgb_u8(0xA9, 0xB8, 0xB6);
    /// HUD text.
    pub const INK: Color = Color::srgb_u8(0x1E, 0x2A, 0x23);

    // Fighters: the only saturated colors. You are the only blue; rivals are warm.
    pub const YOU: Color = Color::srgb_u8(0x4C, 0x9E, 0xE0);
    pub const RIVALS: [Color; 3] = [
        Color::srgb_u8(0xE8, 0x80, 0x3A), // ember
        Color::srgb_u8(0xE6, 0xB2, 0x3A), // marigold
        Color::srgb_u8(0xE3, 0x5F, 0x5A), // coral
    ];

    // Light. Fire is the one warm, saturated thing in the world: small, static and flickering.
    pub const SUN: Color = Color::srgb_u8(0xFF, 0xE2, 0xC4);
    pub const SKY: Color = Color::srgb_u8(0x8F, 0xA4, 0xC2);
    pub const TORCH_FLAME: Color = Color::srgb_u8(0xFF, 0xB2, 0x57);
    pub const TORCH_LIGHT: Color = Color::srgb_u8(0xFF, 0x94, 0x43);
}

use palette::*;

/// Gameplay (x, y) to world (x, height, -y): the gameplay plane is the ground, +y is "up" on
/// screen. The one place this convention lives.
pub fn to_world(p: Vec2, height: f32) -> Vec3 {
    Vec3::new(p.x, height, -p.y)
}

pub fn to_gameplay(w: Vec3) -> Vec2 {
    Vec2::new(w.x, -w.z)
}

/// Matte, so flat facets read cleanly. Vertex colors (if any) multiply `color`.
pub fn matte(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.95,
        metallic: 0.0,
        reflectance: 0.2,
        ..default()
    }
}

/// Self-lit: shots, flames and markers that should read the same in light and shade.
pub fn glow(color: Color, strength: f32) -> StandardMaterial {
    StandardMaterial { emissive: LinearRgba::from(color) * strength, unlit: true, ..matte(color) }
}

/// See-through glow: swing flashes and telegraphs.
pub fn translucent(color: Color, alpha: f32, strength: f32) -> StandardMaterial {
    StandardMaterial { alpha_mode: AlphaMode::Blend, ..glow(color.with_alpha(alpha), strength) }
}

/// One normal per face: the low-poly look.
pub fn faceted(mesh: Mesh) -> Mesh {
    mesh.with_duplicated_vertices().with_computed_flat_normals()
}

/// Class ids that have their own figure in `fighter_mesh`. A new class in `classes.ron` gets the
/// plain pawn until it's added here (a test checks every class has one).
pub const FIGHTER_LOOKS: [&str; 4] = ["shade", "warden", "sorcerer", "ranger"];

/// A fighter's low-poly figure, feet at the origin, picked by class id so each class has its own
/// silhouette from above. Ids not in `FIGHTER_LOOKS` get a plain pawn.
pub fn fighter_mesh(class_key: &str) -> Mesh {
    let part = |mesh: Mesh, at: f32| mesh.translated_by(Vec3::Y * at);
    let cylinder = |r: f32, h: f32, sides: u32| Cylinder::new(r, h).mesh().resolution(sides).build();
    let cone = |r: f32, h: f32, sides: u32| Cone::new(r, h).mesh().resolution(sides).build();
    let head = |r: f32| Sphere::new(r).mesh().ico(0).unwrap();
    let parts = match class_key {
        // Assassin: slim, hooded, pointed.
        "shade" => vec![part(cylinder(0.34, 0.9, 6), 0.45), part(cone(0.42, 0.75, 6), 1.25)],
        // Brawler: broad body with a shoulder bar.
        "warden" => vec![
            part(cylinder(0.55, 0.85, 8), 0.425),
            part(Cuboid::new(1.5, 0.28, 0.55).mesh().build(), 0.85),
            part(head(0.38), 1.25),
        ],
        // Caster: robe cone and a tall pointed hat.
        "sorcerer" => vec![part(cone(0.56, 1.15, 8), 0.575), part(head(0.32), 1.3), part(cone(0.42, 0.75, 7), 1.82)],
        // Sniper: lean, with a quiver on the back.
        "ranger" => vec![
            part(cylinder(0.4, 0.95, 7), 0.475),
            part(head(0.36), 1.25),
            part(Cuboid::new(0.2, 0.8, 0.2).mesh().build().translated_by(Vec3::new(-0.25, 0.0, 0.35)), 1.0),
        ],
        _ => vec![part(cylinder(0.48, 0.9, 8), 0.45), part(head(0.42), 1.25)],
    };
    parts
        .into_iter()
        .map(faceted)
        .reduce(|mut all, p| {
            all.merge(&p).expect("fighter parts share attributes");
            all
        })
        .expect("a fighter has parts")
}

/// A flat fan for a melee swing: `reach` long, `arc_degrees` wide, pointing along world +X.
pub fn swing_mesh(reach: f32, arc_degrees: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let half = arc_degrees.to_radians() / 2.0;
    let steps = 12;
    let point = |t: f32| Vec3::new(t.cos() * reach, 0.0, -t.sin() * reach);
    for i in 0..steps {
        let (a, z) = (-half + 2.0 * half * i as f32 / steps as f32, -half + 2.0 * half * (i + 1) as f32 / steps as f32);
        b.tri([Vec3::ZERO, point(a), point(z)], Color::WHITE);
    }
    b.build()
}

/// A flat strip along world +X from `from` to `to`, `width` wide: the lane a shot will fly down.
pub fn lane_mesh(from: f32, to: f32, width: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let side = Vec3::Z * width / 2.0;
    let (near, far) = (Vec3::X * from, Vec3::X * to);
    b.quad([near + side, far + side, far - side, near - side], Color::WHITE);
    b.build()
}

pub fn projectile_mesh(radius: f32) -> Mesh {
    faceted(Sphere::new(radius).mesh().ico(0).unwrap())
}

/// Haze that fades the forest ring into the sky color; goes on the camera.
pub fn haze() -> DistanceFog {
    DistanceFog { color: HAZE, falloff: FogFalloff::Linear { start: 40.0, end: 95.0 }, ..default() }
}

pub struct GladePlugin;

impl Plugin for GladePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(HAZE));
        app.insert_resource(GlobalAmbientLight { color: SKY, brightness: 160.0, ..default() });
        app.add_systems(Startup, build_glade);
        app.add_systems(Update, flicker_torches);
    }
}

const WATER_LEVEL: f32 = -0.3;
const RIVERBED_LEVEL: f32 = -0.65;

/// Deterministic, so the glade looks the same for every player and every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// Collects flat-colored triangles into one mesh.
#[derive(Default)]
struct FlatMesh {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

impl FlatMesh {
    /// One triangle, counter-clockwise seen from its front.
    fn tri(&mut self, corners: [Vec3; 3], color: Color) {
        let c = color.to_linear();
        for p in corners {
            self.positions.push(p.to_array());
            self.colors.push([c.red, c.green, c.blue, 1.0]);
        }
    }

    /// A quad from four corners in order (counter-clockwise seen from its front).
    fn quad(&mut self, [a, b, c, d]: [Vec3; 4], color: Color) {
        self.tri([a, b, c], color);
        self.tri([a, c, d], color);
    }

    fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_computed_flat_normals()
    }
}

/// A torch flame; flickers its light.
#[derive(Component)]
struct Torch {
    phase: f32,
}

const TORCH_INTENSITY: f32 = 170_000.0;

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

fn build_glade(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = Lcg(7);
    let m = map();

    commands.spawn((
        DirectionalLight { color: SUN, illuminance: 3_600.0, shadow_maps_enabled: true, ..default() },
        // Low dusk sun from the upper left, so shadows are long and fall toward the camera's right.
        Transform::from_xyz(-32.0, 14.0, -18.0).looking_at(Vec3::ZERO, Vec3::Y),
        // Enough to cover what the camera sees at full zoom-out; the far forest needs no shadows.
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: 50.0, ..default() }.build(),
    ));

    let vertex_colored = materials.add(matte(Color::WHITE));
    commands.spawn((Mesh3d(meshes.add(terrain_mesh(&mut rng))), MeshMaterial3d(vertex_colored.clone())));
    commands.spawn((Mesh3d(meshes.add(ground_mesh(&mut rng))), MeshMaterial3d(vertex_colored.clone())));
    commands.spawn((
        Mesh3d(meshes.add(water_mesh())),
        MeshMaterial3d(materials.add(StandardMaterial { perceptual_roughness: 0.35, reflectance: 0.5, ..matte(POND) })),
    ));

    let mut props = Props::new();
    let wall_tops = props.walls(&mut rng);
    props.bridge(&mut rng);
    props.fords(&mut rng);
    for (t, tile) in m.tiles() {
        let c = Map::center(t);
        match tile {
            Tile::Rock => props.boulder(&mut rng, c),
            Tile::Tree => props.tree(&mut rng, c, 0.0, 1.25),
            Tile::Grass if rng.next() < 0.02 => props.flower(&mut rng, c),
            _ => {}
        }
    }
    // Forest ring: dense trees outside the clearing, kept out of the river.
    for _ in 0..320 {
        let p = Vec2::new(rng.range(-58.0, 58.0), rng.range(-40.0, 40.0));
        if clearing_margin(p) > -1.2 || (p.x - river_x(p.y)).abs() < RIVER_HALF_WIDTH + 1.5 {
            continue;
        }
        let scale = rng.range(0.9, 1.8);
        props.tree(&mut rng, p, ground_height(p), scale);
    }

    // Torches on the bridge and the ruins (posts go into the prop mesh; flames and lights don't).
    let flame = meshes.add(faceted(Cone::new(0.16, 0.42).mesh().resolution(5).build()));
    let flame_mat = materials.add(glow(TORCH_FLAME, 8.0));
    for (i, &p) in m.torches.iter().enumerate() {
        let base = wall_tops.get(&Map::tile_of(p)).copied().unwrap_or(0.0);
        let post_height = if base > 0.0 { 0.5 } else { 1.5 };
        props.add(Shape::Post, BARK, Transform::from_translation(to_world(p, base + post_height / 2.0)).with_scale(Vec3::new(1.0, post_height, 1.0)));
        commands
            .spawn((
                Torch { phase: i as f32 * 1.7 },
                PointLight { color: TORCH_LIGHT, intensity: TORCH_INTENSITY, range: 12.0, shadow_maps_enabled: false, ..default() },
                Transform::from_translation(to_world(p, base + post_height + 0.5)),
            ))
            .with_child((Mesh3d(flame.clone()), MeshMaterial3d(flame_mat.clone()), Transform::from_xyz(0.0, -0.3, 0.0)));
    }

    // All static props as one mesh: ~1,200 trees, stones and wall blocks draw as one object.
    commands.spawn((Mesh3d(meshes.add(props.into_mesh())), MeshMaterial3d(vertex_colored)));
}

#[derive(Clone, Copy)]
enum Shape {
    Rock,
    Trunk,
    Cone(usize),
    Block,
    Flower,
    Post,
}

/// Builds every static prop into one vertex-colored mesh.
struct Props {
    rock: Mesh,
    trunk: Mesh,
    cones: [Mesh; 3],
    block: Mesh,
    flower: Mesh,
    post: Mesh,
    out: Option<Mesh>,
}

impl Props {
    fn new() -> Self {
        Props {
            rock: faceted(Sphere::new(0.6).mesh().ico(0).unwrap()),
            trunk: faceted(Cylinder::new(0.22, 1.2).mesh().resolution(5).build()),
            cones: [0, 1, 2].map(|k| faceted(Cone::new(1.5 - k as f32 * 0.35, 1.7).mesh().resolution(7).build())),
            block: faceted(Cuboid::new(1.0, 1.0, 1.0).mesh().build()),
            flower: projectile_mesh(0.14),
            post: faceted(Cylinder::new(0.08, 1.0).mesh().resolution(5).build()),
            out: None,
        }
    }

    fn add(&mut self, shape: Shape, color: Color, t: Transform) {
        let base = match shape {
            Shape::Rock => &self.rock,
            Shape::Trunk => &self.trunk,
            Shape::Cone(k) => &self.cones[k],
            Shape::Block => &self.block,
            Shape::Flower => &self.flower,
            Shape::Post => &self.post,
        };
        let mut mesh = base.clone().transformed_by(t);
        let c = color.to_linear();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[c.red, c.green, c.blue, 1.0]; mesh.count_vertices()]);
        match &mut self.out {
            Some(out) => out.merge(&mesh).expect("prop meshes share attributes"),
            None => self.out = Some(mesh),
        }
    }

    fn into_mesh(self) -> Mesh {
        self.out.expect("the glade has props")
    }

    /// Ruined walls: stacked stone blocks of uneven height, some with a block on top. Returns the
    /// top height per wall tile, so torches can stand on them.
    fn walls(&mut self, rng: &mut Lcg) -> HashMap<IVec2, f32> {
        let mut tops = HashMap::default();
        for (t, tile) in map().tiles() {
            if tile != Tile::Wall {
                continue;
            }
            let c = Map::center(t);
            let h = rng.range(0.9, 1.7);
            let color = WALL.mix(&STONE, rng.range(0.0, 0.25));
            self.add(Shape::Block, color, Transform::from_translation(to_world(c, h / 2.0)).with_scale(Vec3::new(1.0, h, 1.0)));
            let mut top = h;
            if rng.next() < 0.45 {
                let s = rng.range(0.45, 0.7);
                let off = Vec2::new(rng.range(-0.2, 0.2), rng.range(-0.2, 0.2));
                self.add(Shape::Block, color, Transform::from_translation(to_world(c + off, h + s / 2.0)).with_scale(Vec3::splat(s)));
                top += s;
            }
            tops.insert(t, top);
            // Rubble at the foot, below knee height.
            for _ in 0..2 {
                let p = c + Vec2::new(rng.range(-0.9, 0.9), rng.range(-0.9, 0.9));
                if map().walkable_at(p) {
                    self.add(Shape::Rock, SLATE, Transform::from_translation(to_world(p, 0.05)).with_scale(Vec3::splat(rng.range(0.15, 0.3))));
                }
            }
        }
        tops
    }

    /// The stone bridge: paving slabs on the bridge tiles, parapets along both sides, and piers
    /// down into the river.
    fn bridge(&mut self, rng: &mut Lcg) {
        let tiles: Vec<Vec2> = map().tiles().filter(|(_, t)| *t == Tile::Bridge).map(|(t, _)| Map::center(t)).collect();
        for &c in &tiles {
            let color = if rng.next() < 0.5 { STONE } else { SLATE };
            self.add(Shape::Block, color, Transform::from_translation(to_world(c, -0.18)).with_scale(Vec3::new(0.97, 0.36, 0.97)));
        }
        let span = tiles.iter().map(|c| Rect::from_center_size(*c, Vec2::ONE)).reduce(|a, b| a.union(b)).expect("the map has a bridge");
        let mid = span.center();
        for y in [span.min.y - 0.18, span.max.y + 0.18] {
            // Parapet, with a cap stone on each end.
            let parapet = Transform::from_translation(to_world(Vec2::new(mid.x, y), 0.25)).with_scale(Vec3::new(span.width() + 0.8, 0.7, 0.36));
            self.add(Shape::Block, WALL.mix(&STONE, 0.12), parapet);
            for x in [span.min.x - 0.3, span.max.x + 0.3] {
                self.add(Shape::Block, WALL.mix(&STONE, 0.24), Transform::from_translation(to_world(Vec2::new(x, y), 0.45)).with_scale(Vec3::new(0.5, 1.1, 0.5)));
            }
        }
        for x in [mid.x - 1.2, mid.x + 1.2] {
            let pier = Transform::from_translation(to_world(Vec2::new(x, mid.y), (RIVERBED_LEVEL - 0.36) / 2.0))
                .with_scale(Vec3::new(0.8, -RIVERBED_LEVEL, span.height()));
            self.add(Shape::Block, WALL, pier);
        }
    }

    /// Stepping stones across the fords.
    fn fords(&mut self, rng: &mut Lcg) {
        for (t, tile) in map().tiles() {
            if tile != Tile::Ford {
                continue;
            }
            for _ in 0..2 {
                let p = Map::center(t) + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
                let s = rng.range(0.55, 0.75);
                let stone = Transform::from_translation(to_world(p, -0.12))
                    .with_scale(Vec3::new(s, 0.3, s))
                    .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)));
                self.add(Shape::Rock, STONE, stone);
            }
        }
    }

    fn boulder(&mut self, rng: &mut Lcg, c: Vec2) {
        let t = Transform::from_translation(to_world(c, 0.25))
            .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
            .with_scale(Vec3::new(rng.range(1.2, 1.5), rng.range(0.9, 1.3), rng.range(1.2, 1.5)));
        self.add(Shape::Rock, SLATE, t);
    }

    fn tree(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        self.add(Shape::Trunk, BARK, Transform::from_translation(to_world(p, ground + 0.6 * s)).with_scale(Vec3::splat(s)));
        let tiers = 2 + (rng.next() * 2.0) as usize;
        for k in 0..tiers {
            let t = Transform::from_translation(to_world(p, ground + (1.5 + k as f32 * 0.85) * s))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::splat(s));
            self.add(Shape::Cone(k), if k % 2 == 0 { FERN } else { PINE }, t);
        }
    }

    fn flower(&mut self, rng: &mut Lcg, c: Vec2) {
        let p = c + Vec2::new(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4));
        self.add(Shape::Flower, HEATHER, Transform::from_translation(to_world(p, 0.1)));
    }
}

/// Height of the faceted terrain outside the clearing (0-ish inside it).
fn ground_height(p: Vec2) -> f32 {
    let margin = clearing_margin(p);
    if margin >= 0.0 { -0.05 } else { (-margin * 0.3).min(3.0) }
}

/// The rolling forest ring around the clearing, with the river channel cut through it.
fn terrain_mesh(rng: &mut Lcg) -> Mesh {
    const SIZE: Vec2 = Vec2::new(124.0, 88.0);
    const CELLS: (usize, usize) = (62, 44);
    let corner = |i: usize, j: usize| Vec2::new(-SIZE.x / 2.0 + SIZE.x * i as f32 / CELLS.0 as f32, -SIZE.y / 2.0 + SIZE.y * j as f32 / CELLS.1 as f32);
    let mut grid = Vec::with_capacity((CELLS.0 + 1) * (CELLS.1 + 1));
    for j in 0..=CELLS.1 {
        for i in 0..=CELLS.0 {
            let mut p = corner(i, j);
            let in_channel = (p.x - river_x(p.y)).abs() < RIVER_HALF_WIDTH + 1.2;
            let rough = clearing_margin(p) < -1.0 && !in_channel;
            if rough {
                p += Vec2::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5));
            }
            let h = if in_channel { RIVERBED_LEVEL } else { ground_height(p) + if rough { rng.range(-0.3, 0.6) } else { 0.0 } };
            grid.push(to_world(p, h));
        }
    }
    let at = |i: usize, j: usize| grid[j * (CELLS.0 + 1) + i];
    let mut b = FlatMesh::default();
    for j in 0..CELLS.1 {
        for i in 0..CELLS.0 {
            let (a, r, u, ru) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            for tri in [[a, r, u], [r, ru, u]] {
                let height = (tri[0].y + tri[1].y + tri[2].y) / 3.0;
                let pick = rng.next();
                let base = if height < -0.3 {
                    RIVERBED
                } else if height < 0.05 {
                    MOSS
                } else if height > 1.4 {
                    if pick < 0.5 { FERN } else { PINE }
                } else if pick < 0.15 {
                    PATH
                } else if pick < 0.6 {
                    MOSS
                } else {
                    FERN
                };
                b.tri(tri, Color::from(base.to_linear() * (1.0 + rng.range(-0.04, 0.04))));
            }
        }
    }
    b.build()
}

/// The walkable ground of the clearing: one quad per tile, one color per tile (a faint checker
/// keeps the grid readable), plus earthen banks where the ground meets the river.
fn ground_mesh(rng: &mut Lcg) -> Mesh {
    let m = map();
    let mut b = FlatMesh::default();
    for (t, tile) in m.tiles() {
        if matches!(tile, Tile::Water | Tile::Bridge | Tile::Ford | Tile::Forest) {
            continue;
        }
        let c = Map::center(t);
        let checker = if (t.x + t.y) % 2 == 0 { 1.03 } else { 0.97 };
        let base = match tile {
            Tile::Path => PATH.mix(&MOSS, rng.next() * 0.25),
            _ => MOSS.mix(&MEADOW, rng.next() * 0.25),
        };
        let color = Color::from(base.to_linear() * checker);
        let (lo, hi) = (c - Vec2::splat(0.5), c + Vec2::splat(0.5));
        b.quad([to_world(lo, 0.0), to_world(Vec2::new(hi.x, lo.y), 0.0), to_world(hi, 0.0), to_world(Vec2::new(lo.x, hi.y), 0.0)], color);
        // Bank faces toward river tiles.
        for (dir, a, z) in [
            (IVec2::X, Vec2::new(hi.x, lo.y), Vec2::new(hi.x, hi.y)),
            (-IVec2::X, Vec2::new(lo.x, hi.y), Vec2::new(lo.x, lo.y)),
            (IVec2::Y, Vec2::new(hi.x, hi.y), Vec2::new(lo.x, hi.y)),
            (-IVec2::Y, Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y)),
        ] {
            if matches!(m.get(t + dir), Tile::Water | Tile::Ford) {
                b.quad([to_world(a, 0.0), to_world(a, RIVERBED_LEVEL), to_world(z, RIVERBED_LEVEL), to_world(z, 0.0)], BANK);
            }
        }
    }
    b.build()
}

/// The river surface, following its centerline through the whole map and into the forest.
fn water_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    let half = RIVER_HALF_WIDTH + 1.2;
    let extent = MAP_HALF_EXTENTS.y + 22.0;
    let mut y = -extent;
    while y < extent {
        let y2 = y + 1.0;
        let (x1, x2) = (river_x(y), river_x(y2));
        b.quad(
            [
                to_world(Vec2::new(x1 - half, y), WATER_LEVEL),
                to_world(Vec2::new(x1 + half, y), WATER_LEVEL),
                to_world(Vec2::new(x2 + half, y2), WATER_LEVEL),
                to_world(Vec2::new(x2 - half, y2), WATER_LEVEL),
            ],
            Color::WHITE,
        );
        y = y2;
    }
    b.build()
}

#[cfg(test)]
mod tests {
    use arena_shared::protocol::ClassId;

    use super::FIGHTER_LOOKS;

    #[test]
    fn every_class_has_its_own_figure() {
        for class in ClassId::all() {
            let key = &class.def().id;
            assert!(FIGHTER_LOOKS.contains(&key.as_str()), "class {key:?} has no figure in fighter_mesh");
        }
    }
}
