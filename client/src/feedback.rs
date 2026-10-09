//! Combat feedback: a health bar over every fighter with a cast bar under it while it winds up
//! an attack, a white flash when one takes damage, and dead fighters disappearing until they
//! respawn.

use arena_shared::protocol::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use lightyear::core::time::TickInstant;
use lightyear::interpolation::timeline::InterpolationTimeline;
use lightyear::prelude::*;

use crate::camera::CameraPlaced;
use crate::glade::{palette, to_world};
use crate::render::{player_color, shown};

pub struct FeedbackPlugin;

impl Plugin for FeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_bars, place_bars.in_set(CameraPlaced), flash_on_hit, end_flashes, hide_the_dead),
        );
    }
}

/// Bar size in pixels, and how high above a fighter's feet it floats.
const BAR_SIZE: Vec2 = Vec2::new(56.0, 7.0);
const BAR_HEIGHT: f32 = 2.3;
/// The cast bar hangs right under the health bar, as wide and a bit thinner.
const CAST_BAR_HEIGHT: f32 = 5.0;
const FLASH_SECONDS: f32 = 0.12;
/// How bright the flash is (emissive white on the fighter's own material).
const FLASH_GLOW: f32 = 3.0;

/// A fighter's bars: the health bar (this entity) and its parts.
#[derive(Component)]
struct Bars {
    player: Entity,
    health_fill: Entity,
    cast: Entity,
    cast_fill: Entity,
}

/// The last health we showed for this fighter, to tell a hit from a heal or a respawn.
#[derive(Component)]
struct ShownHealth(i32);

/// A fighter flashing white after a hit, until `until`.
#[derive(Component)]
struct HitFlash {
    until: f32,
}

/// Each fighter gets its bars once it has a body.
fn spawn_bars(mut commands: Commands, new: Query<(Entity, &PlayerId, Has<Predicted>), Added<Mesh3d>>) {
    let fill = |color: Color| (Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(color));
    let frame = |node: Node| {
        (
            Node { border: UiRect::all(px(1.0)), position_type: PositionType::Absolute, ..node },
            BackgroundColor(palette::INK.with_alpha(0.8)),
            BorderColor::all(palette::INK),
        )
    };
    for (player, id, is_me) in &new {
        let health_fill = commands.spawn(fill(player_color(id.0, is_me))).id();
        // Pale, so it never reads as a (colored) health bar.
        let cast_fill = commands.spawn(fill(palette::SUN)).id();
        // Absolute inside the health bar's border, so shift left by it to line the two up.
        let cast = commands
            .spawn((
                frame(Node { left: px(-1.0), top: px(BAR_SIZE.y - 1.0), width: px(BAR_SIZE.x), height: px(CAST_BAR_HEIGHT), ..default() }),
                Visibility::Hidden,
            ))
            .add_child(cast_fill)
            .id();
        commands
            .spawn((
                Bars { player, health_fill, cast, cast_fill },
                frame(Node { width: px(BAR_SIZE.x), height: px(BAR_SIZE.y), ..default() }),
                Visibility::Hidden,
            ))
            .add_children(&[health_fill, cast]);
    }
}

/// "Now" in fractional ticks, on the timeline a fighter is shown on, to compare with its
/// `AttackState`: ours is predicted (our own windup starts at the click), others' are
/// interpolated (their windup plays out in step with their delayed position).
#[derive(SystemParam)]
pub(crate) struct AttackClock<'w, 's> {
    local: SyncedLocalTimeline<'w, 's>,
    fixed: Res<'w, Time<Fixed>>,
    interpolation: Option<Res<'w, InterpolationTimeline>>,
}

impl AttackClock<'_, '_> {
    pub(crate) fn now(&self, predicted: bool) -> f32 {
        let ticks = |t: TickInstant| t.tick().0 as f32 + t.overstep().to_f32();
        match &self.interpolation {
            Some(timeline) if !predicted => ticks(timeline.now),
            _ => ticks(self.local.instant(&self.fixed)),
        }
    }
}

/// Keep each fighter's bars over it and filled (the cast bar by `AttackClock`). Only writes what
/// changed, so a still scene doesn't make Bevy lay out the UI again every frame.
fn place_bars(
    mut commands: Commands,
    clock: AttackClock,
    camera: Single<(&Camera, &GlobalTransform)>,
    players: Query<(&Pos, &ClassId, Option<&Health>, &AttackState, Has<Predicted>)>,
    mut bars: Query<(Entity, &Bars, &mut Node, &mut Visibility)>,
    mut parts: Query<(&mut Node, &mut Visibility), Without<Bars>>,
) {
    let (camera, camera_transform) = *camera;
    for (bar, ids, mut node, mut visibility) in &mut bars {
        let Ok((pos, class, health, attack, is_me)) = players.get(ids.player) else {
            commands.entity(bar).despawn();
            continue;
        };
        let alive = health.is_none_or(Health::alive);
        let screen = camera.world_to_viewport(camera_transform, to_world(pos.0, BAR_HEIGHT)).ok();
        let Some(screen) = screen.filter(|_| alive) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let (left, top) = (px(screen.x - BAR_SIZE.x / 2.0), px(screen.y - BAR_SIZE.y / 2.0));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }

        let mut fill_to = |part: Entity, fraction: f32| {
            if let Ok((mut fill, _)) = parts.get_mut(part) {
                let width = percent(fraction.clamp(0.0, 1.0) * 100.0);
                if fill.width != width {
                    fill.width = width;
                }
            }
        };
        fill_to(ids.health_fill, health.map_or(1.0, |h| h.0 as f32 / class.def().max_hp as f32));
        if let Some(windup) = attack.windup {
            fill_to(ids.cast_fill, windup.progress(clock.now(is_me), *class));
        }
        if let Ok((_, mut cast_visibility)) = parts.get_mut(ids.cast) {
            cast_visibility.set_if_neq(shown(attack.windup.is_some()));
        }
    }
}

/// Flash white when health drops. `Health` is server-authoritative, so this shows confirmed
/// hits only, about one round trip after the swing or shot. The fighter's material is its own
/// (one per player), so the flash brightens it in place.
fn flash_on_hit(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut players: Query<
        (Entity, &Health, &MeshMaterial3d<StandardMaterial>, Option<&mut ShownHealth>, Option<&mut HitFlash>),
        Changed<Health>,
    >,
) {
    let until = time.elapsed_secs() + FLASH_SECONDS;
    for (player, health, material, shown, flashing) in &mut players {
        let Some(mut shown) = shown else {
            commands.entity(player).insert(ShownHealth(health.0));
            continue;
        };
        let was_hit = health.0 < shown.0;
        shown.0 = health.0;
        if !was_hit {
            continue;
        }
        match flashing {
            Some(mut flash) => flash.until = until,
            None => {
                if let Some(mut m) = materials.get_mut(&material.0) {
                    m.emissive = LinearRgba::WHITE * FLASH_GLOW;
                }
                commands.entity(player).insert(HitFlash { until });
            }
        }
    }
}

fn end_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    flashing: Query<(Entity, &HitFlash, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (player, flash, material) in &flashing {
        if time.elapsed_secs() >= flash.until {
            if let Some(mut m) = materials.get_mut(&material.0) {
                m.emissive = LinearRgba::BLACK;
            }
            commands.entity(player).remove::<HitFlash>();
        }
    }
}

/// Dead fighters (waiting to respawn) aren't drawn. Also checked when a fighter first gets its
/// body, in case we first see someone while they're dead.
fn hide_the_dead(
    mut players: Query<(&Health, &mut Visibility), (With<PlayerId>, Or<(Changed<Health>, Added<Mesh3d>)>)>,
) {
    for (health, mut visibility) in &mut players {
        visibility.set_if_neq(shown(health.alive()));
    }
}
