//! First person (`CameraMode::through_my_eyes`): your own fighter isn't drawn (`feedback`), only
//! its weapon arm in front of the camera and what it holds, posed from the same attack as its
//! body (`Rig::phase`): the spear is drawn back and let go just when it would be over the
//! shoulder, the flying one appearing on the crosshair's line (`render::fly_shots`).

use super::*;

use bevy::light::NotShadowCaster;

use crate::Me;
use crate::camera::{CameraMode, CameraRig};
use crate::ease::ease;

/// The weapon arm in view, in the camera's space (right +X, up +Y, ahead -Z; meters): where its
/// shoulder is, and in each of its poses (carrying, drawn back, let go: `Phase::pose`'s first
/// three) where its hand is and which way what it holds points. The elbow bends out toward
/// `pole`.
struct ViewPoses {
    shoulder: Vec3,
    hand: [Vec3; 3],
    points: [Vec3; 3],
    pole: Vec3,
}

/// The javelinist's: carried low on the right, its point just under the crosshair; drawn back
/// high beside the head, the shaft coming in from the screen's edge at eye level; driven out
/// ahead to the middle as it's let go.
const JAVELIN_VIEW: ViewPoses = ViewPoses {
    shoulder: Vec3::new(0.2, -0.32, 0.05),
    hand: [Vec3::new(0.3, -0.22, -0.45), Vec3::new(0.3, 0.02, -0.25), Vec3::new(0.12, -0.12, -0.62)],
    points: [Vec3::new(-0.08, 0.06, -1.0), Vec3::new(-0.12, 0.02, -1.0), Vec3::new(0.0, -0.05, -1.0)],
    pole: Vec3::new(1.0, -0.6, 0.3),
};

/// The arm in view for a figure, if it has one: only a thrower's yet.
fn view_poses(figure: &str) -> Option<&'static ViewPoses> {
    match figure {
        "javelinist" => Some(&JAVELIN_VIEW),
        _ => None,
    }
}

/// The empty hand drops out of view a moment after a throw (seconds), and comes back up with the
/// next weapon once it has it: how long either takes (seconds), and how far below it's out of
/// sight (meters).
const LOWER_AFTER: f32 = 0.15;
const ARM_SECONDS: f32 = 0.25;
const ARM_FROM: f32 = 0.5;
/// How far the arm in view bobs as you walk (meters, across and up).
const VIEW_BOB: Vec2 = Vec2::new(0.012, 0.016);

/// The weapon arm in first person, a child of the camera, for a fighter of `class`: its upper
/// arm, elbow and what its hand holds, and whether the hand holds it, since when (seconds).
#[derive(Component)]
pub(super) struct View {
    class: ClassId,
    arm: Entity,
    elbow: Entity,
    held: Entity,
    armed: Option<(bool, f32)>,
}

/// Puts the weapon arm in view while you see through your fighter's eyes (and takes it away
/// after), posed by its attack this frame: the hand reaching for where its poses put it, the
/// arm bending to it, the weapon pointing as they do and in sight when the body's would be.
#[allow(clippy::too_many_arguments)]
pub(super) fn hold_in_view(
    mut commands: Commands,
    time: Res<Time>,
    mode: Res<CameraMode>,
    assets: Res<RigAssets>,
    me: Query<(&Rig, &ClassId, &MeshMaterial3d<PlainMaterial>), With<Me>>,
    camera: Single<Entity, With<CameraRig>>,
    mut views: Query<(Entity, &mut View)>,
    mut parts: Query<(&mut Transform, &mut Visibility)>,
) {
    let mine = me.single().ok().filter(|_| mode.through_my_eyes()).and_then(|(rig, class, body)| Some((rig, *class, body, view_poses(class.def().figure())?)));
    let mut have = false;
    for (entity, view) in &views {
        if mine.is_some_and(|(_, class, ..)| class == view.class) {
            have = true;
        } else {
            commands.entity(entity).despawn();
        }
    }
    let Some((rig, class, body, poses)) = mine else { return };
    let handles = &assets.meshes[&class];
    if !have {
        let part = |commands: &mut Commands, mesh: &Handle<Mesh>, at: Vec3| commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(body.0.clone()), Transform::from_translation(at), NotShadowCaster)).id();
        let arm = part(&mut commands, &handles.arm, poses.shoulder);
        let elbow = part(&mut commands, &handles.forearm, rig.moves.elbow_at);
        let held = part(&mut commands, &handles.held, rig.moves.hand_at - rig.moves.elbow_at);
        commands.entity(arm).add_child(elbow);
        commands.entity(elbow).add_child(held);
        let view = commands.spawn((View { class, arm, elbow, held, armed: None }, Transform::default(), Visibility::default())).add_child(arm).id();
        commands.entity(*camera).add_child(view);
        return;
    }
    let Some((_, mut view)) = views.iter_mut().next() else { return };
    // In hand when the body's weapon would be (`empty_hands`): the empty hand dropping out of
    // view after the throw, coming back up with the next.
    let armed = parts.get(rig.held).is_ok_and(|(_, visibility)| *visibility != Visibility::Hidden);
    let now = time.elapsed_secs();
    if view.armed.is_none_or(|(was, _)| was != armed) {
        view.armed = Some((armed, if view.armed.is_some() { now } else { f32::NEG_INFINITY }));
    }
    let since = view.armed.map_or(f32::INFINITY, |(_, at)| now - at);
    let lowered = if armed { 1.0 - ease(0.0, ARM_SECONDS, since) } else { ease(LOWER_AFTER, LOWER_AFTER + ARM_SECONDS, since) };

    let phase = rig.phase;
    let posed = |keys: [Vec3; 3]| keys[0].lerp(keys[1], phase.draw).lerp(keys[2], phase.throw);
    let stride = rig.stride.rem_euclid(1.0) * TAU;
    let bob = Vec3::new(stride.sin() * VIEW_BOB.x, -stride.sin().abs() * VIEW_BOB.y, 0.0) * rig.walking * (1.0 - phase.braced());
    let hand = posed(poses.hand) + bob - Vec3::Y * ARM_FROM * lowered;
    let shoulder = poses.shoulder + bob;
    let (upper, fore) = (rig.moves.elbow_at.length(), (rig.moves.hand_at - rig.moves.elbow_at).length());
    let (arm, elbow) = reach_for(shoulder, hand, upper, fore, poses.pole);
    // Pointing as posed, its flat side up.
    let points = posed(poses.points).normalize();
    let flat = Vec3::Y.reject_from_normalized(points).normalize_or(Vec3::X);
    let held = Quat::from_mat3(&Mat3::from_cols(flat, points, flat.cross(points)));

    if let Ok((mut transform, _)) = parts.get_mut(view.arm) {
        transform.set_if_neq(Transform::from_translation(shoulder).with_rotation(arm));
    }
    if let Ok((mut transform, _)) = parts.get_mut(view.elbow) {
        transform.set_if_neq(Transform::from_translation(rig.moves.elbow_at).with_rotation(elbow));
    }
    if let Ok((mut transform, mut visibility)) = parts.get_mut(view.held) {
        transform.set_if_neq(Transform::from_translation(rig.moves.hand_at - rig.moves.elbow_at).with_rotation((arm * elbow).inverse() * held));
        visibility.set_if_neq(crate::render::shown(armed));
    }
}
