//! A strike tearing the ground open over its lane: the void and the wisp light welling out of
//! its cracks, a burst of mist, chunks of earth thrown up and tumbling back, the tear lingering,
//! then closing and fading.

use super::*;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshTag;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};

use crate::arena::to_world;

/// A strike's impact: how long the rift lasts and when it starts fading (seconds), how long the
/// dust takes to burst out, how far out (meters) past the blade, and how see-through the rift's
/// ink and rim are at most.
const RIFT_LIFE: (f32, f32) = (2.2, 3.4);
/// How far into a tear's life (0..1) it starts to close, when its class says how long it lasts
/// (`RiftDef`): it's drawn as long as its rift is open.
const RIFT_CLOSES: f32 = 0.65;
const DUST_LIFE: f32 = 0.45;
const DUST_OUT: f32 = 0.9;
/// How see-through the mist is at most.
const DUST_ALPHA: f32 = 0.15;
pub(crate) const RIFT_ALPHA: f32 = 0.9;
pub(crate) const RIM_ALPHA: f32 = 1.0;
/// The spectral light in the cracks: how thick its core is (a fraction of the cracks' ink), how
/// brightly it glows, how long it flares as the ground breaks (seconds), and how it breathes
/// after (how much of its brightness, how fast: radians per second). How thick the ink edging
/// the cracks is.
pub(crate) const CORE_THICKNESS: f32 = 0.6;
pub(crate) const INK_THICKNESS: f32 = 1.35;
pub(super) const CORE_GLOW: f32 = 3.5;
/// How bright (opaque) the light in the cracks is at most.
pub(super) const CORE_ALPHA: f32 = 0.85;
/// Through a tear's going (from when it starts closing to when it's gone, 0..1): when its cracks
/// have closed, and when the mark starts fading out. How thick the cracks are once closed (of how
/// thick they were): a hairline left till the tear's faded.
const CLOSED_BY: f32 = 0.8;
const CLOSED_THICKNESS: f32 = 0.125;
const FADES_FROM: f32 = 0.75;
const FLARE_SECONDS: f32 = 0.18;
const BREATH: (f32, f32) = (0.18, 5.5);
/// The chunks of earth thrown up: how many per meter of blade, how big (meters, smallest and
/// largest), how hard they're thrown (m/s up, and out), and how long they last (seconds).
const CHUNKS_PER_METER: f32 = 5.0;
const CHUNK_SIZE: (f32, f32) = (0.035, 0.09);
const CHUNK_THROW: (f32, f32) = (3.2, 1.6);
const CHUNK_LIFE: f32 = 0.9;
pub(super) const GRAVITY: f32 = 14.0;

/// An impact on the ground, from `started` (seconds): the strike's break over its lane, its
/// hitbox (`length` along it, `width` across), lying on the surfaces under it in layers (`lying`), each
/// drawn by an entity of its own from the lane's frame (`frame`): the void under the cracks, the
/// ink bleeding out round it, the cracks' ink and the light in them (each closing onto its
/// spine), and the mist bursting out.
#[derive(Component)]
pub(super) struct Impact {
    pub(super) started: f32,
    /// When (seconds after it opens) it starts closing, and when it's gone.
    pub(super) life: (f32, f32),
    frame: Transform,
    length: f32,
    width: f32,
    /// Which way it breaks (`BREAKS`), and whether it's drawn mirrored across its lane
    /// (`lying_tag`).
    breaks: u32,
    mirrored: bool,
    rim: Entity,
    bleed: Entity,
    ink: Entity,
    core: Entity,
    dust: Entity,
}

impl Impact {
    fn parts(&self) -> [Entity; 5] {
        [self.rim, self.bleed, self.ink, self.core, self.dust]
    }
}

/// How a tear's layers are stretched `open` (0 to 1) of the way split open (from the middle out),
/// over a lane `length` by `width`, and `across` as wide again.
fn opened(open: f32, across: f32, (length, width): (f32, f32)) -> Vec3 {
    Vec3::new((0.6 + 0.4 * open) * length, 1.0, open * across * width)
}

/// How much wider again (across the lane) the light in the cracks flares as the ground breaks,
/// and how far in from where it holds the ink bleeding round it starts seeping out.
pub(super) const FLARE_WIDE: f32 = 1.2;
const SEEP_FROM: f32 = 0.92;

/// How the mist is stretched `burst` (0 to 1) of the way burst out over a tear `length` by `width`.
fn dusted(length: f32, width: f32, burst: f32) -> Vec3 {
    Vec3::new(length + DUST_OUT, 1.0, width + DUST_OUT) * (0.6 + 0.6 * burst)
}

/// How high over what it lies on (meters) each layer of a tear lies, from the void up: the void,
/// the ink bleeding round it, the cracks' ink and the light in them, the mist; and how high the
/// mist rises off it as it bursts out.
const RIM_LIFT: f32 = 0.025;
const BLEED_LIFT: f32 = 0.024;
const INK_LIFT: f32 = 0.03;
const CORE_LIFT: f32 = 0.034;
const DUST_LIFT: f32 = 0.04;
const DUST_RISE: f32 = 0.25;
/// How many cells (along, across) the grids a tear's layers are drawn on have (`lying_grid`), to
/// follow what they lie on under the biggest lane: the cracks (flaring wide) and the broken
/// ground, and the soft mist.
pub(super) const TEAR_CELLS: UVec2 = UVec2::new(120, 90);
const DUST_CELLS: UVec2 = UVec2::new(48, 24);

/// How many ways a tear breaks (each drawn mirrored across its lane as well), each a tile of its
/// texture (`tear_look`): how many tiles there are (along it, down it), and what of its lane each
/// covers (from the lane's near end, along it and across it, in its length and width).
pub(super) const BREAKS: u32 = 8;
pub(super) const TEAR_TILES: UVec2 = UVec2::new(2, 4);
pub(super) const TEAR_BOX: (Vec2, Vec2) = (Vec2::new(-0.3, -0.95), Vec2::new(1.3, 0.95));

/// The texture each way a tear breaks is drawn from (`bake.rs`), a tile each: how opaque the
/// broken ground is (red), and the ink bleeding round it (green); how far from the nearest
/// crack's spine each point is, in that crack's widths there (blue, as far as `CLOSING_REACH`).
pub(super) fn tear_look() -> Image {
    let png = include_bytes!("tears.png");
    Image::from_buffer(png, ImageType::Extension("png"), CompressedImageFormats::NONE, false, ImageSampler::linear(), RenderAssetUsages::RENDER_WORLD).expect("tears.png is a png")
}

/// A chunk of earth thrown up by an impact: flying (m/s), tumbling (rad/s), from `started`, till
/// it comes down on whatever's under it.
#[derive(Component)]
pub(super) struct Chunk {
    pub(super) started: f32,
    pub(super) velocity: Vec3,
    pub(super) spin: Vec3,
    pub(super) size: f32,
}

/// A strike going off breaks the ground all over its hitbox (its lane, `AttackDef::lane`: longer
/// and wider empowered), from where the striker stands toward the blow, the moment its hit is
/// dealt: the hit area, shown, lying on whatever the blade comes down on all over it (`Underfoot`:
/// up a slope, on a step or a crate, only as far down one or over a drop as it reaches), in
/// pieces where it runs over steps and edges, and none of it out in the air.
pub(super) fn strike_ground(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut fighters: Query<(Entity, &mut Blade, &LastSwing, &Pos, &ClassId, (&PlayerId, Option<&Team>), Option<&mut TornRifts>)>,
) {
    let now = time.elapsed_secs();
    for (entity, mut blade, swing, pos, class, (id, team), torn) in &mut fighters {
        if !std::mem::take(&mut blade.striking) {
            continue;
        }
        // Only a blade that tears the ground open (the revenant's) breaks it.
        if class.def().attack.rift.is_none() {
            continue;
        }
        if let Some(feet) = blade.struck.as_ref().map(Underfoot::feet)
            && let Some(rift) = arena_shared::sim::Rift::torn(id.0, *class, pos.0, feet, swing)
        {
            // Where it burns its striker's foes (`burn_in_rifts`), while it's open.
            if rift.burn > 0 {
                let open = (rift.until - swing.tick) as f32 / arena_shared::config::TICK_HZ as f32;
                commands.spawn(RiftZone { rift, tick: swing.tick, team: team.copied(), until: now + open });
            }
            // And where it hastes its striker, kept from where they stood as they struck: the
            // live move speed (`stat_frame`) reads it, as the server grants it.
            match torn {
                Some(mut torn) => {
                    torn.0.retain(|r| r.until > swing.tick);
                    torn.0.push(rift);
                }
                None => _ = commands.entity(entity).insert(TornRifts(vec![rift])),
            }
        }
        // The plain strike's break, and an empowered one's: the same, grown to its lane.
        let (Some((from, range, _)), Some((near, far, wide))) = (class.def().attack.lane(false), class.def().attack.lane_grown(swing.empowered, swing.grown)) else { continue };
        let Some(mut under) = blade.struck.take() else { continue };
        let dir = swing.dir.normalize_or(Vec2::X);
        let big = (far - near) / (range - from);
        // Drawn as long as its rift is open, if it tears one.
        let life = class.def().attack.rift.map_or(RIFT_LIFE, |rift| {
            let open = rift.ticks as f32 / arena_shared::config::TICK_HZ as f32;
            (open * RIFT_CLOSES, open)
        });
        let seed = swing.tick;
        let frame = under.frame();
        let looks = &assets.tear;
        let mut part = |mesh: &Handle<Mesh>, material: &Handle<LyingMaterial>| {
            // Not shown till it's laid (`lay_impacts`) and driven (`drive_impacts`).
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), frame, MeshTag(0), Visibility::Hidden, NoFrustumCulling, NotShadowCaster, NotShadowReceiver)).id()
        };
        // Each drawn over those below it in the same place: the void and the ink bleeding round
        // it, the cracks' ink, their light, the mist.
        let (rim, bleed, ink, core) = (part(&looks.grid, &looks.rim), part(&looks.grid, &looks.bleed), part(&looks.grid, &looks.ink), part(&looks.grid, &looks.core));
        let dust = part(&looks.mist, &looks.dust);
        // Earth thrown up all over it, out to either side, off whatever it lies on: its
        // chunks as big as the whole lane's.
        let (along, side) = (to_world(dir, 0.0), to_world(Vec2::new(dir.y, -dir.x), 0.0));
        for i in 0..((range - from) * CHUNKS_PER_METER).round() as u32 {
            let r = |k: u32| noise(seed, 1000 + i * 8 + k);
            let across = (r(1) - 0.5) * wide;
            let local = Vec2::new(near + (far - near) * r(0), across);
            let ground = under.height(local);
            let out = side * across.signum() * (0.4 + r(1)) + along * (r(2) - 0.3) * 0.6;
            let velocity = (Vec3::Y * CHUNK_THROW.0 * (0.5 + r(3)) + out * CHUNK_THROW.1) * big.sqrt();
            let size = (CHUNK_SIZE.0 + (CHUNK_SIZE.1 - CHUNK_SIZE.0) * r(4) * r(4)) * big;
            let spin = Vec3::new(r(5) - 0.5, r(6) - 0.5, r(7) - 0.5) * 30.0;
            let transform = Transform::from_translation(to_world(under.at(local), ground + size / 2.0)).with_scale(Vec3::splat(size));
            commands.spawn((
                Mesh3d(assets.chunk.clone()),
                MeshMaterial3d(assets.chunks[(i % 3 == 0) as usize].clone()),
                transform,
                GlobalTransform::from(transform),
                Chunk { started: now, velocity, spin, size },
                NotShadowReceiver,
            ));
        }
        let (lane, middle, reach) = ((far - near, wide), (near + far) / 2.0, tear_reach(near, far, wide));
        // One of the ways it breaks, mirrored every other round of them.
        let (breaks, mirrored) = (seed % BREAKS, seed / BREAKS % 2 == 1);
        let impact = Impact { started: now, life, frame, length: lane.0, width: lane.1, breaks, mirrored, rim, bleed, ink, core, dust };
        commands.spawn((impact, Unlaid { under, reach, near, middle }));
    }
}

/// All the ground a tear over the lane from `near` to `far`, `wide`, can come over (in the lane's
/// plane, a box): its layers split open, flaring and mirrored (its box is the same either side of
/// the lane), and the mist burst out.
pub(super) fn tear_reach(near: f32, far: f32, wide: f32) -> (Vec2, Vec2) {
    let (lane, middle) = ((far - near, wide), (near + far) / 2.0);
    let (start, most) = (Vec2::X * near, opened(1.0, 1.0 + FLARE_WIDE, lane).xz());
    let mist = dusted(lane.0, lane.1, 1.0).xz() / 2.0;
    ((start + TEAR_BOX.0 * most).min(Vec2::X * middle - mist), (start + TEAR_BOX.1 * most).max(Vec2::X * middle + mist))
}

/// A tear struck, yet to be laid on what's under its lane (`under`): all the ground it can come
/// over (`reach`), and where it's stretched from (along the lane: its near end, and its middle
/// for the mist).
#[derive(Component)]
pub(super) struct Unlaid {
    under: Underfoot,
    reach: (Vec2, Vec2),
    near: f32,
    middle: f32,
}

/// Lays each tear struck on what's under it the frame after (so the strike's frame isn't held
/// up by it), into a slot of the heights (`LyingHeights`), and shows it: none if there's no room.
pub(super) fn lay_impacts(mut commands: Commands, mut heights: ResMut<LyingHeights>, mut impacts: Query<(Entity, &Impact, &mut Unlaid)>) {
    for (entity, impact, mut unlaid) in &mut impacts {
        let (lo, hi) = unlaid.reach;
        let under = unlaid.under.heights(lo, hi);
        let Some(slot) = heights.keep(&under, unlaid.near, unlaid.middle) else {
            for part in impact.parts().into_iter().chain([entity]) {
                commands.entity(part).despawn();
            }
            continue;
        };
        commands.entity(entity).remove::<Unlaid>().insert(slot);
    }
}

/// How each of a tear's layers is drawn, whatever it lies on, each over those below it in the
/// same place (`LyingMaterial`): the void and the ink bleeding round it, the cracks' ink, the
/// light in them (all on one grid over its texture's box), the mist (on its own).
pub(super) struct TearLooks {
    grid: Handle<Mesh>,
    mist: Handle<Mesh>,
    rim: Handle<LyingMaterial>,
    bleed: Handle<LyingMaterial>,
    ink: Handle<LyingMaterial>,
    core: Handle<LyingMaterial>,
    dust: Handle<LyingMaterial>,
}

impl TearLooks {
    pub(super) fn new(meshes: &mut Assets<Mesh>, images: &mut Assets<Image>, materials: &mut Assets<LyingMaterial>, heights: &LyingHeights) -> Self {
        let tear = images.add(tear_look());
        let mut look = |color: Color, lift: f32, from_middle: bool, drawn: LyingShape, depth_bias: f32| {
            materials.add(LyingMaterial::new(color, lift, from_middle, heights, (&tear, TEAR_TILES, drawn), depth_bias))
        };
        TearLooks {
            grid: meshes.add(lying_grid(TEAR_BOX.0, TEAR_BOX.1, TEAR_CELLS, false)),
            mist: meshes.add(lying_grid(Vec2::splat(-0.5), Vec2::splat(0.5), DUST_CELLS, false)),
            rim: look(palette::TATTERS.with_alpha(RIM_ALPHA), RIM_LIFT, false, LyingShape::Channel(0), 0.0),
            bleed: look(palette::TATTERS.with_alpha(RIM_ALPHA), BLEED_LIFT, false, LyingShape::Channel(1), 0.0),
            ink: look(palette::INK.with_alpha(RIFT_ALPHA), INK_LIFT, false, LyingShape::Closing(2), 2.0),
            core: look(palette::WISP.with_alpha(CORE_ALPHA), CORE_LIFT, false, LyingShape::Closing(2), 3.0),
            dust: look(palette::WISP.with_alpha(DUST_ALPHA), DUST_LIFT, true, LyingShape::Ring, 4.0),
        }
    }
}

/// Shows `part` (`lying_at` its lane's `frame`) stretched by `stretch`, its cracks `thickness`
/// thick, `opaque` (0 to 1): not drawn at all while it's clear.
fn show_part(parts: &mut Query<(&mut Transform, &mut MeshTag, &mut Visibility)>, part: Entity, lies: (&HeightsSlot, u32, bool), frame: Transform, stretch: Vec3, thickness: f32, opaque: f32) {
    let Ok((mut transform, mut tag, mut visibility)) = parts.get_mut(part) else { return };
    visibility.set_if_neq(if opaque <= 0.0 { Visibility::Hidden } else { Visibility::Inherited });
    transform.set_if_neq(lying_at(frame, stretch));
    tag.set_if_neq(lying_tag(lies, thickness, opaque));
}

/// Tears split open at once, light flaring in their cracks, linger glowing and breathing, then
/// close and fade; their mist bursts out and is gone. Each layer lies on what's under it however
/// it's stretched (`lying.wgsl`): only its transform and its tag change.
pub(super) fn drive_impacts(mut commands: Commands, time: Res<Time>, impacts: Query<(Entity, &Impact, Option<&HeightsSlot>)>, mut parts: Query<(&mut Transform, &mut MeshTag, &mut Visibility)>) {
    let now = time.elapsed_secs();
    for (entity, impact, slot) in &impacts {
        let age = now - impact.started;
        if age >= impact.life.1 {
            for part in impact.parts().into_iter().chain([entity]) {
                commands.entity(part).despawn();
            }
            continue;
        }
        // Not laid yet (`lay_impacts`).
        let Some(slot) = slot else { continue };
        let lies = (slot, impact.breaks, impact.mirrored);
        let (frame, lane) = (impact.frame, (impact.length, impact.width));
        // Splitting open fast, from the middle out, then still.
        let open = crate::ease::ease_out(0.0, 0.07, age);
        // As it goes: the cracks closing in first, done a little before the end; the mark
        // fading out later, slow at first and quicker and quicker, gone at the end.
        let going = ((age - impact.life.0) / (impact.life.1 - impact.life.0)).max(0.0);
        let narrowed = 1.0 - (1.0 - CLOSED_THICKNESS) * (going / CLOSED_BY).min(1.0);
        let faded_out = ((going - FADES_FROM) / (1.0 - FADES_FROM)).clamp(0.0, 1.0).powi(2);
        // Closing in as it fades: each crack narrowing onto its own spine.
        show_part(&mut parts, impact.ink, lies, frame, opened(open, 1.0, lane), INK_THICKNESS * narrowed, 1.0 - faded_out);
        // The light in the cracks: flaring wide and full as the ground breaks, then glowing lower,
        // breathing, and closing with the tear as it fades.
        let flare = 1.0 - (age / FLARE_SECONDS).min(1.0);
        let breath = 1.0 - BREATH.0 * (0.5 + 0.5 * (age * BREATH.1).sin());
        let bright = (1.0 - faded_out) * (0.7 * breath + 0.3 * flare).max(flare);
        show_part(&mut parts, impact.core, lies, frame, opened(open, 1.0 + FLARE_WIDE * flare, lane), CORE_THICKNESS * narrowed, bright);
        // The ink seeps out a little after the ground breaks, then holds and fades with it.
        let seep = 1.0 - (1.0 - (age / 0.35).min(1.0)).powi(2);
        let seeped = SEEP_FROM + (1.0 - SEEP_FROM) * seep;
        show_part(&mut parts, impact.bleed, lies, frame, opened(open, 1.0, lane) * Vec3::new(seeped, 1.0, seeped), 1.0, (1.0 - faded_out).min(seep));
        show_part(&mut parts, impact.rim, lies, frame, opened(open, 1.0, lane), 1.0, 1.0 - faded_out);
        // The mist bursting out over it, rising off it, and gone.
        let burst = crate::ease::ease_out(0.0, DUST_LIFE, age);
        let risen = frame.with_translation(frame.translation + Vec3::Y * DUST_RISE * burst);
        show_part(&mut parts, impact.dust, lies, risen, dusted(impact.length, impact.width, burst), 1.0, 1.0 - (age / DUST_LIFE).min(1.0));
    }
}

/// The rifts a fighter's strikes tore that are still open (`RiftDef`), each from where they stood
/// as they struck: their own ground hastes them (the server's `move_players`).
#[derive(Component)]
pub(crate) struct TornRifts(pub(crate) Vec<arena_shared::sim::Rift>);

/// An open rift that burns its striker's foes standing in it (`RiftDef::burn`), as the server
/// has it: torn at `tick`, by one on `team`, gone at `until` (seconds).
#[derive(Component)]
pub(super) struct RiftZone {
    rift: arena_shared::sim::Rift,
    tick: u32,
    team: Option<Team>,
    until: f32,
}

/// How often (seconds) a foe burning in a rift gives off a green flame.
const BURN_EVERY: f32 = 0.12;

/// Foes standing in an open rift burn: green flames licking up off them now and then. Rifts
/// gone are let go.
#[allow(clippy::type_complexity)]
pub(super) fn burn_in_rifts(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::fx::FxArt>,
    map: Res<crate::rooms::CurrentMap>,
    zones: Query<(Entity, &RiftZone)>,
    fighters: Query<(&Pos, &Health, Option<&Team>, (Option<&Motion>, Option<&crate::render::PredictedWalk>, &ClassId, Has<Elite>)), (With<PlayerId>, Without<Shopkeeper>)>,
    mut next: Local<(f32, u32)>,
) {
    let now = time.elapsed_secs();
    for (entity, zone) in &zones {
        if now >= zone.until {
            commands.entity(entity).despawn();
        }
    }
    if now < next.0 {
        return;
    }
    *next = (now + BURN_EVERY, next.1.wrapping_add(1));
    let mut k = next.1.wrapping_mul(97);
    for (pos, health, team, (motion, predicted, class, elite)) in &fighters {
        let foe = |zone: &RiftZone| !zone.team.zip(team).is_some_and(|(theirs, mine)| theirs.allied(*mine));
        let feet = crate::render::shown_height(map.map(), motion, predicted, pos.0);
        if health.alive() && zones.iter().any(|(_, zone)| now < zone.until && foe(zone) && zone.rift.holds(map.map(), (pos.0, feet, class.def().hit_size(elite)), zone.tick)) {
            k = k.wrapping_add(1);
            crate::fx::sparkle(&mut commands, &art, now, to_world(pos.0, 0.0), crate::fx::Sparkle::Burn, k);
        }
    }
}

/// Chunks of earth fly, tumble and fall, come to rest on whatever they come down on, and shrink
/// away.
pub(super) fn throw_chunks(mut commands: Commands, time: Res<Time>, map: Res<crate::rooms::CurrentMap>, mut chunks: Query<(Entity, &mut Chunk, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs().min(0.05));
    for (entity, mut chunk, mut transform) in &mut chunks {
        let age = now - chunk.started;
        if age >= CHUNK_LIFE {
            commands.entity(entity).despawn();
            continue;
        }
        // What's under it, up to where its bottom is.
        let at = transform.translation;
        let rest = map.map().surface_under(arena::to_gameplay(at), at.y - chunk.size / 2.0 + 1e-3) + chunk.size / 2.0;
        if at.y <= rest + 1e-3 && chunk.velocity.y <= 0.0 {
            chunk.velocity = Vec3::ZERO;
            chunk.spin = Vec3::ZERO;
            transform.translation.y = rest;
        } else {
            chunk.velocity.y -= GRAVITY * dt;
            transform.translation += chunk.velocity * dt;
            transform.translation.y = transform.translation.y.max(rest);
            transform.rotate(Quat::from_scaled_axis(chunk.spin * dt));
        }
        let shrink = 1.0 - ((age - CHUNK_LIFE * 0.7) / (CHUNK_LIFE * 0.3)).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(chunk.size * shrink);
    }
}
