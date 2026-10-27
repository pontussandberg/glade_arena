//! Action mode's aim: toward the cursor, from your fighter. Where the auto-attack will land is
//! drawn on the ground at your feet (`show_attack_indicator`), turned toward the cursor, the
//! way an ability's indicator is while a normal cast waits for its click: a blow's lane, as wide
//! as it is and as far as it reaches (longer and wider while the `follow_up` empowers it), or a
//! shot's lane out to the edge of its range. It is the aim itself, so it runs through anything
//! in the way, as the aim never bends for a wall; the attack stops where the sim stops it.
//! While a normal cast is being aimed the ability's own indicator takes its place
//! (`casting::show_indicator`).

use super::*;

use arena_shared::classes::AttackKind;

use crate::Me;
use crate::camera::CameraMode;
use crate::feedback::AttackClock;

/// How high above the ground the indicator lies.
const INDICATOR_HEIGHT: f32 = 0.05;

/// Where our auto-attack lands: its shape plain, and empowered by its `follow_up`.
#[derive(Component)]
pub(super) struct AttackIndicator {
    plain: Handle<Mesh>,
    empowered: Handle<Mesh>,
}

/// The lane an auto-attack of `class` covers, `empowered` or not, along +X from the attacker's
/// center: a blow's own, a shot's from the edge of the attacker to the edge of its range, as
/// wide as the shot (no thinner than a needle to see).
fn attack_lane(class: ClassId, empowered: bool) -> Mesh {
    let attack = &class.def().attack;
    match attack.kind {
        AttackKind::Melee { .. } => {
            let (from, range, width) = attack.lane(empowered).expect("a melee attack has a lane");
            arena::lane_mesh(from, range, width)
        }
        AttackKind::Projectile { radius, .. } => {
            let shot = attack.kind.shot().expect("a projectile attack is a shot");
            arena::lane_mesh(PLAYER_RADIUS, shot.edge_reach(), (2.0 * radius).max(0.4))
        }
    }
}

/// Builds the indicator once our own player exists, for its class's auto-attack.
pub(super) fn spawn_attack_indicator(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    me: Query<&ClassId, Added<Me>>,
    indicators: Query<Entity, With<AttackIndicator>>,
) {
    let Ok(class) = me.single() else { return };
    // A new fighter (back from the lobby, or another class): its own attack's.
    for old in &indicators {
        commands.entity(old).despawn();
    }
    let plain = meshes.add(attack_lane(*class, false));
    commands.spawn((
        AttackIndicator { plain: plain.clone(), empowered: meshes.add(attack_lane(*class, true)) },
        Mesh3d(plain),
        MeshMaterial3d(materials.add(arena::translucent(palette::SILVER, 0.22, 1.5))),
        Transform::default(),
        Visibility::Hidden,
    ));
}

/// Shows where our auto-attack will land, at our feet, turned toward the cursor: in action
/// mode, while no ability is being aimed and we're alive.
pub(super) fn show_attack_indicator(
    mode: Res<CameraMode>,
    aiming: Res<crate::casting::Aiming>,
    desired: Res<crate::DesiredInput>,
    clock: AttackClock,
    me: Query<(&Transform, &Health, &AttackState), (With<Me>, With<PlayerId>, Without<AttackIndicator>)>,
    indicator: Single<(&AttackIndicator, &mut Mesh3d, &mut Transform, &mut Visibility)>,
) {
    let (indicator, mut mesh, mut transform, mut visibility) = indicator.into_inner();
    let me = me.single().ok().filter(|(_, health, _)| health.alive() && mode.free && aiming.0.is_none());
    visibility.set_if_neq(shown(me.is_some()));
    let Some((at, _, attack)) = me else { return };
    let wanted = if attack.empowered_at(clock.now(true)) { &indicator.empowered } else { &indicator.plain };
    if mesh.0 != *wanted {
        mesh.0 = wanted.clone();
    }
    let turn = desired.0.aim.try_normalize().map_or(0.0, |aim| aim.to_angle());
    transform.set_if_neq(Transform::from_xyz(at.translation.x, INDICATOR_HEIGHT, at.translation.z).with_rotation(Quat::from_rotation_y(turn)));
}
