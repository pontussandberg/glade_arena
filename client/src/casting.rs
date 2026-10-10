//! Quick cast and normal cast, LoL-style. Quick cast sends Q off toward the cursor the moment
//! it's pressed; normal cast first shows where it will go (the aim indicator, on the ground at
//! your feet) and casts it on the next left click (a right click drops it). The pill beside the Q
//! icon picks which one plain Q is; Shift+Q is always the other.
//! Clicking the Q icon is always a normal cast. The choice is kept in the
//! browser (`localStorage`). The keys themselves are read in `render::read_local_input`.

use std::f32::consts::FRAC_PI_2;

use arena_shared::classes::AbilityKind;
use arena_shared::config::PLAYER_RADIUS;
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::DesiredInput;
use crate::action_bar::{Tooltip, tip_panel};
use crate::glade::{self, palette};
use crate::minimap;
use crate::render::{clicked, shown, ui_text};

pub struct CastingPlugin;

impl Plugin for CastingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CastMode { quick: load_quick_cast() });
        app.init_resource::<Aiming>();
        app.add_systems(
            Update,
            (
                (spawn_toggle, flip_toggle, light_toggle).chain(),
                (spawn_indicator, show_indicator).chain().after(crate::rig::Posing),
            ),
        );
    }
}

/// Whether plain Q is a quick cast (on by default). Shift+Q is then a normal cast, and the other
/// way round when it's off.
#[derive(Resource, Clone, Copy)]
pub(crate) struct CastMode {
    pub quick: bool,
}

impl CastMode {
    /// Whether a Q press (with Shift held or not) casts right away.
    pub(crate) fn casts_now(self, shift: bool) -> bool {
        self.quick != shift
    }
}

/// A normal cast waiting for its left click: the aim indicator is up.
#[derive(Resource, Default)]
pub(crate) struct Aiming(pub bool);

/// The quick cast pill, just above the minimap, at its right edge (a button).
#[derive(Component)]
pub(crate) struct QuickCastToggle {
    label: Entity,
}

/// The pill's size and its gap above the minimap (pixels).
const PILL_WIDTH: f32 = 84.0;
const PILL_HEIGHT: f32 = 22.0;
const PILL_GAP: f32 = 6.0;
const TIP_WIDTH: f32 = 280.0;

/// Builds the pill once our own player exists, over the minimap's top right corner.
fn spawn_toggle(mut commands: Commands, me: Query<(), Added<Predicted>>, toggles: Query<(), With<QuickCastToggle>>) {
    if me.is_empty() || !toggles.is_empty() {
        return;
    }
    let label = commands.spawn(ui_text("Quick cast", 11.0, palette::HAZE)).id();
    let tip = commands
        .spawn(tip_panel(Node {
            position_type: PositionType::Absolute,
            bottom: px(PILL_HEIGHT + 10.0),
            // Right edges lined up, so it stays on the screen.
            right: px(0.0),
            width: px(TIP_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            padding: UiRect::all(px(10.0)),
            ..default()
        }))
        .with_children(|tip| {
            tip.spawn(ui_text("Quick cast", 15.0, palette::HAZE));
            tip.spawn(ui_text("Click to turn on or off", 11.0, palette::STONE));
            tip.spawn(ui_text(
                "On: Q casts your ability right away, toward the cursor. Shift+Q shows where it will \
                 go first: left click casts it, right click cancels. Clicking the ability's icon \
                 always shows where it will go.",
                13.0,
                palette::HAZE,
            ));
            tip.spawn(ui_text("Off: the other way round. Q shows where it will go, Shift+Q casts right away.", 13.0, palette::HAZE));
        })
        .id();
    commands
        .spawn((
            QuickCastToggle { label },
            crate::render::GameUi,
            Button,
            Tooltip(tip),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(minimap::MARGIN + minimap::picture_size().y + PILL_GAP),
                right: px(minimap::MARGIN),
                width: px(PILL_WIDTH),
                height: px(PILL_HEIGHT),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::MAX,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(palette::INK.with_alpha(0.9)),
            BorderColor::all(palette::STONE),
        ))
        .add_children(&[label, tip]);
}

/// A click on the pill turns quick cast on or off, and remembers it.
fn flip_toggle(toggle: Query<Ref<Interaction>, With<QuickCastToggle>>, mut mode: ResMut<CastMode>) {
    if toggle.single().is_ok_and(clicked) {
        mode.quick = !mode.quick;
        save_quick_cast(mode.quick);
    }
}

/// The pill glows spirit blue while quick cast is on, and is dim stone while it's off; a little
/// brighter under the mouse.
fn light_toggle(
    mode: Res<CastMode>,
    toggles: Query<(Ref<Interaction>, &QuickCastToggle, &mut BackgroundColor, &mut BorderColor)>,
    mut texts: Query<&mut TextColor>,
) {
    for (interaction, toggle, mut background, mut border) in toggles {
        if !mode.is_changed() && !interaction.is_changed() {
            continue;
        }
        let color = if mode.quick { palette::SPIRIT } else { palette::STONE };
        let fill = if mode.quick { palette::SPIRIT.with_alpha(0.18) } else { palette::INK.with_alpha(0.9) };
        let fill = if *interaction == Interaction::None { fill } else { fill.lighter(0.08) };
        background.set_if_neq(BackgroundColor(fill));
        border.set_if_neq(BorderColor::all(color));
        if let Ok(mut text) = texts.get_mut(toggle.label) {
            text.set_if_neq(TextColor(color));
        }
    }
}

/// Where your Q will go, on the ground, while a normal cast waits for its click.
#[derive(Component)]
struct AimIndicator {
    /// Whether it turns toward the cursor (a thrown ability or a dash; a nova is all around).
    aimed: bool,
}

/// How high above the ground the indicator lies.
const INDICATOR_HEIGHT: f32 = 0.06;

/// Builds the indicator once our own player exists, for its class's ability: the lane a thrown
/// ability flies down to the edge of its range, the lane a dash crosses, or the ground a nova
/// covers. All flat, pointing along world +X.
fn spawn_indicator(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    me: Query<&ClassId, Added<Predicted>>,
    indicators: Query<Entity, With<AimIndicator>>,
) {
    let Ok(class) = me.single() else { return };
    // A new fighter (back from the lobby): its own ability's indicator.
    for old in &indicators {
        commands.entity(old).despawn();
    }
    let (mesh, aimed) = match class.def().ability.kind {
        AbilityKind::Projectile { radius, range, .. } => {
            (glade::lane_mesh(PLAYER_RADIUS, PLAYER_RADIUS + 2.0 * radius + range, (2.0 * radius).max(0.4)), true)
        }
        AbilityKind::Dash { distance, .. } => (glade::lane_mesh(0.0, distance + PLAYER_RADIUS, 2.0 * PLAYER_RADIUS), true),
        AbilityKind::Nova { radius, .. } => (
            Circle::new(radius).mesh().resolution(64).build().rotated_by(Quat::from_rotation_x(-FRAC_PI_2)),
            false,
        ),
    };
    commands.spawn((
        AimIndicator { aimed },
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(glade::translucent(palette::SPIRIT, 0.25, 1.5))),
        Transform::default(),
        Visibility::Hidden,
    ));
}

/// Shows the indicator at our feet while aiming, turned toward the cursor. Dropped (aiming
/// ends) once we're dead.
fn show_indicator(
    mut aiming: ResMut<Aiming>,
    desired: Res<DesiredInput>,
    me: Query<(&Transform, &Health), (With<Predicted>, With<PlayerId>, Without<AimIndicator>)>,
    indicator: Single<(&AimIndicator, &mut Transform, &mut Visibility)>,
) {
    let (indicator, mut transform, mut visibility) = indicator.into_inner();
    let me = me.single().ok().filter(|(_, health)| health.alive());
    if me.is_none() && aiming.0 {
        aiming.0 = false;
    }
    visibility.set_if_neq(shown(aiming.0));
    let Some((at, _)) = me.filter(|_| aiming.0) else { return };
    let turn = if indicator.aimed { desired.0.aim.to_angle() } else { 0.0 };
    let wanted = Transform::from_xyz(at.translation.x, INDICATOR_HEIGHT, at.translation.z)
        .with_rotation(Quat::from_rotation_y(turn));
    transform.set_if_neq(wanted);
}

/// The browser keeps the choice (`localStorage`), "on" or "off".
const QUICK_CAST_KEY: &str = "arena.quick_cast";

fn load_quick_cast() -> bool {
    #[cfg(target_family = "wasm")]
    {
        let stored = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(QUICK_CAST_KEY).ok().flatten());
        stored.as_deref() != Some("off")
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = QUICK_CAST_KEY;
        true
    }
}

fn save_quick_cast(quick: bool) {
    #[cfg(target_family = "wasm")]
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(QUICK_CAST_KEY, if quick { "on" } else { "off" });
    }
    #[cfg(not(target_family = "wasm"))]
    let _ = quick;
}
