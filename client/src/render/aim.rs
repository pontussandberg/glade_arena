//! Action mode's aim: toward the cursor, from your fighter. Where the auto-attack will land is
//! drawn as it winds up (`show_attack_aim`), turned toward the cursor: fading in over the
//! windup, lit bright for a moment as the attack goes off, and gone between attacks. A blow
//! lands on the ground, so its lane lies there at your feet, as wide as it is and as far as it
//! reaches (longer and wider while the `follow_up` empowers it), the way an ability's indicator
//! does while a normal cast waits for its click. A shot flies at chest height, so its path is a
//! faint line up there, from where it leaves you out to its range, with a mark at the end.
//! Either is the aim itself, so it runs through anything in the way, as the aim never bends for
//! a wall; the attack stops where the sim stops it. While a normal cast is being aimed the
//! ability's own indicator takes its place (`casting::show_indicator`).

use super::*;

use arena_shared::classes::{AttackDef, AttackKind};

use crate::Me;
use crate::camera::CameraMode;
use crate::ease::ease_out;
use crate::feedback::AttackClock;

/// How high above the ground a blow's lane lies, and how strongly it glows.
const LANE_HEIGHT: f32 = 0.05;
const LANE_GLOW: f32 = 1.5;
/// How long (ticks) the aim stays lit bright after the attack goes off.
const BLINK_TICKS: f32 = 3.2;
/// How wide (px) the mark at the end of a shot's line is.
pub(super) const AIM_END_SIZE: f32 = 10.0;

/// The mark at the end of a shot's line.
#[derive(Component)]
pub(super) struct AimEnd;

/// Where our blow lands: its lane plain, and empowered by its `follow_up`. Only a melee class
/// has one; a shot's path is a line (`shot_line`).
#[derive(Component)]
pub(super) struct AttackLane {
    plain: Handle<Mesh>,
    empowered: Handle<Mesh>,
}

/// The lane a blow of `class` covers, `empowered` or not, along +X from the attacker's center,
/// if it's a melee class.
fn blow_lane(class: ClassId, empowered: bool) -> Option<Mesh> {
    let (from, range, width) = class.def().attack.lane(empowered)?;
    Some(arena::lane_mesh(from, range, width))
}

/// How high the shot of `class` flies.
fn shot_height_of(class: ClassId) -> f32 {
    shot_height(arena::thrown(class.def(), false))
}

/// Where a shot of `class`, sent from `me` toward `dir`, starts and ends: from the edge of you
/// out to its range. `None` for a melee class.
fn shot_line(class: ClassId, me: Vec2, dir: Vec2) -> Option<(Vec2, Vec2)> {
    let dir = dir.try_normalize()?;
    let AttackKind::Projectile { radius, range, .. } = class.def().attack.kind else { return None };
    let (from, to) = (PLAYER_RADIUS + radius, PLAYER_RADIUS + radius + range);
    Some((me + dir * from, me + dir * to))
}

/// Builds the blow's lane once our own player exists, if its class swings one.
pub(super) fn spawn_attack_lane(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    me: Query<&ClassId, Added<Me>>,
    lanes: Query<Entity, With<AttackLane>>,
) {
    let Ok(class) = me.single() else { return };
    // A new fighter (back from the lobby, or another class): its own attack's.
    for old in &lanes {
        commands.entity(old).despawn();
    }
    let (Some(plain), Some(empowered)) = (blow_lane(*class, false), blow_lane(*class, true)) else { return };
    let plain = meshes.add(plain);
    commands.spawn((
        AttackLane { plain: plain.clone(), empowered: meshes.add(empowered) },
        Mesh3d(plain),
        MeshMaterial3d(materials.add(arena::translucent(palette::SILVER, 0.22, LANE_GLOW))),
        Transform::default(),
        Visibility::Hidden,
    ));
}

/// How the aim shows as the attack winds up and goes off: how strong (0 to 1) it's drawn, and
/// whether it's lit up bright (the attack just went off).
struct Glow {
    strength: f32,
    bright: bool,
}

/// The aim's glow at `now` (a fractional tick) for an attack in `state`, defined by `def`:
/// fading in over the windup (eased out, so it's mostly there early and settles), bright for
/// `BLINK_TICKS` once the attack has gone off, and none at all otherwise. The news that it went
/// off can reach us a frame before our clock gets there: full, not yet bright, till then.
fn aim_glow(state: &AttackState, def: &AttackDef, now: f32) -> Option<Glow> {
    let swing = state.swing?;
    if state.released() {
        let since = now - state.released_at? as f32;
        return (since <= BLINK_TICKS).then(|| Glow { strength: 1.0, bright: since >= 0.0 });
    }
    Some(Glow { strength: ease_out(0.0, 1.0, swing.windup_progress(now, def)), bright: false })
}

/// Shows where our auto-attack will land, toward the cursor, as it winds up: in action mode,
/// while no ability is being aimed and we're alive. A blow's lane at our feet, or a shot's line
/// and the mark at its end, fading in over the windup and lit bright a moment as the attack goes
/// off (`aim_glow`); nothing between attacks.
#[allow(clippy::type_complexity)]
pub(super) fn show_attack_aim(
    mode: Res<CameraMode>,
    aiming: Res<crate::casting::Aiming>,
    desired: Res<crate::DesiredInput>,
    clock: AttackClock,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::camera::CameraRig>>,
    me: Query<(&Pos, &Transform, &ClassId, &Health, &AttackState), (With<Me>, With<PlayerId>, Without<AttackLane>)>,
    lane: Option<Single<(&AttackLane, &mut Mesh3d, &MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility), Without<AimEnd>>>,
    end: Single<(&mut UiTransform, &mut BackgroundColor, &mut BorderColor, &mut Visibility), With<AimEnd>>,
    mut gizmos: Gizmos,
) {
    let (camera, camera_transform) = *camera;
    let (mut place, mut fill, mut rim, mut end_visibility) = end.into_inner();
    let now = clock.now(true);
    let me = me.single().ok().filter(|(.., health, _)| health.alive() && mode.free && aiming.0.is_none());
    let glow = me.and_then(|(.., class, _, attack)| aim_glow(attack, &class.def().attack, now));
    let me = me.zip(glow);
    // Toward the cursor (the windup follows it), and once the attack has gone off, the way it
    // went.
    let aim = match &me {
        Some(((.., attack), glow)) if glow.bright => attack.swing.map_or(desired.0.aim, |swing| swing.dir),
        _ => desired.0.aim,
    };
    let line = me.as_ref().and_then(|((pos, _, class, ..), _)| shot_line(**class, pos.0, aim).map(|line| (line, shot_height_of(**class))));
    // A shot's line, and its end mark where the line's end is on the screen.
    let end_at = line.and_then(|((_, to), height)| camera.world_to_viewport(camera_transform, to_world(to, height)).ok());
    end_visibility.set_if_neq(shown(end_at.is_some()));
    if let (Some(((from, to), height)), Some((_, glow))) = (line, &me) {
        let color = if glow.bright { Color::WHITE.with_alpha(0.9) } else { palette::ui::TEXT.with_alpha(0.35 * glow.strength) };
        gizmos.line(to_world(from, height), to_world(to, height), color);
        let (inside, edge) = if glow.bright {
            (palette::ui::TEXT.with_alpha(0.6), Color::WHITE)
        } else {
            (palette::ui::BACKDROP.with_alpha(0.35 * glow.strength), palette::ui::TEXT.with_alpha(0.7 * glow.strength))
        };
        fill.set_if_neq(BackgroundColor(inside));
        rim.set_if_neq(BorderColor::all(edge));
    }
    if let Some(at) = end_at {
        place.set_if_neq(UiTransform { translation: Val2::px(at.x, at.y), ..default() });
    }
    // A blow's lane at our feet.
    let Some(lane) = lane else { return };
    let (lane, mut mesh, material, mut transform, mut visibility) = lane.into_inner();
    let blow = me.filter(|_| line.is_none());
    visibility.set_if_neq(shown(blow.is_some()));
    let Some(((_, at, _, _, attack), glow)) = blow else { return };
    let wanted = if attack.empowered_at(now) { &lane.empowered } else { &lane.plain };
    if mesh.0 != *wanted {
        mesh.0 = wanted.clone();
    }
    // Only touched when it changes: a touched material is sent to the GPU again.
    let color = if glow.bright { Color::WHITE.with_alpha(0.6) } else { palette::SILVER.with_alpha(0.22 * glow.strength) };
    if materials.get(&material.0).is_some_and(|lit| lit.base_color != color)
        && let Some(mut lit) = materials.get_mut(&material.0)
    {
        lit.base_color = color;
        lit.emissive = LinearRgba::from(color) * LANE_GLOW;
    }
    let turn = aim.try_normalize().map_or(0.0, |aim| aim.to_angle());
    transform.set_if_neq(Transform::from_xyz(at.translation.x, LANE_HEIGHT, at.translation.z).with_rotation(Quat::from_rotation_y(turn)));
}
