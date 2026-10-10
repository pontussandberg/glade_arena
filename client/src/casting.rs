//! Quick cast and normal cast, LoL-style. Quick cast sends Q off toward the cursor the moment
//! it's pressed; normal cast first shows where it will go (the aim indicator, on the ground at
//! your feet) and casts it on the next left click (a right click drops it). The quick cast switch,
//! in the key hints, picks which one plain Q is; Shift+Q is always the other. Clicking the Q icon
//! is always a normal cast. The choice is kept in the browser (`localStorage`). The keys
//! themselves are read in `render::read_local_input`.

use std::f32::consts::FRAC_PI_2;

use arena_shared::classes::AbilityKind;
use arena_shared::config::PLAYER_RADIUS;
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::DesiredInput;
use crate::glade::{self, palette};
use crate::render::{HudButton, clicked, shown, ui_text};
use crate::rooms::{load_setting, save_setting};
use crate::tooltip::{Side, hover_shows, tip, tip_title};

pub struct CastingPlugin;

impl Plugin for CastingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CastMode { quick: load_quick_cast() });
        app.init_resource::<Aiming>();
        app.add_systems(
            Update,
            (
                flip_toggle,
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

/// The quick cast switch and its label, the last row of the key hints
/// (`render::update_key_hints`): clicking either turns it on or off.
#[derive(Component)]
pub(crate) struct QuickCastToggle;

/// The switch's size, and its knob's (pixels).
const SWITCH: Vec2 = Vec2::new(28.0, 14.0);
const KNOB: f32 = 8.0;
/// The extra gap between the keys and the switch (pixels).
const ROW_GAP: f32 = 6.0;
const TIP_WIDTH: f32 = 220.0;

/// The quick cast row under the key hints: a switch where the key goes (its knob right and
/// glowing while on, left and dim while off), then its name; hovering either explains it. Set a
/// little apart from the keys. The hints are rebuilt when it's flipped, so it's drawn as it is now.
pub(crate) fn spawn_toggle_row(grid: &mut ChildSpawnerCommands, quick: bool) {
    let color = if quick { palette::SPIRIT } else { palette::ui::MUTED };
    let mut tip = Entity::PLACEHOLDER;
    let switch = grid
        .spawn((
            QuickCastToggle,
            HudButton,
            Node {
                width: px(SWITCH.x),
                height: px(SWITCH.y),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::MAX,
                padding: UiRect::horizontal(px(2.0)),
                margin: UiRect::top(px(ROW_GAP)),
                align_items: AlignItems::Center,
                justify_content: if quick { JustifyContent::FlexEnd } else { JustifyContent::FlexStart },
                ..default()
            },
            BackgroundColor(if quick { palette::SPIRIT.with_alpha(0.18) } else { Color::NONE }),
            BorderColor::all(color),
        ))
        .with_children(|switch| {
            switch.spawn((
                Node { width: px(KNOB), height: px(KNOB), border_radius: BorderRadius::MAX, ..default() },
                BackgroundColor(color),
            ));
            tip = switch.spawn(toggle_tip()).id();
        })
        .id();
    grid.commands().entity(switch).insert(hover_shows(tip));
    grid.spawn((
        QuickCastToggle,
        HudButton,
        hover_shows(tip),
        Node { margin: UiRect::top(px(ROW_GAP)), ..default() },
        ui_text("Quick cast", 12.0, palette::ui::LICHEN),
    ));
}

/// What quick cast does, under the switch: Q with it on, and off.
fn toggle_tip() -> impl Bundle {
    let line = |state: &'static str, color: Color, what: &'static str| {
        (
            Node { column_gap: px(8.0), ..default() },
            children![
                (Node { width: px(26.0), ..default() }, children![ui_text(state, 13.0, color)]),
                ui_text(what, 13.0, palette::ui::LICHEN),
            ],
        )
    };
    (
        tip(Side::Below, -1.0, TIP_WIDTH),
        children![
            tip_title("Quick cast"),
            line("On", palette::SPIRIT, "Q casts instantly"),
            line("Off", palette::ui::MUTED, "Q aims first, click to cast"),
        ],
    )
}

/// A click on the switch (or its label) turns quick cast on or off, and remembers it.
fn flip_toggle(toggles: Query<Ref<Interaction>, With<QuickCastToggle>>, mut mode: ResMut<CastMode>) {
    if toggles.iter().any(clicked) {
        mode.quick = !mode.quick;
        save_quick_cast(mode.quick);
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

/// The browser keeps the choice, "on" or "off".
const QUICK_CAST_KEY: &str = "arena.quick_cast";

fn load_quick_cast() -> bool {
    load_setting(QUICK_CAST_KEY).as_deref() != Some("off")
}

fn save_quick_cast(quick: bool) {
    save_setting(QUICK_CAST_KEY, if quick { "on" } else { "off" });
}
