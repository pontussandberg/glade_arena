//! Normal cast: an aimed ability picked from its icon first shows where it will go (its aim
//! indicator, on the ground at your feet) and casts on the next left click (a right click drops
//! it). An ability's key sends it off at once. The keys themselves are read in
//! `render::read_local_input`.

use arena_shared::protocol::*;
use bevy::prelude::*;

use crate::plain::PlainMaterial;
use crate::{DesiredInput, Me};
use crate::arena::{self, palette};
use crate::render::shown;

pub struct CastingPlugin;

impl Plugin for CastingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Aiming>();
        app.add_systems(Update, (spawn_indicator, show_indicator).chain().after(crate::rig::Posing));
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

/// Where one of our abilities will go, on the ground, while a normal cast of it waits for its
/// click.
#[derive(Component)]
struct AimIndicator {
    slot: AbilitySlot,
    /// Whether it turns with the aim (`abilities::Indicator::aimed`).
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

/// Shows the indicator of the ability being aimed at our feet, turned the way we aim. Dropped
/// (aiming ends) once we're dead.
fn show_indicator(
    mut aiming: ResMut<Aiming>,
    desired: Res<DesiredInput>,
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
        let wanted = Transform::from_translation(arena::to_world(feet, map.map().height_at(feet) + INDICATOR_HEIGHT))
            .with_rotation(Quat::from_rotation_y(turn))
            .with_scale(Vec3::new(length, 1.0, 1.0));
        transform.set_if_neq(wanted);
    }
}
