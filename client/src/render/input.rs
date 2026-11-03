//! Our own input on the field: clicks and keys into orders (`read_local_input`), and where a
//! move is going.

use super::*;

use arena_shared::abilities::AbilityKind;
use arena_shared::config::PLAYER_RADIUS;
use arena_shared::sim;

use crate::Me;
use crate::DesiredInput;
use crate::action_bar::AbilityIcon;
use crate::camera::{CameraMode, key_axis};
use crate::casting::{Aiming, ability_key};
use crate::feedback::AttackClock;
use crate::arena::{to_gameplay, to_world};

/// Shows where we're walking to.
#[derive(Component)]
pub(super) struct DestinationMarker;

/// Mouse and keys -> `DesiredInput`: you aim where the camera looks, WASD walks relative to it,
/// and holding the left button attacks again each time the last attack is done (the server drops
/// a click before then, so it's asked each tick until it takes). A click that only captures the
/// cursor doesn't attack. An ability's key (1, 2, 3, 4) casts it right away. Clicking its icon
/// aims it first, called by the next left click and dropped by a right click (see `casting.rs`),
/// unless it has nothing to aim.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_local_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Option<Single<&Window>>,
    me: Query<(&Pos, &ClassId, &Cooldowns, &AttackState), (With<Me>, With<PlayerId>)>,
    (mode, clock, map, minimap_view): (Res<CameraMode>, AttackClock, Res<crate::rooms::CurrentMap>, Res<crate::minimap::MinimapView>),
    hud: Query<
        (&ComputedNode, &UiGlobalTransform, &InheritedVisibility, Option<&AbilityIcon>),
        Or<(With<HudButton>, With<AbilityIcon>)>,
    >,
    minimap: Option<Single<(&ComputedNode, &UiGlobalTransform, &InheritedVisibility), With<crate::minimap::Minimap>>>,
    mut aiming: ResMut<Aiming>,
    mut held_click: Local<bool>,
    mut last_pos: Local<Option<Vec2>>,
    mut desired: ResMut<DesiredInput>,
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
    // Whether the cursor is on a HUD control, or on an ability's icon (and which). Tested on the
    // nodes themselves, so nothing drawn over them can get in the way.
    let screen = window.as_ref().and_then(|w| w.physical_cursor_position());
    let under_cursor = |(node, transform, visible, _): &(&ComputedNode, &UiGlobalTransform, &InheritedVisibility, Option<&AbilityIcon>)| {
        visible.get() && screen.is_some_and(|at| node.contains_point(**transform, at))
    };
    // The point on the map under the cursor on the minimap, if it's there.
    let on_minimap = minimap
        .filter(|minimap| minimap.2.get())
        .and_then(|minimap| crate::minimap::ground_on_minimap(minimap.0, *minimap.1, map.map(), &minimap_view, screen?));
    let on_control = on_minimap.is_some() || hud.iter().filter(|part| part.3.is_none()).any(|part| under_cursor(&part));
    let on_icon = hud.iter().filter(under_cursor).find_map(|(.., icon)| icon.map(|icon| icon.slot));

    let mut move_to = desired.0.move_to;
    // A jump (respawn or server correction) cancels the old destination. (So does attacking:
    // see `forget_walk_on_attack`.)
    if let (Some(now), Some(before)) = (me, *last_pos)
        && now.distance(before) > JUMP
    {
        move_to = None;
    }
    *last_pos = me;
    // A right click while aiming a normal cast drops the aim.
    let drops_aim = mouse.just_pressed(MouseButton::Right) && aiming.0.is_some();
    // Arrived: forget it, so nothing lingers.
    if let (Some(point), Some(me)) = (move_to, me)
        && sim::arrived(map.map(), me, point)
    {
        move_to = None;
    }

    // The aim is where the camera looks, up or down too (`camera::aim_of`).
    let (aim, climb, aside) = (mode.aim, mode.climb, mode.aside());
    // Casts not sent yet, and new ones after them, each the way the camera looked as its key
    // went down.
    let mut casts = PlayerInput { casts: desired.0.casts, ..default() };
    for (slot, ability) in my_class.into_iter().flat_map(|def| def.abilities()) {
        if !keys.just_pressed(ability_key(slot)) {
            continue;
        }
        // Goes off at once, a dash its whole distance.
        casts.order_cast(slot, cast_aim(&ability.kind, aim));
        aiming.0 = None;
    }
    // A left click that casts, aims (on an ability's icon) or is for a HUD control doesn't also
    // attack. Any other one does, and held down it attacks again each time the attack is ready
    // (one `Attack` command per attack). A right
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
            casts.order_cast(slot, kind.map_or(aim, |kind| cast_aim(&kind, aim)));
            *held_click = true;
        }
    }
    if drops_aim {
        aiming.0 = None;
    }
    if !mouse.pressed(MouseButton::Left) {
        *held_click = false;
    }
    let attacking = mouse.pressed(MouseButton::Left) && attack_ready;
    let fire = desired.0.fire || (attacking && aiming.0.is_none() && !*held_click);
    // Held down to attack (not for a control, nor to cast): a shot drawn is held so until it's
    // let go, walking slower meanwhile (`PlayerInput::draw`).
    let draw = mouse.pressed(MouseButton::Left) && aiming.0.is_none() && !*held_click;
    // WASD walks relative to the camera, dropping any destination.
    let (forward, right) = mode.ground_axes();
    let walk = forward * key_axis(&keys, KeyCode::KeyW, KeyCode::KeyS) + right * key_axis(&keys, KeyCode::KeyD, KeyCode::KeyA);
    if walk != Vec2::ZERO {
        move_to = None;
    }
    // Space jumps, and held, jumps again each time it lands (the server jumps
    // only from the ground): whether it's held, sent as that changes (`send_commands`), so a
    // tap is held for a frame at least. `casts` and `fire` are kept until they've been sent
    // (`send_commands` clears them), so a short click isn't missed.
    let jump = keys.pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Space);
    desired.0 = PlayerInput { move_to, walk, aim, climb, aside, fire, draw, jump, casts: casts.casts, ..default() };
}

/// What a cast of `kind` is aimed with, aiming `aim`: a dash's aim reaches its whole distance
/// (`DashDef::length`); the rest, the aim as it is.
fn cast_aim(kind: &AbilityKind, aim: Vec2) -> Vec2 {
    match kind {
        AbilityKind::Dash(dash) => aim.normalize_or(Vec2::Y) * (dash.distance + PLAYER_RADIUS),
        _ => aim,
    }
}

/// Where we're walking to, while we are: a marker on the ground, and a ring on the minimap.
pub(super) fn show_destination(
    desired: Res<DesiredInput>,
    current: Res<crate::rooms::CurrentMap>,
    minimap_view: Res<crate::minimap::MinimapView>,
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
            crate::minimap::place_destination(&mut node, current.map(), &minimap_view, point);
        }
    }
}

/// The point on the ground (gameplay coordinates) under a point on the screen.
pub(crate) fn ground_at(camera: &Camera, transform: &GlobalTransform, screen: Vec2) -> Option<Vec2> {
    let ray = camera.viewport_to_world(transform, screen).ok()?;
    ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)).map(to_gameplay)
}
