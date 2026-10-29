//! `ARENA_PERF=file` (native only): measures what a frame costs, for finding what's slow.
//! Vsync goes off (so the frame rate is what the work allows, not the monitor's), every frame's
//! time is noted, and every second what's in the world is counted (entities, meshes drawn, the
//! effects' parts, lights, UI nodes), with how long the main world's schedules and the render
//! world's took a frame (`main`, `render`: they run side by side, so the longer one sets the frame
//! rate, with the extraction between them, `extract`, on top). At the end (`ARENA_PERF_SECS` in,
//! 60 by default) it writes them out, a line a second and a summary (frame times: average,
//! median, 99th percentile, worst), and quits.
//!
//! `ARENA_PERF_OFF` (a comma-separated list; `?off=` on the page in local dev) switches parts of
//! the scene off, to see what each costs: `lights` (the torches' point lights), `shadows` (the
//! sun's), `fog`, `ui` (all of it), `fx` (effects' parts), `blend` (everything see-through).

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::prelude::*;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::PresentMode;

use crate::fx::Parts as FxParts;
use crate::plain::PlainMaterial;

pub struct PerfPlugin {
    pub out: std::path::PathBuf,
}

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        let secs = std::env::var("ARENA_PERF_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60.0);
        app.insert_resource(Perf { out: self.out.clone(), secs, frames: Vec::new(), lines: String::new(), next: 1.0, second: Vec::new() });
        app.add_systems(Startup, no_vsync);
        app.add_systems(Last, (measure, census));
        let busy = Busy::default();
        app.insert_resource(busy.clone());
        app.add_systems(First, start_main);
        app.add_systems(Last, end_main.before(measure));
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            // The extraction between them, timed around Bevy's own: the main thread does it while
            // both worlds wait, so it's on every frame's length in full.
            if let Some(mut extract) = render_app.take_extract() {
                let times = busy.0.clone();
                render_app.set_extract(move |main_world, render_world| {
                    let started = Instant::now();
                    extract(main_world, render_world);
                    times.extract.fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
                    times.extracts.fetch_add(1, Ordering::Relaxed);
                });
            }
            render_app.insert_resource(busy);
            render_app.add_systems(Render, (start_render.in_set(RenderSystems::ExtractCommands), end_render.in_set(RenderSystems::PostCleanup)));
        }
    }
}

/// How long the main world's schedules (from `First` to `Last`), the render world's, and the
/// extraction took, added up over this second's frames, in microseconds, with how many there were;
/// shared by both worlds.
#[derive(Resource, Clone, Default)]
struct Busy(Arc<BusyTimes>);

#[derive(Default)]
struct BusyTimes {
    main_started: Mutex<Option<Instant>>,
    render_started: Mutex<Option<Instant>>,
    main: AtomicU64,
    mains: AtomicU64,
    render: AtomicU64,
    renders: AtomicU64,
    extract: AtomicU64,
    extracts: AtomicU64,
}

impl BusyTimes {
    fn start(at: &Mutex<Option<Instant>>) {
        *at.lock().unwrap() = Some(Instant::now());
    }

    fn end(at: &Mutex<Option<Instant>>, total: &AtomicU64, count: &AtomicU64) {
        if let Some(started) = at.lock().unwrap().take() {
            total.fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
            count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// The average of `total` over `count` (ms), both taken back to nothing.
    fn take(total: &AtomicU64, count: &AtomicU64) -> f32 {
        let (total, count) = (total.swap(0, Ordering::Relaxed), count.swap(0, Ordering::Relaxed));
        total as f32 / 1000.0 / count.max(1) as f32
    }
}

fn start_main(busy: Res<Busy>) {
    BusyTimes::start(&busy.0.main_started);
}

fn end_main(busy: Res<Busy>) {
    BusyTimes::end(&busy.0.main_started, &busy.0.main, &busy.0.mains);
}

fn start_render(busy: Res<Busy>) {
    BusyTimes::start(&busy.0.render_started);
}

fn end_render(busy: Res<Busy>) {
    BusyTimes::end(&busy.0.render_started, &busy.0.render, &busy.0.renders);
}

/// Switches parts of the scene off (`ARENA_PERF_OFF`, `?off=`): `off` lists them.
pub struct OffPlugin {
    pub off: String,
}

impl Plugin for OffPlugin {
    fn build(&self, app: &mut App) {
        let off = &self.off;
        app.insert_resource(Off {
            lights: off.contains("lights"),
            shadows: off.contains("shadows"),
            fog: off.contains("fog"),
            ui: off.contains("ui"),
            fx: off.contains("fx"),
            blend: off.contains("blend"),
        });
        app.add_systems(PostUpdate, switch_off);
    }
}

#[derive(Resource)]
struct Perf {
    out: std::path::PathBuf,
    secs: f32,
    /// Every frame's time (ms) since the start.
    frames: Vec<f32>,
    lines: String,
    next: f32,
    /// This second's frame times.
    second: Vec<f32>,
}

/// What's switched off (`ARENA_PERF_OFF`).
#[derive(Resource)]
struct Off {
    lights: bool,
    shadows: bool,
    fog: bool,
    ui: bool,
    fx: bool,
    blend: bool,
}

#[allow(clippy::type_complexity)]
fn switch_off(
    mut commands: Commands,
    off: Res<Off>,
    lights: Query<Entity, With<PointLight>>,
    mut suns: Query<&mut DirectionalLight>,
    fog: Query<Entity, With<bevy::pbr::DistanceFog>>,
    mut roots: Query<&mut Node, Without<ChildOf>>,
    mut parts: ResMut<FxParts>,
    mut drawn: Query<(&MeshMaterial3d<PlainMaterial>, &mut Visibility)>,
    materials: Res<Assets<PlainMaterial>>,
) {
    if off.lights {
        for light in &lights {
            commands.entity(light).remove::<PointLight>();
        }
    }
    if off.shadows {
        for mut sun in &mut suns {
            sun.shadow_maps_enabled = false;
        }
    }
    if off.fog {
        for camera in &fog {
            commands.entity(camera).remove::<bevy::pbr::DistanceFog>();
        }
    }
    if off.ui {
        for mut node in &mut roots {
            node.display = Display::None;
        }
    }
    if off.fx {
        parts.0.clear();
    }
    if off.blend {
        for (material, mut visibility) in &mut drawn {
            if materials.get(&material.0).is_some_and(|m| matches!(m.alpha_mode, AlphaMode::Blend | AlphaMode::Add | AlphaMode::Premultiplied | AlphaMode::Multiply)) {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

/// Once, a while into the run: what's drawn, by what it belongs to (the class of the fighter whose
/// rig it's on, else its own or its nearest named owner's name, else the game's own components
/// its topmost owner has), how many of each are see-through, and how many distinct meshes and
/// materials each comes to, written next to the perf file.
#[allow(clippy::type_complexity)]
fn census(
    time: Res<Time<Real>>,
    perf: Res<Perf>,
    mut done: Local<bool>,
    meshes: Query<(Entity, &ViewVisibility, &Mesh3d, Option<&MeshMaterial3d<PlainMaterial>>, Has<bevy::light::NotShadowCaster>)>,
    parents: Query<&ChildOf>,
    classes: Query<&arena_shared::classes::ClassId>,
    names: Query<&Name>,
    materials: Res<Assets<PlainMaterial>>,
    entities: Query<EntityRef>,
    components: &bevy::ecs::component::Components,
    nodes: Query<Entity, With<Node>>,
) {
    if *done || time.elapsed_secs() < perf.secs - 2.0 {
        return;
    }
    *done = true;
    use std::collections::{HashMap, HashSet};
    #[derive(Default)]
    struct Kind {
        drawn: usize,
        blended: usize,
        casters: usize,
        pairs: HashSet<(AssetId<Mesh>, Option<AssetId<PlainMaterial>>)>,
        mats: HashSet<Option<AssetId<PlainMaterial>>>,
        meshes: HashSet<AssetId<Mesh>>,
    }
    let mut kinds: HashMap<String, Kind> = HashMap::new();
    let mut all_mats: HashMap<Option<AssetId<PlainMaterial>>, usize> = HashMap::new();
    let ours = |entity: Entity| -> String {
        let Ok(entity) = entities.get(entity) else { return String::new() };
        let mut ours: Vec<String> = entity
            .archetype()
            .components()
            .iter()
            .filter_map(|&id| components.get_info(id))
            .map(|info| info.name().to_string())
            .filter(|name| name.starts_with("arena_"))
            .map(|name| name.rsplit("::").next().unwrap_or_default().to_string())
            .collect();
        ours.sort();
        ours.join(",")
    };
    for (entity, visible, mesh, material, no_shadow) in &meshes {
        if !visible.get() {
            continue;
        }
        let mut top = entity;
        let mut kind = None;
        let mut named = None;
        loop {
            if named.is_none() {
                named = names.get(top).ok().map(|n| n.to_string());
            }
            if let Ok(class) = classes.get(top) {
                kind = Some(format!("class {}", class.def().id));
                break;
            }
            match parents.get(top) {
                Ok(parent) => top = parent.parent(),
                Err(_) => break,
            }
        }
        let custom = if material.is_none() { " (own material)" } else { "" };
        let kind = kind.unwrap_or_else(|| match named {
            Some(name) => format!("other {name}{custom}"),
            None => format!("other [{}]{custom}", ours(top)),
        });
        let k = kinds.entry(kind).or_default();
        let id = material.map(|m| m.id());
        let blended = id.and_then(|id| materials.get(id)).is_some_and(|m| !matches!(m.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)));
        k.drawn += 1;
        k.blended += blended as usize;
        k.casters += (!no_shadow && !blended) as usize;
        k.pairs.insert((mesh.id(), id));
        k.mats.insert(id);
        k.meshes.insert(mesh.id());
        *all_mats.entry(id).or_default() += 1;
    }
    let mut text = String::new();
    let mut rows: Vec<_> = kinds.into_iter().collect();
    rows.sort_by_key(|(_, k)| std::cmp::Reverse(k.drawn));
    for (name, k) in rows {
        let _ = writeln!(text, "{name:40} drawn={:5} blended={:5} casters={:5} pairs={:4} materials={:4} meshes={:4}", k.drawn, k.blended, k.casters, k.pairs.len(), k.mats.len(), k.meshes.len());
    }
    let _ = writeln!(text, "distinct materials drawn: {}", all_mats.len());
    // The UI's nodes, by the game's own components on the nearest of each and its owners that
    // has any.
    let mut uis: HashMap<String, usize> = HashMap::new();
    for node in &nodes {
        let mut at = node;
        let mut depth = 0;
        let label = loop {
            let found = ours(at);
            if !found.is_empty() {
                break format!("{}[{found}]", "^".repeat(depth));
            }
            match parents.get(at) {
                Ok(parent) => (at, depth) = (parent.parent(), depth + 1),
                Err(_) => break names.get(node).map_or("(none)".to_string(), |n| n.to_string()),
            }
        };
        *uis.entry(label).or_default() += 1;
    }
    let mut uis: Vec<_> = uis.into_iter().collect();
    uis.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let _ = writeln!(text, "ui nodes: {}", nodes.iter().count());
    for (label, n) in uis {
        let _ = writeln!(text, "  {n:5} {label}");
    }
    let _ = std::fs::write(perf.out.with_extension("census.txt"), text);
}

fn no_vsync(mut windows: Query<&mut Window>) {
    for mut window in &mut windows {
        window.present_mode = PresentMode::AutoNoVsync;
    }
}

#[allow(clippy::type_complexity)]
fn measure(
    time: Res<Time<Real>>,
    mut perf: ResMut<Perf>,
    all: Query<Entity>,
    meshes: Query<(&ViewVisibility, &Mesh3d, Option<&MeshMaterial3d<PlainMaterial>>)>,
    parts: (Res<FxParts>, Res<crate::fx::draw::Drawn>),
    lights: Query<(), Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>>,
    nodes: Query<(), With<Node>>,
    assets: (Res<Assets<Mesh>>, Res<Assets<PlainMaterial>>, Res<Assets<Image>>),
    busy: Res<Busy>,
    mut exit: MessageWriter<AppExit>,
) {
    let ms = time.delta_secs() * 1000.0;
    let now = time.elapsed_secs();
    if now < 0.5 {
        return;
    }
    perf.frames.push(ms);
    perf.second.push(ms);
    if now >= perf.next {
        perf.next = now + 1.0;
        let second = std::mem::take(&mut perf.second);
        let avg = second.iter().sum::<f32>() / second.len().max(1) as f32;
        let worst = second.iter().copied().fold(0.0, f32::max);
        let drawn = meshes.iter().filter(|(v, ..)| v.get()).count();
        // What's drawn by mesh and material: about what batching leaves as draws.
        let batches = meshes.iter().filter(|(v, ..)| v.get()).map(|(_, mesh, material)| (mesh.id(), material.map(|m| m.id()))).collect::<std::collections::HashSet<_>>().len();
        let (main, render) = (BusyTimes::take(&busy.0.main, &busy.0.mains), BusyTimes::take(&busy.0.render, &busy.0.renders));
        let extract = BusyTimes::take(&busy.0.extract, &busy.0.extracts);
        let line = format!(
            "t={now:5.1}s fps={:5.0} avg={avg:5.2}ms worst={worst:6.2}ms main={main:5.2}ms render={render:5.2}ms extract={extract:5.2}ms entities={} meshes={} drawn={drawn} batches={batches} fx_parts={} fx_corners={} lights={} ui_nodes={} mesh_assets={} materials={} images={}",
            second.len(),
            all.iter().count(),
            meshes.iter().count(),
            parts.0.0.len(),
            parts.1.corners(),
            lights.iter().count(),
            nodes.iter().count(),
            assets.0.len(),
            assets.1.len(),
            assets.2.len(),
        );
        let _ = writeln!(perf.lines, "{line}");
    }
    if now >= perf.secs {
        let mut sorted = perf.frames.clone();
        sorted.sort_by(f32::total_cmp);
        let at = |q: f32| sorted.get(((sorted.len() as f32 * q) as usize).min(sorted.len().saturating_sub(1))).copied().unwrap_or(0.0);
        let avg = sorted.iter().sum::<f32>() / sorted.len().max(1) as f32;
        let summary = format!(
            "frames={} avg={avg:.2}ms median={:.2}ms p99={:.2}ms p999={:.2}ms worst={:.2}ms\n",
            sorted.len(),
            at(0.5),
            at(0.99),
            at(0.999),
            sorted.last().copied().unwrap_or(0.0)
        );
        let text = format!("{summary}{}", perf.lines);
        let _ = std::fs::write(&perf.out, text);
        exit.write(AppExit::Success);
    }
}
