//! Quick cast and normal cast, LoL-style, for every ability (1, 2, 3, 4). Quick cast sends an
//! ability off toward the cursor the moment its key is pressed; normal cast first shows where it
//! will go (its aim indicator, on the ground at your feet) and casts it on the next left click (a
//! right click drops it, without walking). The quick cast switch, in the key hints, picks which
//! one a plain key is; with Shift it's always the other. Clicking an ability's icon is always a
//! normal cast. The choice is kept in the browser (`localStorage`). The keys themselves are read
//! in `render::read_local_input`.

use arena_shared::protocol::*;
use bevy::prelude::*;

use crate::plain::PlainMaterial;
use crate::{DesiredInput, Me};
use crate::arena::{self, palette};
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

/// Whether a plain ability key is a quick cast (on by default). With Shift it's then a normal
/// cast, and the other way round when it's off.
#[derive(Resource, Clone, Copy)]
pub(crate) struct CastMode {
    pub quick: bool,
}

impl CastMode {
    /// Whether an ability's key pressed (with Shift held or not) casts right away.
    pub(crate) fn casts_now(self, shift: bool) -> bool {
        self.quick != shift
    }
}

/// The ability a normal cast is waiting to cast with a left click, if any: its aim indicator is
/// up.
#[derive(Resource, Default)]
pub(crate) struct Aiming(pub Option<AbilitySlot>);

/// The key each ability slot is on, and how players read it. As many as a class may have
/// abilities (`MAX_ABILITIES`, checked as the class files load).
const ABILITY_KEYS: [(KeyCode, &str); MAX_ABILITIES] =
    [(KeyCode::Digit1, "1"), (KeyCode::Digit2, "2"), (KeyCode::Digit3, "3"), (KeyCode::Digit4, "4")];

/// The key the ability in `slot` is on.
pub(crate) fn ability_key(slot: AbilitySlot) -> KeyCode {
    ABILITY_KEYS[slot.0 as usize].0
}

/// The key the ability in `slot` is on, as players read it.
pub(crate) fn key_label(slot: AbilitySlot) -> &'static str {
    ABILITY_KEYS[slot.0 as usize].1
}

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
        ui_text("Quick cast", 12.0, palette::ui::TEXT),
    ));
}

/// What quick cast does, under the switch: an ability's key with it on, and off.
fn toggle_tip() -> impl Bundle {
    let line = |state: &'static str, color: Color, what: &'static str| {
        (
            Node { column_gap: px(8.0), ..default() },
            children![
                (Node { width: px(26.0), ..default() }, children![ui_text(state, 13.0, color)]),
                ui_text(what, 13.0, palette::ui::TEXT),
            ],
        )
    };
    (
        tip(Side::Below, -1.0, TIP_WIDTH),
        children![
            tip_title("Quick cast"),
            line("On", palette::SPIRIT, "An ability's key casts it instantly"),
            line("Off", palette::ui::MUTED, "It aims first, click to cast"),
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

/// Where one of our abilities will go, on the ground, while a normal cast of it waits for its
/// click.
#[derive(Component)]
struct AimIndicator {
    slot: AbilitySlot,
    /// Whether it turns toward the cursor (`abilities::Indicator::aimed`).
    aimed: bool,
}

/// How high above the ground the indicators lie.
const INDICATOR_HEIGHT: f32 = 0.06;

/// Builds the indicators once our own player exists, one for each of its class's abilities
/// (`abilities::indicator`).
fn spawn_indicator(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<PlainMaterial>>,
    me: Query<&ClassId, Added<Me>>,
    indicators: Query<Entity, With<AimIndicator>>,
) {
    let Ok(class) = me.single() else { return };
    // A new fighter (back from the lobby): its own abilities' indicators.
    for old in &indicators {
        commands.entity(old).despawn();
    }
    let material = materials.add(arena::translucent(palette::SPIRIT, 0.25, 1.5));
    for (slot, ability) in class.def().abilities() {
        let crate::abilities::Indicator { mesh, aimed } = crate::abilities::indicator(&ability.kind);
        commands.spawn((
            AimIndicator { slot, aimed },
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

/// Shows the indicator of the ability being aimed at our feet, turned toward the cursor; a
/// strike's where it would land (`CameraMode::strike_aim`). Dropped (aiming ends) once we're
/// dead.
fn show_indicator(
    mut aiming: ResMut<Aiming>,
    desired: Res<DesiredInput>,
    mode: Res<crate::camera::CameraMode>,
    map: Res<crate::rooms::CurrentMap>,
    me: Query<(&Transform, &Health, &ClassId), (With<Me>, With<PlayerId>, Without<AimIndicator>)>,
    mut indicators: Query<(&AimIndicator, &mut Transform, &mut Visibility)>,
) {
    let me = me.single().ok().filter(|(_, health, _)| health.alive());
    if me.is_none() && aiming.0.is_some() {
        aiming.0 = None;
    }
    for (indicator, mut transform, mut visibility) in &mut indicators {
        let showing = aiming.0 == Some(indicator.slot);
        visibility.set_if_neq(shown(showing));
        let Some((at, _, class)) = me.filter(|_| showing) else { continue };
        let Some(ability) = class.def().ability(indicator.slot) else { continue };
        let turn = if indicator.aimed { desired.0.aim.to_angle() } else { 0.0 };
        let length = crate::abilities::indicator_length(&ability.kind, desired.0.aim);
        let feet = arena::to_gameplay(at.translation);
        let place = match ability.kind {
            arena_shared::abilities::AbilityKind::Strike(strike) => feet + mode.strike_aim(feet, strike.range),
            _ => feet,
        };
        let wanted = Transform::from_translation(arena::to_world(place, map.map().height_at(place) + INDICATOR_HEIGHT))
            .with_rotation(Quat::from_rotation_y(turn))
            .with_scale(Vec3::new(length, 1.0, 1.0));
        transform.set_if_neq(wanted);
    }
}

/// The browser keeps the choice, "on" or "off".
const QUICK_CAST_KEY: &str = "arena.quick_cast";

fn load_quick_cast() -> bool {
    load_setting(QUICK_CAST_KEY).as_deref() != Some("off")
}

fn save_quick_cast(quick: bool) {
    save_setting(QUICK_CAST_KEY, if quick { "on" } else { "off" });
}
