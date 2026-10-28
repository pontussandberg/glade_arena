//! `ARENA_PERF=file` (native only): measures what a frame costs, for finding what's slow.
//! Vsync goes off (so the frame rate is what the work allows, not the monitor's), every frame's
//! time is noted, and every second what's in the world is counted (entities, meshes drawn, the
//! effects' parts, lights, UI nodes). At the end (`ARENA_PERF_SECS` in, 60 by default) it
//! writes them out, a line a second and a summary (frame times: average, median, 99th
//! percentile, worst), and quits.
//!
//! `ARENA_PERF_OFF` (a comma-separated list) switches parts of the scene off, to see what each
//! costs: `lights` (the torches' point lights), `shadows` (the sun's), `fog`, `ui` (all of it),
//! `fx` (effects' parts).

use std::fmt::Write as _;

use bevy::prelude::*;
use bevy::window::PresentMode;

use crate::fx::Part as FxPart;

pub struct PerfPlugin {
    pub out: std::path::PathBuf,
}

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        let secs = std::env::var("ARENA_PERF_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60.0);
        app.insert_resource(Perf { out: self.out.clone(), secs, frames: Vec::new(), lines: String::new(), next: 1.0, second: Vec::new() });
        app.add_systems(Startup, no_vsync);
        app.add_systems(Last, measure);
        let off = std::env::var("ARENA_PERF_OFF").unwrap_or_default();
        app.insert_resource(Off { lights: off.contains("lights"), shadows: off.contains("shadows"), fog: off.contains("fog"), ui: off.contains("ui"), fx: off.contains("fx") });
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
}

#[allow(clippy::type_complexity)]
fn switch_off(
    mut commands: Commands,
    off: Res<Off>,
    lights: Query<Entity, With<PointLight>>,
    mut suns: Query<&mut DirectionalLight>,
    fog: Query<Entity, With<bevy::pbr::DistanceFog>>,
    mut roots: Query<&mut Node, Without<ChildOf>>,
    parts: Query<Entity, With<FxPart>>,
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
        for part in &parts {
            commands.entity(part).despawn();
        }
    }
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
    meshes: Query<(&ViewVisibility, &Mesh3d, Option<&MeshMaterial3d<StandardMaterial>>)>,
    parts: Query<(), With<FxPart>>,
    lights: Query<(), Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>>,
    nodes: Query<(), With<Node>>,
    assets: (Res<Assets<Mesh>>, Res<Assets<StandardMaterial>>, Res<Assets<Image>>),
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
        let line = format!(
            "t={now:5.1}s fps={:5.0} avg={avg:5.2}ms worst={worst:6.2}ms entities={} meshes={} drawn={drawn} batches={batches} fx_parts={} lights={} ui_nodes={} mesh_assets={} materials={} images={}",
            second.len(),
            all.iter().count(),
            meshes.iter().count(),
            parts.iter().count(),
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
