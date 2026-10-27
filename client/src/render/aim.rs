//! Action mode's aim: toward the cursor, from your fighter. The shot's path is drawn
//! (`show_aim_line`): a faint line from where the shot leaves you out to its range, with a mark
//! at the end. It is the aim itself, so it runs through anything in the way, as the aim never
//! bends for a wall; the shot stops where the sim stops it. While a normal cast is being aimed
//! the ability's own indicator takes its place (`casting::show_indicator`).

use super::*;

use arena_shared::classes::AttackKind;

use crate::Me;
use crate::camera::CameraMode;
use crate::feedback::AttackClock;

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

/// Where the auto-attack of `class`, sent from `me` toward `dir`, starts and ends: a shot from
/// the edge of you out to its range, a blow over its lane (`empowered` or not).
pub(super) fn attack_line(class: ClassId, me: Vec2, dir: Vec2, empowered: bool) -> Option<(Vec2, Vec2)> {
    let dir = dir.try_normalize()?;
    let attack = &class.def().attack;
    let (from, to) = match attack.kind {
        AttackKind::Projectile { radius, range, .. } => (PLAYER_RADIUS + radius, PLAYER_RADIUS + radius + range),
        AttackKind::Melee { .. } => {
            let (from, range, _) = attack.lane(empowered)?;
            (from, range)
        }
    };
    Some((me + dir * from, me + dir * to))
}

/// The shot's line and the mark at its end, in action mode, while no ability is being aimed.
pub(super) fn show_aim_line(
    mode: Res<CameraMode>,
    aiming: Res<crate::casting::Aiming>,
    desired: Res<crate::DesiredInput>,
    clock: AttackClock,
    camera: Single<(&Camera, &GlobalTransform), With<crate::camera::CameraRig>>,
    me: Query<(&Pos, &ClassId, &AttackState), With<Me>>,
    mut gizmos: Gizmos,
    end: Single<(&mut UiTransform, &mut Visibility), With<AimEnd>>,
) {
    let (camera, camera_transform) = *camera;
    let (mut place, mut visibility) = end.into_inner();
    let line = me.single().ok().filter(|_| mode.free && aiming.0.is_none()).and_then(|(pos, class, attack)| {
        let line = attack_line(*class, pos.0, desired.0.aim, attack.empowered_at(clock.now(true)))?;
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
