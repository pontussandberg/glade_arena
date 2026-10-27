//! The action camera's aim. The crosshair is in the middle of the view, but the camera is off
//! your shoulder, so a shot sent the way the camera looks passes beside what the crosshair is
//! on. Instead the shot is sent through the first thing the crosshair's line meets
//! (`aim_through`): a wall, a fighter, or the ground. Then a fighter under the crosshair is hit,
//! a wall under it is hit where it is, and looking down sends the shot over the ground the
//! crosshair is on. The path itself is shown (`show_aim_line`): a faint line from where the shot
//! leaves you to where it ends (out of range, or at a wall), with a mark there.

use super::*;

use arena_shared::classes::AttackKind;
use arena_shared::map::Map;

use crate::Me;
use crate::camera::CameraMode;
use crate::feedback::AttackClock;
use crate::arena::to_gameplay;

/// How far out along the crosshair's line to look for something it meets.
const AIM_REACH: f32 = 60.0;
/// How finely (meters) the line is walked for a wall, rock or tree.
const AIM_STEP: f32 = 0.2;
/// Below this height, a tile that blocks shots blocks the crosshair's line too: the ruins' walls
/// are head height and more, the trees far taller.
const BLOCK_HEIGHT: f32 = 3.0;
/// How tall a fighter's body is to the crosshair's line.
const BODY_HEIGHT: f32 = 2.0;
/// The line meeting something nearer than this ahead of you (looking down at your own feet, or
/// with your face to a wall) can't aim a shot: the aim stays straight ahead.
const MIN_AHEAD: f32 = 0.5;
/// How wide (px) the mark at the end of the shot's line is.
pub(super) const AIM_END_SIZE: f32 = 10.0;

/// The mark at the end of the shot's line.
#[derive(Component)]
pub(super) struct AimEnd;

/// How high the auto-attack of `class` flies (a blow is taken to land at chest height).
pub(super) fn attack_height(class: ClassId) -> f32 {
    let def = class.def();
    let thrown = def.attack.kind.shot().is_some_and(|shot| arena::shot_look(def, shot, false).thrown);
    shot_height(thrown)
}

/// Where a fighter at `me` aims to send its shot through the point the crosshair (the middle of
/// the view) is on: the first wall, rock or tree the crosshair's line meets, or the first of the
/// other fighters' `bodies`, or else the ground. `None` with nothing ahead to aim at.
pub(super) fn aim_through(camera: &Camera, transform: &GlobalTransform, map: &Map, me: Vec2, forward: Vec2, bodies: &[Vec2]) -> Option<Vec2> {
    let center = camera.logical_viewport_size()? / 2.0;
    let ray = camera.viewport_to_world(transform, center).ok()?;
    let far = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)).map_or(AIM_REACH, |d| d.min(AIM_REACH));
    let mut hit = ray.get_point(far);
    let mut along = AIM_STEP;
    while along < far {
        let at = ray.get_point(along);
        let on = to_gameplay(at);
        let wall = at.y < BLOCK_HEIGHT && map.get(Map::tile_of(on)).blocks_shots();
        let body = at.y < BODY_HEIGHT && bodies.iter().any(|body| body.distance(on) <= PLAYER_RADIUS);
        if wall || body {
            hit = at;
            break;
        }
        along += AIM_STEP;
    }
    let aim = to_gameplay(hit) - me;
    (aim.dot(forward) >= MIN_AHEAD).then_some(aim)
}

/// Where the auto-attack of `class`, sent from `me` toward `dir`, starts and ends: a shot from
/// the edge of you out to its range, a blow over its lane (`empowered` or not); either stopped
/// by the first wall, rock or tree.
pub(super) fn attack_line(map: &Map, class: ClassId, me: Vec2, dir: Vec2, empowered: bool) -> Option<(Vec2, Vec2)> {
    let dir = dir.try_normalize()?;
    let attack = &class.def().attack;
    let (from, to) = match attack.kind {
        AttackKind::Projectile { radius, range, .. } => (PLAYER_RADIUS + radius, PLAYER_RADIUS + radius + range),
        AttackKind::Melee { .. } => {
            let (from, range, _) = attack.lane(empowered)?;
            (from, range)
        }
    };
    let mut end = to;
    let mut along = from;
    while along < to {
        if map.get(Map::tile_of(me + dir * along)).blocks_shots() {
            end = along;
            break;
        }
        along += 0.25;
    }
    Some((me + dir * from, me + dir * end))
}

/// The shot's line and the mark at its end, while the action camera has the cursor.
pub(super) fn show_aim_line(
    mode: Res<CameraMode>,
    desired: Res<crate::DesiredInput>,
    clock: AttackClock,
    map: Res<crate::rooms::CurrentMap>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::camera::CameraRig>>,
    me: Query<(&Pos, &ClassId, &AttackState), With<Me>>,
    mut gizmos: Gizmos,
    end: Single<(&mut UiTransform, &mut Visibility), With<AimEnd>>,
) {
    let (camera, camera_transform) = *camera;
    let (mut place, mut visibility) = end.into_inner();
    let line = me.single().ok().filter(|_| mode.free && mode.locked).and_then(|(pos, class, attack)| {
        let line = attack_line(map.map(), *class, pos.0, desired.0.aim, attack.empowered_at(clock.now(true)))?;
        Some((line, attack_height(*class)))
    });
    let Some(((from, to), height)) = line else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    gizmos.line(to_world(from, height), to_world(to, height), palette::ui::TEXT.with_alpha(0.35));
    match camera.world_to_viewport(camera_transform, to_world(to, height)) {
        Ok(at) => {
            place.set_if_neq(UiTransform { translation: Val2::px(at.x, at.y), ..default() });
            visibility.set_if_neq(Visibility::Inherited);
        }
        Err(_) => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}
