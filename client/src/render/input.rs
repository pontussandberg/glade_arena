//! Our own input on the field: clicks and keys into orders (`read_local_input`), where a move
//! is going, and the attack's range circle.

use super::*;

use arena_shared::abilities::AbilityKind;
use arena_shared::config::PLAYER_RADIUS;
use arena_shared::sim;

use crate::Me;
use crate::{DesiredInput, StopWalking};
use crate::action_bar::AbilityIcon;
use crate::camera::{CameraMode, key_axis};
use crate::casting::{Aiming, CastMode, ability_key};
use crate::feedback::AttackClock;
use crate::arena::{to_gameplay, to_world};

/// How wide (meters) the circle at the edge of your auto-attack's range is.
pub(super) const RANGE_CIRCLE_WIDTH: f32 = 0.05;

/// Shows where we're walking to.
#[derive(Component)]
pub(super) struct DestinationMarker;

/// The faint circle at the edge of your auto-attack's range: shown with A (MOBA camera), hidden again
/// by the next key or click (`toggle_range_circle`).
#[derive(Component)]
pub(super) struct RangeCircle;

/// Mouse -> `DesiredInput`, LoL-style: right click walks to the clicked point, left click
/// attacks toward the cursor. An ability's key (1, 2, 3, 4) casts it right away (quick cast) or
/// first shows where it will go, cast by the next left click and dropped by a right click
/// (normal cast); with Shift it's the other one (see `casting.rs`). Clicking an ability's icon is
/// always a normal cast.
///
/// With the action camera (`CameraMode::free`) you aim where it looks, WASD walks, and holding
/// the left button attacks again each time the last attack is done (the server drops a click
/// before then, so it's asked each tick until it takes). A click that only captures the cursor
/// doesn't attack.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_local_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Option<Single<&Window>>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::camera::CameraRig>>,
    me: Query<(&Pos, &ClassId, &Cooldowns, &AttackState), (With<Me>, With<PlayerId>)>,
    (mode, cast_mode, clock, map): (Res<CameraMode>, Res<CastMode>, AttackClock, Res<crate::rooms::CurrentMap>),
    hud: Query<
        (&ComputedNode, &UiGlobalTransform, &InheritedVisibility, Option<&AbilityIcon>),
        Or<(With<HudButton>, With<AbilityIcon>)>,
    >,
    minimap: Option<Single<(&ComputedNode, &UiGlobalTransform, &InheritedVisibility), With<crate::minimap::Minimap>>>,
    mut aiming: ResMut<Aiming>,
    mut held_click: Local<bool>,
    mut last_pos: Local<Option<Vec2>>,
    (mut desired, mut stop): (ResMut<DesiredInput>, ResMut<StopWalking>),
) {
    let me = me.single().ok();
    // Our abilities. A normal cast only aims while its ability is ready (the global cooldown over
    // too), as in LoL.
    let my_class = me.map(|(_, class, ..)| class.def());
    let ready = |slot: AbilitySlot| {
        me.is_some_and(|(_, class, cooldowns, _)| {
            class.def().ability(slot).is_some_and(|ability| cooldowns.usable_at(slot, ability) as f32 <= clock.now(true))
        })
    };
    // Whether our next attack may start now: none in progress, or the last one's timer run out.
    let attack_ready = me.is_some_and(|(_, class, _, attack)| attack.swing.is_none_or(|swing| swing.ready_at(&class.def().attack) as f32 <= clock.now(true)));
    let me = me.map(|(p, ..)| p.0);
    let (camera, camera_transform) = *camera;
    // Whether the cursor is on a HUD control, or on an ability's icon (and which). Tested on the
    // nodes themselves, so nothing drawn over them can get in the way.
    let screen = window.as_ref().and_then(|w| w.physical_cursor_position());
    let under_cursor = |(node, transform, visible, _): &(&ComputedNode, &UiGlobalTransform, &InheritedVisibility, Option<&AbilityIcon>)| {
        visible.get() && screen.is_some_and(|at| node.contains_point(**transform, at))
    };
    // The point on the map under the cursor on the minimap, if it's there.
    let on_minimap = minimap
        .filter(|minimap| minimap.2.get())
        .and_then(|minimap| crate::minimap::ground_on_minimap(minimap.0, *minimap.1, map.map(), screen?));
    let on_control = on_minimap.is_some() || hud.iter().filter(|part| part.3.is_none()).any(|part| under_cursor(&part));
    let on_icon = hud.iter().filter(under_cursor).find_map(|(.., icon)| icon.map(|icon| icon.slot));
    let cursor = window.and_then(|w| w.cursor_position()).and_then(|c| ground_at(camera, camera_transform, c));

    let mut move_to = desired.0.move_to;
    // A jump (respawn or server correction) cancels the old destination. (So does attacking:
    // see `forget_walk_on_attack`.)
    if let (Some(now), Some(before)) = (me, *last_pos)
        && now.distance(before) > JUMP
    {
        move_to = None;
    }
    *last_pos = me;
    // A right click while aiming a normal cast only drops the aim: it doesn't walk.
    let right_click = mouse.just_pressed(MouseButton::Right);
    let drops_aim = right_click && aiming.0.is_some();
    // Once per click: holding the button doesn't keep re-targeting. Not with the free camera:
    // there the right button turns the camera, and WASD walks. A click on the minimap walks to
    // where it is there.
    if !mode.free
        && right_click
        && !drops_aim
        && let Some(cursor) = on_minimap.or(cursor)
        && let Some(point) = map.map().walk_target(cursor, 4)
    {
        move_to = Some(point);
    }
    // Arrived: forget it, so nothing lingers.
    if let (Some(point), Some(me)) = (move_to, me)
        && sim::arrived(map.map(), me, point)
    {
        move_to = None;
    }

    // The action camera aims where it looks, up or down too (`camera::aim_of`).
    let aim = match (cursor, me) {
        _ if mode.free => mode.aim,
        (Some(cursor), Some(me)) => cursor - me,
        _ => desired.0.aim,
    };
    let climb = if mode.free { mode.climb } else { 0.0 };
    // Casts not sent yet, and new ones after them, each toward the cursor as its key went down.
    let mut casts = PlayerInput { casts: desired.0.casts, ..default() };
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for (slot, ability) in my_class.into_iter().flat_map(|def| def.abilities()) {
        if !keys.just_pressed(ability_key(slot)) {
            continue;
        }
        // In action mode a strike is always a normal cast: its ring shows where the crosshair is
        // on the ground until a left click calls it there (`cast_aim`); everything else goes
        // off at once, a dash its whole distance. Otherwise a prime has nothing to aim: its key
        // casts it at once, quick cast or not.
        let now = if mode.free { !matches!(ability.kind, AbilityKind::Strike(_)) } else { cast_mode.casts_now(shift) || !ability.kind.aimed() };
        if now {
            casts.order_cast(slot, cast_aim(&mode, &ability.kind, me, aim));
            aiming.0 = None;
        } else if ready(slot) {
            aiming.0 = Some(slot);
        }
    }
    // A left click that casts, aims (on an ability's icon) or is for a HUD control doesn't also
    // attack. Any other one does: once per click (holding it doesn't attack again). A right
    // click drops the aim (`drops_aim`); ESC does too (`esc_menu`).
    if mouse.just_pressed(MouseButton::Left) {
        if on_control || mode.just_locked {
            *held_click = true;
        } else if let Some(slot) = on_icon
            && my_class.and_then(|def| def.ability(slot)).is_some_and(|ability| !ability.kind.aimed())
        {
            casts.order_cast(slot, aim);
            *held_click = true;
        } else if let Some(slot) = on_icon {
            aiming.0 = Some(slot).filter(|&slot| ready(slot));
            *held_click = true;
        } else if let Some(slot) = aiming.0.take() {
            let kind = my_class.and_then(|def| def.ability(slot)).map(|ability| ability.kind);
            casts.order_cast(slot, kind.map_or(aim, |kind| cast_aim(&mode, &kind, me, aim)));
            *held_click = true;
        }
    }
    if drops_aim {
        aiming.0 = None;
    }
    if !mouse.pressed(MouseButton::Left) {
        *held_click = false;
    }
    // Not while the scoreboard is held up (Tab): the mouse is free then, for pointing at it.
    let board = keys.pressed(KeyCode::Tab);
    let attacking = if mode.free { mouse.pressed(MouseButton::Left) && attack_ready && !board } else { mouse.just_pressed(MouseButton::Left) };
    let fire = desired.0.fire || (attacking && aiming.0.is_none() && !*held_click);
    // With the action camera, WASD walks relative to it, dropping any destination; otherwise S
    // stops: drops the destination (as in LoL).
    let walk = if mode.free {
        let (forward, right) = mode.ground_axes();
        forward * key_axis(&keys, KeyCode::KeyW, KeyCode::KeyS) + right * key_axis(&keys, KeyCode::KeyD, KeyCode::KeyA)
    } else {
        Vec2::ZERO
    };
    if walk != Vec2::ZERO {
        move_to = None;
    }
    if !mode.free && keys.just_pressed(KeyCode::KeyS) {
        move_to = None;
        stop.0 = true;
    }
    // Space jumps (action mode), and held, jumps again each time it lands (the server jumps
    // only from the ground). `casts`, `fire` and `jump` are kept until they've been sent
    // (`send_commands` clears them), so a short tap isn't missed.
    let jump = desired.0.jump || (mode.free && keys.pressed(KeyCode::Space) && !board);
    desired.0 = PlayerInput { move_to, walk, aim, climb, fire, jump, casts: casts.casts };
}

/// What a cast of `kind` by our fighter at `me` is aimed with, aiming `aim`: in action mode a
/// dash's aim reaches its whole distance (`DashDef::length`), and a strike's is where the
/// crosshair is on the ground (`CameraMode::strike_aim`); the rest, and everything in the MOBA,
/// the aim as it is.
fn cast_aim(mode: &CameraMode, kind: &AbilityKind, me: Option<Vec2>, aim: Vec2) -> Vec2 {
    match kind {
        _ if !mode.free => aim,
        AbilityKind::Dash(dash) => aim.normalize_or(Vec2::Y) * (dash.distance + PLAYER_RADIUS),
        AbilityKind::Strike(strike) => mode.strike_aim(me.unwrap_or_default(), strike.range),
        _ => aim,
    }
}

/// The range circle shows how far our attack reaches as it is now: further while our `follow_up`
/// empowers it.
pub(super) fn fit_range_circle(
    clock: AttackClock,
    visuals: Res<Visuals>,
    me: Query<(&ClassId, &AttackState), With<Me>>,
    mut circle: Query<&mut Mesh3d, With<RangeCircle>>,
) {
    let (Ok((class, attack)), Ok(mut mesh)) = (me.single(), circle.single_mut()) else { return };
    let Some(circles) = visuals.range_circles.get(class) else { return };
    let fitting = &circles[attack.empowered_at(clock.now(true)) as usize];
    if mesh.0 != *fitting {
        mesh.0 = fitting.clone();
    }
}

/// A shows the range circle (MOBA camera only: the action camera walks with A); any key or click
/// after that, A included, hides it.
pub(super) fn toggle_range_circle(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mode: Res<CameraMode>,
    mut circle: Query<&mut Visibility, With<RangeCircle>>,
) {
    let Ok(mut visibility) = circle.single_mut() else { return };
    let showing = *visibility != Visibility::Hidden;
    let pressed = keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some();
    let show = if mode.free {
        false
    } else if showing {
        !pressed
    } else {
        keys.just_pressed(KeyCode::KeyA)
    };
    visibility.set_if_neq(shown(show));
}

/// Where we're walking to, while we are: a marker on the ground, and a ring on the minimap.
pub(super) fn show_destination(
    desired: Res<DesiredInput>,
    current: Res<crate::rooms::CurrentMap>,
    marker: Single<(&mut Transform, &mut Visibility), With<DestinationMarker>>,
    dot: Option<Single<(&mut Node, &mut Visibility), (With<crate::minimap::DestinationDot>, Without<DestinationMarker>)>>,
) {
    let (mut transform, mut visibility) = marker.into_inner();
    visibility.set_if_neq(shown(desired.0.move_to.is_some()));
    if let Some(point) = desired.0.move_to {
        transform.translation = to_world(point, 0.03);
    }
    if let Some(dot) = dot {
        let (mut node, mut visibility) = dot.into_inner();
        visibility.set_if_neq(shown(desired.0.move_to.is_some()));
        if let Some(point) = desired.0.move_to {
            crate::minimap::place_destination(&mut node, current.map(), point);
        }
    }
}

/// The point on the ground (gameplay coordinates) under a point on the screen.
pub(crate) fn ground_at(camera: &Camera, transform: &GlobalTransform, screen: Vec2) -> Option<Vec2> {
    let ray = camera.viewport_to_world(transform, screen).ok()?;
    ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)).map(to_gameplay)
}
