//! What items and monsters do, drawn (`Fx`, from the server): each a little scene of its own, in
//! the manner of the revenant's rift (`swish/rift.rs`), not a marker: frost growing over the
//! ground, earth scorched and cracked glowing, flames licking out, embers and chunks thrown,
//! smoke rising, lightning, a vortex, thorns bursting up. An effect arriving spawns its parts
//! (`recipes.rs`), each living on by itself (a scorch mark lingers after its blast): appearing,
//! holding, fading, moving as it's thrown or whirled, then gone. Their shapes are in
//! `shapes.rs`, colored by their `Ink`; they're all drawn together, laid out anew every frame
//! (`draw_parts`) into buffers kept on the GPU (`draw.rs`), so hundreds of them cost one draw.

use std::f32::consts::{FRAC_PI_2, TAU};

use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::camera::primitives::{Frustum, Sphere as Ball};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::arena::{self, palette};
use crate::feedback::AttackClock;

mod death;
mod downed;
mod draw;
mod recipes;
mod shapes;

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Parts>();
        draw::plugin(app);
        app.add_systems(Startup, load_art);
        app.add_systems(Update, (recipes::play_new, strike_fx, burning_flames, downed::mark_downed, death::burst_deaths, death::spark_hits));
        // Sorted from the camera once it has moved for the frame.
        app.add_systems(PostUpdate, draw_parts.after(TransformSystems::Propagate));
    }
}

/// How often (seconds) a burning fighter sheds a new round of flames.
const FLAME_EVERY: f32 = 0.08;

/// Fire on the burning (`Burning`): tongues of flame licking up off them, more the more stacks,
/// and embers rising, for as long as the fire lasts, on the timeline each fighter is drawn on.
fn burning_flames(
    mut commands: Commands,
    time: Res<Time>,
    clock: AttackClock,
    art: Res<FxArt>,
    fighters: Query<(&Pos, &Burning, Has<crate::Me>)>,
    mut next: Local<f32>,
) {
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + FLAME_EVERY;
    for (pos, burning, is_me) in &fighters {
        let stacks = burning.at(clock.now(is_me));
        if stacks == 0 {
            continue;
        }
        let at = arena::to_world(pos.0, 0.0);
        let seed = (now * 997.0) as u32 ^ pos.0.x.to_bits().rotate_left(7) ^ pos.0.y.to_bits();
        for i in 0..2 + 2 * stacks {
            let r = |n: u32| crate::swish::noise(seed, i * 8 + n);
            let angle = r(1) * TAU;
            let out = 0.12 + 0.25 * r(2);
            let from = at + Vec3::new(angle.cos() * out, 0.1 + 1.0 * r(3), -angle.sin() * out);
            // The tongue lies along +X: stood up, leaning a little, turned any way.
            let lick = Quat::from_rotation_y(r(4) * TAU) * Quat::from_rotation_z(FRAC_PI_2 - 0.4 * r(5));
            let length = 0.35 + 0.35 * r(6);
            let rise = Motion::Thrown { velocity: Vec3::Y * (1.0 + 0.7 * r(7)), gravity: -0.6, tumble: Vec3::ZERO };
            let tongue = Part::new(now, [0.05, 0.18, 0.45], Ink::Flame, from).sized(Vec3::new(length, 1.0, length * 0.6));
            put(&mut commands, &art.tongue, tongue.turn(lick).grow(0.3).shrink(0.2).flicker(0.3).motion(rise));
            if r(0) < 0.4 {
                let ember = Motion::Thrown { velocity: Vec3::new(r(1) - 0.5, 1.5 + r(2), r(3) - 0.5), gravity: -0.3, tumble: Vec3::ZERO };
                put(&mut commands, &art.mote, Part::new(now, [0.02, 0.3, 0.7], Ink::Spark, from + Vec3::Y * 0.3).size(0.03 + 0.02 * r(4)).motion(ember).flicker(0.5).shrink(0.2));
            }
        }
    }
}

/// Each hero's auto-attack lands with a contact effect of its own, where its swing went
/// (`LastSwing`): the warden's shield shoving the air ahead of it, the breaker's maul breaking
/// the ground. Once per swing, by the swing's own tick, never by the component's change flag
/// (replication marks it changed every update).
fn strike_fx(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<FxArt>,
    swings: Query<(Entity, &Pos, &ClassId, &LastSwing), (Changed<LastSwing>, With<PlayerId>)>,
    mut seen: Local<bevy::platform::collections::HashMap<Entity, u32>>,
) {
    let now = time.elapsed_secs();
    for (entity, pos, class, swing) in &swings {
        // The first sighting only notes the swing: it may be long past.
        match seen.insert(entity, swing.tick) {
            Some(before) if before != swing.tick => {}
            _ => continue,
        }
        let dir = swing.dir.normalize_or(Vec2::X);
        match class.def().figure() {
            "warden" => recipes::shield_push(&mut commands, &art, now, pos.0, dir, swing.tick),
            "breaker" => recipes::tremor(&mut commands, &art, now, pos.0, dir, swing.tick),
            _ => {}
        }
    }
}

/// A nova going off at `at` (`radius` round) now, cast by a figure of `figure`, drawn as its own
/// burst, if it has one: the pyre witch's ring of cinders, the breaker's and the warden's quake.
/// Whether it did (the frost mage's, and anyone else's, is a burst of ice: `render::show_novas`).
pub(crate) fn cast_nova(commands: &mut Commands, art: &FxArt, now: f32, figure: &str, at: Vec2, radius: f32, seed: u32) -> bool {
    match figure {
        "pyre_witch" => recipes::cinder_ring(commands, art, now, at, radius, seed),
        "breaker" | "warden" => recipes::quake(commands, art, now, at, radius, seed),
        _ => return false,
    }
    true
}

/// How many steps a fading material goes through, from its full opacity to clear.
const FADE_STEPS: usize = 12;

/// What a part is drawn in: a color, how see-through at most, how brightly it glows, and which
/// layer it's drawn on among others in the same place (higher over lower). (The glow is for
/// the materials of `FxArt::ink`: the parts themselves are drawn unlit, as those are.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Ink {
    /// Scorched earth, soot, ash.
    Scorch,
    Smoke,
    /// Clods of earth.
    Earth,
    /// The glow in a burn's cracks, embers, a flame's heart, sparks.
    Ember,
    Flame,
    Spark,
    /// Ice crystals, and the thin glaze of frost under them; light catching on them.
    Frost,
    Glaze,
    Glint,
    Thorn,
    /// A thorn itself: pale bark, catching the light.
    Bark,
    Spirit,
    SpiritMist,
    Void,
    VoidLight,
    VoidMist,
    Storm,
    StormMist,
    Gold,
    Poison,
    PoisonLight,
    PoisonMist,
    Blood,
    /// Bone chips and splinters off the slain.
    Bone,
    Heal,
    /// A heal's glow pooled on the ground, faint.
    HealMist,
    Wisp,
    Haste,
    /// Dust and grit kicked up: pale, see-through.
    Dust,
}

impl Ink {
    const ALL: [Ink; 29] = [
        Ink::Scorch,
        Ink::Smoke,
        Ink::Earth,
        Ink::Ember,
        Ink::Flame,
        Ink::Spark,
        Ink::Frost,
        Ink::Glaze,
        Ink::Glint,
        Ink::Thorn,
        Ink::Bark,
        Ink::Spirit,
        Ink::SpiritMist,
        Ink::Void,
        Ink::VoidLight,
        Ink::VoidMist,
        Ink::Storm,
        Ink::StormMist,
        Ink::Gold,
        Ink::Poison,
        Ink::PoisonLight,
        Ink::PoisonMist,
        Ink::Blood,
        Ink::Bone,
        Ink::Heal,
        Ink::HealMist,
        Ink::Wisp,
        Ink::Haste,
        Ink::Dust,
    ];

    /// (color, opacity at most, glow, layer).
    fn look(self) -> (Color, f32, f32, u8) {
        let rgb = Color::srgb_u8;
        match self {
            Ink::Scorch => (rgb(0x1C, 0x15, 0x12), 0.82, 0.0, 0),
            Ink::Smoke => (rgb(0x2E, 0x2A, 0x28), 0.32, 0.0, 4),
            Ink::Earth => (rgb(0x3E, 0x31, 0x26), 1.0, 0.0, 1),
            Ink::Ember => (palette::TORCH_FLAME, 0.95, 5.0, 2),
            Ink::Flame => (rgb(0xFF, 0xC4, 0x5C), 0.8, 6.0, 3),
            Ink::Spark => (rgb(0xFF, 0xE6, 0xB0), 1.0, 8.0, 5),
            Ink::Frost => (palette::ICE, 0.8, 2.4, 2),
            Ink::Glaze => (palette::RIME, 0.32, 1.0, 1),
            Ink::Glint => (Color::WHITE, 1.0, 9.0, 5),
            Ink::Thorn => (rgb(0x3A, 0x48, 0x22), 1.0, 0.3, 2),
            Ink::Bark => (rgb(0xB4, 0xA0, 0x6A), 1.0, 0.6, 3),
            Ink::Spirit => (palette::SPIRIT, 0.9, 4.0, 3),
            Ink::SpiritMist => (palette::SPIRIT, 0.22, 2.0, 1),
            Ink::Void => (rgb(0x16, 0x0C, 0x1E), 0.85, 0.0, 1),
            Ink::VoidLight => (rgb(0xA0, 0x60, 0xFF), 0.9, 5.0, 3),
            Ink::VoidMist => (rgb(0x9A, 0x5C, 0xF0), 0.38, 3.0, 2),
            Ink::Storm => (rgb(0xDC, 0xEE, 0xFF), 1.0, 10.0, 5),
            Ink::StormMist => (rgb(0x8C, 0xB8, 0xFF), 0.3, 3.0, 2),
            Ink::Gold => (palette::GOLD, 1.0, 2.5, 3),
            Ink::Poison => (rgb(0x5A, 0x86, 0x22), 0.75, 0.6, 1),
            Ink::PoisonLight => (rgb(0xA4, 0xE6, 0x54), 0.9, 3.0, 3),
            Ink::PoisonMist => (rgb(0x8C, 0xC8, 0x50), 0.22, 1.5, 1),
            Ink::Blood => (rgb(0x6E, 0x0E, 0x16), 0.85, 0.4, 1),
            Ink::Bone => (rgb(0xD8, 0xCE, 0xB0), 1.0, 0.1, 2),
            Ink::Heal => (palette::HEAL, 0.9, 4.0, 3),
            Ink::HealMist => (palette::HEAL, 0.22, 2.0, 1),
            Ink::Wisp => (palette::WISP, 0.85, 6.0, 4),
            Ink::Haste => (palette::HASTE, 0.9, 5.0, 4),
            Ink::Dust => (rgb(0xC6, 0xB6, 0x96), 0.5, 0.4, 4),
        }
    }
}

/// How many of each broken shape there are to pick from, so no two effects look the same.
const VARIANTS: u32 = 4;

/// Everything effects are drawn with.
#[derive(Resource)]
pub(crate) struct FxArt {
    /// Each ink's materials, full first, clearing in `FADE_STEPS`.
    inks: Vec<Vec<Handle<StandardMaterial>>>,
    splotch: Vec<Handle<Mesh>>,
    streaks: Vec<Handle<Mesh>>,
    cracks: Vec<Handle<Mesh>>,
    frost: Vec<Handle<Mesh>>,
    whirl: Vec<Handle<Mesh>>,
    band: Handle<Mesh>,
    /// A cross lying flat (`shapes::cross`): where the downed lie.
    cross: Handle<Mesh>,
    /// A round glow lying flat (`shapes::disc`).
    disc: Handle<Mesh>,
    pub(crate) tongue: Handle<Mesh>,
    glint: Handle<Mesh>,
    /// Solid bits: a chunk, a mote, a coin, a keg, an ice shard (leaning out along +X, 1 tall),
    /// a thorn (its base at the origin, 1 tall).
    chunk: Handle<Mesh>,
    mote: Handle<Mesh>,
    coin: Handle<Mesh>,
    keg: Handle<Mesh>,
    shard: Handle<Mesh>,
    thorn: Handle<Mesh>,
}

impl FxArt {
    fn ink(&self, ink: Ink, faded: f32) -> &Handle<StandardMaterial> {
        let steps = &self.inks[ink as usize];
        &steps[((faded * FADE_STEPS as f32) as usize).min(FADE_STEPS - 1)]
    }
}

fn load_art(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let inks = Ink::ALL
        .iter()
        .map(|ink| {
            let (color, alpha, glow, layer) = ink.look();
            (0..FADE_STEPS)
                .map(|i| {
                    let faded = alpha * (1.0 - i as f32 / FADE_STEPS as f32);
                    materials.add(StandardMaterial { depth_bias: layer as f32, cull_mode: None, ..arena::translucent(color, faded, glow) })
                })
                .collect()
        })
        .collect();
    let mut variants = |make: &dyn Fn(u32) -> Mesh| (0..VARIANTS).map(|seed| meshes.add(make(seed * 7919 + 13))).collect::<Vec<_>>();
    let splotch = variants(&|seed| shapes::splotch(seed, 0.5));
    let streaks = variants(&|seed| shapes::streaks(seed, 14, 0.05));
    let cracks = variants(&|seed| shapes::cracks(seed, 7));
    let frost = variants(&|seed| shapes::frost(seed));
    let whirl = variants(&|seed| shapes::whirl(seed, 6, 0.045));
    commands.insert_resource(FxArt {
        inks,
        splotch,
        streaks,
        cracks,
        frost,
        whirl,
        band: meshes.add(shapes::band(0.3)),
        cross: meshes.add(shapes::cross(0.32)),
        disc: meshes.add(shapes::disc()),
        tongue: meshes.add(shapes::tongue()),
        glint: meshes.add(shapes::glint()),
        chunk: meshes.add(Cuboid::new(1.0, 0.7, 0.85)),
        mote: meshes.add(Sphere::new(0.5).mesh().ico(1).expect("a small ico sphere")),
        coin: meshes.add(Cylinder::new(0.5, 0.12).mesh().resolution(10).build()),
        keg: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(8).build()),
        shard: meshes.add(arena::nova_meshes(1.0).shard),
        thorn: meshes.add(Cone { radius: 0.5, height: 1.0 }.mesh().resolution(5).build().translated_by(Vec3::Y * 0.5)),
    });
}

/// How a part moves.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Motion {
    Still,
    /// Thrown from where it starts at `velocity` (m/s), falling at `gravity` (m/s², negative to
    /// rise), tumbling (rad/s); lying still once it lands.
    Thrown { velocity: Vec3, gravity: f32, tumble: Vec3 },
    /// Round `center` (in the world) at `speed` (rad/s) from `angle`, its radius and height going
    /// from the first to the second over its life.
    Orbit { center: Vec3, angle: f32, speed: f32, radius: (f32, f32), height: (f32, f32) },
    /// Flying from `from` to `to` (in the world) in an arc `height` high, landing as it starts
    /// to fade, tumbling.
    Arc { from: Vec3, to: Vec3, height: f32, tumble: Vec3 },
}

/// A part of an effect: from `born` (seconds), in `ink`; fully in `life[0]` later, fading from
/// `life[1]`, gone at `life[2]`. It grows in from `grow` times its `size` and shrinks to `shrink`
/// times it as it fades, turns `spin` (rad/s) about the up, flickers by up to `flicker` of its
/// opacity, and moves by `motion` from `at` (in the world). Its shape is `mesh` (set by `put`),
/// or with `frames`, it flicks through them.
#[derive(Clone, Debug)]
pub(crate) struct Part {
    mesh: Handle<Mesh>,
    born: f32,
    life: [f32; 3],
    ink: Ink,
    at: Vec3,
    turn: Quat,
    size: Vec3,
    grow: f32,
    shrink: f32,
    spin: f32,
    flicker: f32,
    motion: Motion,
    frames: Vec<Handle<Mesh>>,
}

impl Part {
    pub(crate) fn new(born: f32, life: [f32; 3], ink: Ink, at: Vec3) -> Part {
        Part { mesh: Handle::default(), born, life, ink, at, turn: Quat::IDENTITY, size: Vec3::ONE, grow: 1.0, shrink: 1.0, spin: 0.0, flicker: 0.0, motion: Motion::Still, frames: Vec::new() }
    }

    pub(crate) fn size(self, size: f32) -> Part {
        Part { size: Vec3::splat(size), ..self }
    }

    pub(crate) fn sized(self, size: Vec3) -> Part {
        Part { size, ..self }
    }

    pub(crate) fn turn(self, turn: Quat) -> Part {
        Part { turn, ..self }
    }

    /// Turned about the up by `angle`.
    pub(crate) fn facing(self, angle: f32) -> Part {
        Part { turn: Quat::from_rotation_y(angle), ..self }
    }

    pub(crate) fn grow(self, grow: f32) -> Part {
        Part { grow, ..self }
    }

    pub(crate) fn shrink(self, shrink: f32) -> Part {
        Part { shrink, ..self }
    }

    pub(crate) fn spin(self, spin: f32) -> Part {
        Part { spin, ..self }
    }

    pub(crate) fn flicker(self, flicker: f32) -> Part {
        Part { flicker, ..self }
    }

    pub(crate) fn motion(self, motion: Motion) -> Part {
        Part { motion, ..self }
    }

    pub(crate) fn frames(self, frames: Vec<Handle<Mesh>>) -> Part {
        Part { frames, ..self }
    }

    /// Where it is, how it's turned, and how big, `age` seconds into its life.
    fn place(&self, age: f32) -> Transform {
        let [full, fading, gone] = self.life;
        let ease_out = |t: f32| 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3);
        let scale = if age < full {
            self.grow + (1.0 - self.grow) * ease_out(age / full.max(1e-3))
        } else if age > fading {
            1.0 + (self.shrink - 1.0) * ((age - fading) / (gone - fading).max(1e-3)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let size = self.size * scale;
        let spun = self.turn * Quat::from_rotation_y(self.spin * age);
        let (translation, rotation) = match self.motion {
            Motion::Still => (self.at, spun),
            Motion::Thrown { velocity, gravity, tumble } => {
                let flown = self.at + velocity * age - Vec3::Y * gravity * age * age / 2.0;
                let floor = size.y / 2.0;
                if flown.y <= floor && gravity > 0.0 {
                    // Landed: lying where it came down, as it last turned.
                    let land = landing(self.at.y - floor, velocity.y, gravity);
                    (Vec3::new(flown.x - velocity.x * (age - land), floor, flown.z - velocity.z * (age - land)), spun * Quat::from_scaled_axis(tumble * land))
                } else {
                    (flown, spun * Quat::from_scaled_axis(tumble * age))
                }
            }
            Motion::Orbit { center, angle, speed, radius, height } => {
                let t = (age / gone.max(1e-3)).clamp(0.0, 1.0);
                let a = angle + speed * age;
                let r = radius.0 + (radius.1 - radius.0) * t;
                (center + Vec3::new(a.cos() * r, height.0 + (height.1 - height.0) * t, -a.sin() * r), spun * Quat::from_rotation_y(a))
            }
            Motion::Arc { from, to, height, tumble } => {
                let t = (age / fading.max(1e-3)).clamp(0.0, 1.0);
                (from.lerp(to, t) + Vec3::Y * height * 4.0 * t * (1.0 - t), spun * Quat::from_scaled_axis(tumble * age))
            }
        };
        Transform { translation, rotation, scale: size }
    }

    /// How opaque it is, `age` seconds into its life (0 to 1).
    fn opacity(&self, age: f32) -> f32 {
        let [full, fading, gone] = self.life;
        if age < full {
            (age / full.max(1e-3)).clamp(0.0, 1.0)
        } else if age > fading {
            1.0 - ((age - fading) / (gone - fading).max(1e-3)).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}

/// When something thrown up from `above` (meters over its floor) at `up` (m/s) comes down,
/// falling at `gravity`.
fn landing(above: f32, up: f32, gravity: f32) -> f32 {
    (up + (up * up + 2.0 * gravity * above.max(0.0)).sqrt()) / gravity
}

/// Every part in the world (`put` there), until it's gone: played out and drawn by `draw_parts`.
#[derive(Resource, Default)]
pub(crate) struct Parts(pub(crate) Vec<Part>);

/// Puts a part in the world, `mesh` drawn in its ink (from when it's born).
pub(crate) fn put(commands: &mut Commands, mesh: &Handle<Mesh>, part: Part) {
    let part = Part { mesh: mesh.clone(), ..part };
    commands.queue(move |world: &mut World| world.resource_mut::<Parts>().0.push(part));
}

/// A part's shape as `draw_parts` copies it: its corners, how opaque each is (white, colored by
/// the ink), its triangles, and the ball it fits in. Each corner is in it once, however many
/// triangles share it (the shapes are made of triangles of their own, and facing both ways).
struct Shape {
    positions: Vec<Vec3>,
    colors: Vec<Vec4>,
    indices: Vec<u32>,
    center: Vec3,
    radius: f32,
}

impl Shape {
    fn of(mesh: &Mesh) -> Option<Shape> {
        let Ok(VertexAttributeValues::Float32x3(positions)) = mesh.try_attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
        let positions: Vec<Vec3> = positions.iter().map(|&p| Vec3::from_array(p)).collect();
        let colors = match mesh.try_attribute(Mesh::ATTRIBUTE_COLOR) {
            Ok(VertexAttributeValues::Float32x4(colors)) => colors.iter().map(|&c| Vec4::from_array(c)).collect(),
            _ => vec![Vec4::ONE; positions.len()],
        };
        let corners: Vec<usize> = match mesh.try_indices_option().ok()? {
            Some(indices) => indices.iter().collect(),
            None => (0..positions.len()).collect(),
        };
        let mut seen: HashMap<([u32; 3], [u32; 4]), u32> = HashMap::default();
        let (mut unique, mut unique_colors) = (Vec::new(), Vec::new());
        let indices = corners
            .into_iter()
            .map(|i| {
                let key = (positions[i].to_array().map(f32::to_bits), colors[i].to_array().map(f32::to_bits));
                *seen.entry(key).or_insert_with(|| {
                    unique.push(positions[i]);
                    unique_colors.push(colors[i]);
                    unique.len() as u32 - 1
                })
            })
            .collect();
        let (positions, colors) = (unique, unique_colors);
        let (low, high) = positions.iter().fold((Vec3::MAX, Vec3::MIN), |(low, high), &p| (low.min(p), high.max(p)));
        let center = (low + high) / 2.0;
        let radius = positions.iter().map(|p| p.distance(center)).fold(0.0, f32::max);
        Some(Shape { positions, colors, indices, center, radius })
    }
}

/// Parts play out their lives (moving, growing and fading, flickering, gone at the end), laid
/// out together for drawing every frame (`draw::Drawn`): their shapes put where they are,
/// colored by their inks and hazed with distance as everything else is, nearest last, as Bevy
/// sorts see-through things drawn one by one (a higher layer counting as nearer). Those off
/// screen are left out.
#[allow(clippy::too_many_arguments)]
fn draw_parts(
    time: Res<Time>,
    mut parts: ResMut<Parts>,
    camera: Single<(&GlobalTransform, &Frustum, Option<&DistanceFog>), With<crate::camera::CameraRig>>,
    meshes: Res<Assets<Mesh>>,
    mut changed: MessageReader<AssetEvent<Mesh>>,
    mut shapes: Local<HashMap<AssetId<Mesh>, Shape>>,
    mut drawn: ResMut<draw::Drawn>,
    mut seed: Local<u32>,
) {
    for event in changed.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } | AssetEvent::Unused { id } = event {
            shapes.remove(id);
        }
    }
    let now = time.elapsed_secs();
    parts.0.retain(|part| now - part.born < part.life[2]);
    let mut random = || {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (1u32 << 24) as f32
    };
    let (eye, frustum, fog) = *camera;
    let depth = eye.to_matrix().inverse().row(2);
    let haze_color = fog.map_or(Vec3::ZERO, |fog| Vec4::from_array(fog.color.to_linear().to_f32_array()).truncate());
    let mut shown: Vec<(f32, AssetId<Mesh>, Affine3A, Vec4, f32)> = Vec::new();
    let mut middle = Vec3::ZERO;
    for part in &parts.0 {
        let age = now - part.born;
        if age < 0.0 {
            continue;
        }
        let (color, alpha, _, layer) = part.ink.look();
        let alpha = alpha * part.opacity(age) * (1.0 - part.flicker * random());
        let mesh = if part.frames.is_empty() { &part.mesh } else { &part.frames[(age * 24.0) as usize % part.frames.len()] };
        let id = mesh.id();
        if !shapes.contains_key(&id) {
            let Some(shape) = meshes.get(id).and_then(Shape::of) else { continue };
            shapes.insert(id, shape);
        }
        let shape = &shapes[&id];
        let placed = part.place(age);
        let center = placed.transform_point(shape.center);
        let ball = Ball { center: center.into(), radius: shape.radius * placed.scale.abs().max_element() };
        if alpha <= 0.0 || !frustum.intersects_sphere(&ball, false) {
            continue;
        }
        let tint = Vec4::from_array(color.to_linear().with_alpha(alpha).to_f32_array());
        // The haze worked out once for the whole part, small as each is.
        let haze = fog.map_or(0.0, |fog| draw::haze(fog, eye.translation().distance(center)));
        shown.push((depth.dot(center.extend(1.0)) + layer as f32, id, placed.compute_affine(), tint, haze));
        middle += center;
    }
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    let draw::Drawn { corners, triangles, center } = &mut *drawn;
    corners.clear();
    triangles.clear();
    for (_, id, placed, tint, haze) in &shown {
        let shape = &shapes[id];
        let first = corners.len() as u32;
        corners.extend(shape.positions.iter().zip(&shape.colors).map(|(&p, &c)| {
            let [x, y, z] = placed.transform_point3(p).to_array();
            let color = c * *tint;
            let [r, g, b] = color.truncate().lerp(haze_color, *haze).to_array();
            [x, y, z, r, g, b, color.w]
        }));
        triangles.extend(shape.indices.iter().map(|&i| first + i));
    }
    *center = middle / shown.len().max(1) as f32;
}

/// A wisp's wake: a mote of its light left `at` (in the world), drifting up as it fades.
pub(crate) fn wisp_wake(commands: &mut Commands, art: &FxArt, now: f32, at: Vec3) {
    let rise = Motion::Thrown { velocity: Vec3::Y * 0.3, gravity: -0.2, tumble: Vec3::ZERO };
    put(commands, &art.mote, Part::new(now, [0.0, 0.08, 0.4], Ink::Wisp, at).size(0.09).motion(rise).shrink(0.2));
}

/// What a pickup lying out gives off (`sparkle`).
#[derive(Clone, Copy)]
pub(crate) enum Sparkle {
    /// A heal: green sparks rising off the ground under it.
    Heal,
    /// A haste: motes of wind whipping round it.
    Haste,
    /// A fury stone: quick flashes of steel round its blades.
    Fury,
    /// A chest: light glinting off its gold.
    Gold,
    /// One burning (in a revenant's rift): a green flame licking up off its feet, and a mote
    /// rising off it.
    Burn,
}

/// One bit of `sparkle` off a pickup floating at `at` (its middle, in the world), the `k`th (for
/// where and how).
pub(crate) fn sparkle(commands: &mut Commands, art: &FxArt, now: f32, at: Vec3, sparkle: Sparkle, k: u32) {
    let r = |n: u32| crate::swish::noise(k, n);
    let round = |n: u32, out: f32| {
        let angle = r(n) * std::f32::consts::TAU;
        Vec3::new(angle.cos() * out, 0.0, -angle.sin() * out)
    };
    let part = match sparkle {
        Sparkle::Heal => {
            let from = at.with_y(0.1) + round(1, 0.55 * r(2).sqrt());
            let rise = Motion::Thrown { velocity: Vec3::Y * (0.7 + 0.6 * r(3)), gravity: -0.3, tumble: Vec3::ZERO };
            (art.mote.clone(), Part::new(now, [0.15, 0.7, 1.3], Ink::Heal, from).size(0.05 + 0.04 * r(4)).motion(rise).flicker(0.4).shrink(0.2))
        }
        Sparkle::Haste => {
            let motion = Motion::Orbit { center: at.with_y(0.0), angle: r(1) * std::f32::consts::TAU, speed: 7.0, radius: (0.6, 0.3), height: (at.y - 0.4 + 0.5 * r(2), at.y + 0.3) };
            (art.mote.clone(), Part::new(now, [0.05, 0.3, 0.5], Ink::Haste, at).size(0.045).motion(motion).flicker(0.3).shrink(0.3))
        }
        Sparkle::Fury => {
            let from = at + round(1, 0.45) + Vec3::Y * (0.5 * r(2) - 0.25);
            (art.glint.clone(), Part::new(now, [0.03, 0.05, 0.18], Ink::Spark, from).size(0.3 + 0.2 * r(3)).facing(r(4) * std::f32::consts::TAU).grow(0.3).shrink(0.2).spin(6.0))
        }
        Sparkle::Burn => {
            let from = at.with_y(0.05) + round(1, 0.3 * r(2).sqrt());
            let rise = Motion::Thrown { velocity: Vec3::Y * (0.8 + 0.5 * r(3)), gravity: -0.2, tumble: Vec3::ZERO };
            put(commands, &art.mote, Part::new(now, [0.05, 0.3, 0.7], Ink::PoisonLight, from + Vec3::Y * 0.2).size(0.04 + 0.03 * r(5)).motion(rise).flicker(0.4).shrink(0.3));
            // The tongue lies along +X: stood up, leaning a little.
            let lick = Quat::from_rotation_y(r(6) * std::f32::consts::TAU) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2 - 0.3 * r(7));
            let length = 0.3 + 0.25 * r(4);
            (art.tongue.clone(), Part::new(now, [0.05, 0.12, 0.4], Ink::PoisonLight, from).sized(Vec3::new(length, 1.0, length * 0.5)).turn(lick).grow(0.3).shrink(0.4).flicker(0.3))
        }
        Sparkle::Gold => {
            let from = at + round(1, 0.35) + Vec3::Y * (0.3 * r(2) - 0.3);
            (art.glint.clone(), Part::new(now, [0.08, 0.1, 0.35], Ink::Glint, from).size(0.25 + 0.15 * r(3)).facing(r(4) * std::f32::consts::TAU).grow(0.2).shrink(0.2).spin(3.0))
        }
    };
    put(commands, &part.0, part.1);
}

/// Seconds `ticks` server ticks are.
fn secs(ticks: f32) -> f32 {
    ticks / TICK_HZ as f32
}

/// The clock effects are timed on: the timeline others' fighters are drawn on.
pub(crate) fn fx_now(clock: &AttackClock) -> f32 {
    clock.now(false)
}
