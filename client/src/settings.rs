//! The player's settings (`Settings`): how fast the mouse turns the action camera, and how loud
//! the game is. Kept in the browser (`localStorage`) across reloads, and set from the ESC
//! menu's settings pane (`esc_menu`): a slider each (`slider`), dragged with the mouse
//! (`drag_sliders`).

use bevy::prelude::*;

use crate::arena::palette;
use crate::arena::palette::ui::ACCENT;
use crate::render::ui_text;
use crate::rooms::{load_setting, save_setting};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::load());
        app.add_systems(Update, (drag_sliders, show_settings).chain());
    }
}

#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct Settings {
    /// Times the usual turn of the camera per mouse movement.
    pub sensitivity: f32,
    /// 0 to 1, of the usual loudness.
    pub volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { sensitivity: 1.0, volume: 1.0 }
    }
}

const SENSITIVITY_KEY: &str = "sensitivity";
const VOLUME_KEY: &str = "volume";
/// What the sensitivity slider runs between.
const SENSITIVITY_RANGE: (f32, f32) = (0.2, 3.0);

impl Settings {
    /// As the browser kept them, where it did; the usual otherwise.
    fn load() -> Settings {
        let usual = Settings::default();
        let read = |key: &str, setting: Setting, fallback: f32| {
            let (lo, hi) = setting.range();
            load_setting(key).and_then(|v| v.parse::<f32>().ok()).map_or(fallback, |v| v.clamp(lo, hi))
        };
        Settings { sensitivity: read(SENSITIVITY_KEY, Setting::Sensitivity, usual.sensitivity), volume: read(VOLUME_KEY, Setting::Volume, usual.volume) }
    }

    fn save(&self) {
        save_setting(SENSITIVITY_KEY, &self.sensitivity.to_string());
        save_setting(VOLUME_KEY, &self.volume.to_string());
    }

    fn get(&self, setting: Setting) -> f32 {
        match setting {
            Setting::Sensitivity => self.sensitivity,
            Setting::Volume => self.volume,
        }
    }

    fn set(&mut self, setting: Setting, value: f32) {
        match setting {
            Setting::Sensitivity => self.sensitivity = value,
            Setting::Volume => self.volume = value,
        }
    }
}

/// One of the settings, as a slider sets it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    Sensitivity,
    Volume,
}

impl Setting {
    fn range(self) -> (f32, f32) {
        match self {
            Setting::Sensitivity => SENSITIVITY_RANGE,
            Setting::Volume => (0.0, 1.0),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Setting::Sensitivity => "Mouse sensitivity",
            Setting::Volume => "Volume",
        }
    }

    /// Its value, as players read it.
    fn reads(self, value: f32) -> String {
        match self {
            Setting::Sensitivity => format!("{value:.2}x"),
            Setting::Volume => format!("{:.0}%", value * 100.0),
        }
    }

    /// How far along the slider (0 to 1) `value` is.
    fn along(self, value: f32) -> f32 {
        let (lo, hi) = self.range();
        ((value - lo) / (hi - lo)).clamp(0.0, 1.0)
    }
}

/// A slider's track: pressed on, and dragged along, it takes the value under the mouse.
#[derive(Component)]
struct Slider(Setting);

/// The knob on a slider, at its value.
#[derive(Component)]
struct Knob(Setting);

/// A slider's value, as read beside it.
#[derive(Component)]
struct Reading(Setting);

/// A setting's row under `parent`: its label and reading over a slider.
pub(crate) fn slider(parent: &mut ChildSpawnerCommands, setting: Setting, settings: &Settings) {
    let value = settings.get(setting);
    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4.0), width: px(300.0), ..default() }).with_children(|row| {
        row.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|over| {
            over.spawn(ui_text(setting.label(), 12.0, palette::ui::TEXT));
            over.spawn((Reading(setting), ui_text(setting.reads(value), 12.0, ACCENT)));
        });
        row.spawn((Slider(setting), Node { width: percent(100.0), height: px(18.0), padding: UiRect::vertical(px(7.0)), ..default() }))
            .with_children(|track| {
                track.spawn((Node { width: percent(100.0), height: px(4.0), ..default() }, BackgroundColor(palette::ui::MUTED.with_alpha(0.5))));
                track.spawn((
                    Knob(setting),
                    Node { position_type: PositionType::Absolute, left: percent(setting.along(value) * 100.0), top: px(3.0), width: px(12.0), height: px(12.0), margin: UiRect::left(px(-6.0)), border_radius: BorderRadius::all(px(6.0)), ..default() },
                    BackgroundColor(ACCENT),
                ));
            });
    });
}

/// While the mouse is down on a slider (from where it went down on it, until it's let go, even
/// off the track), the setting is where the mouse is along it; let go, it's kept. The track
/// under the mouse is found from its laid-out rectangle, as the camera finds the HUD.
fn drag_sliders(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    mut settings: ResMut<Settings>,
    sliders: Query<(&Slider, &ComputedNode, &UiGlobalTransform, &InheritedVisibility)>,
    mut held: Local<Option<Setting>>,
) {
    if mouse.just_released(MouseButton::Left) && held.take().is_some() {
        settings.save();
    }
    if !mouse.pressed(MouseButton::Left) {
        return;
    }
    let Some(cursor) = window.physical_cursor_position() else { return };
    if held.is_none() && mouse.just_pressed(MouseButton::Left) {
        *held = sliders.iter().find(|(_, node, place, visible)| visible.get() && node.contains_point(**place, cursor)).map(|(slider, ..)| slider.0);
    }
    let Some(setting) = *held else { return };
    let Some((_, node, place, _)) = sliders.iter().find(|(slider, ..)| slider.0 == setting) else { return };
    let width = node.size().x.max(1.0);
    let along = ((cursor.x - (place.translation.x - width / 2.0)) / width).clamp(0.0, 1.0);
    let (lo, hi) = setting.range();
    let value = lo + along * (hi - lo);
    if settings.get(setting) != value {
        settings.set(setting, value);
    }
}

/// The knobs and readings follow the settings.
fn show_settings(settings: Res<Settings>, mut knobs: Query<(&Knob, &mut Node)>, mut readings: Query<(&Reading, &mut Text)>) {
    if !settings.is_changed() {
        return;
    }
    for (knob, mut node) in &mut knobs {
        node.left = percent(knob.0.along(settings.get(knob.0)) * 100.0);
    }
    for (reading, mut text) in &mut readings {
        text.0 = reading.0.reads(settings.get(reading.0));
    }
}
