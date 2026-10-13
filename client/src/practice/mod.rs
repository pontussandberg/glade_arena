//! The practice tools, LoL-style: a panel in the top right corner while practicing, under a
//! "Practice" header that folds them away or out (open to begin with). Each tool is a mechanic
//! of its own, the panel only switches it:
//!
//! - "Map" switches practice to the next map, everyone starting over on it.
//! - Switches (`Switch`), lit while on: "No cooldowns" (our Q's cooldown off: what the server
//!   says, via our `AbilityState`), "Hitboxes" (`hitboxes`: what hits are judged by, drawn on the
//!   ground) and "Free camera" (`camera`: a camera to turn around fighters and look at them up
//!   close).
//! - "Target dummy" and "Moving dummy" place one (`dummies`).
//!
//! Outside practice every tool is off (`tools_off`).

mod dummies;
pub mod hitboxes;

use arena_shared::protocol::{AbilityState, RoomRequest};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::Me;
use crate::arena::palette;
use crate::camera::CameraMode;
use crate::render::{HudButton, button_fill, chevron, chevron_turn, clicked, corner_panel, fold_header, shown, ui_text};
use crate::rooms::{CurrentRoom, Screen, request};
use dummies::Placing;
use hitboxes::ShowHitboxes;

pub struct PracticePlugin;

impl Plugin for PracticePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((dummies::DummiesPlugin, hitboxes::HitboxesPlugin));
        app.add_systems(
            Update,
            (show_tools, fold_tools, use_switches, switch_map).chain().run_if(in_state(Screen::InGame)),
        );
        app.add_systems(Update, tools_off.run_if(not(in_state(Screen::InGame)).or_else(not(practicing))));
        app.add_systems(OnExit(Screen::InGame), close_tools);
    }
}

fn practicing(room: Res<CurrentRoom>) -> bool {
    room.0.as_ref().is_some_and(|r| r.practice)
}

/// The practice tools' panel, while practicing.
#[derive(Component)]
struct Tools;

/// A practice tool switched on and off from the panel.
#[derive(Component, Clone, Copy, PartialEq)]
enum Switch {
    NoCooldowns,
    Hitboxes,
    FreeCamera,
}

impl Switch {
    const ALL: [Switch; 3] = [Switch::NoCooldowns, Switch::Hitboxes, Switch::FreeCamera];

    fn label(self) -> &'static str {
        match self {
            Switch::NoCooldowns => "No cooldowns",
            Switch::Hitboxes => "Hitboxes",
            Switch::FreeCamera => "Free camera",
        }
    }
}

/// A switch's ON / OFF text.
#[derive(Component)]
struct SwitchState(Switch);

/// What the switches switch.
#[derive(SystemParam)]
struct Switched<'w, 's> {
    me: Query<'w, 's, &'static AbilityState, With<Me>>,
    hitboxes: ResMut<'w, ShowHitboxes>,
    camera: ResMut<'w, CameraMode>,
    sender: Single<'w, 's, &'static mut MessageSender<RoomRequest>, With<Client>>,
}

impl Switched<'_, '_> {
    fn on(&self, switch: Switch) -> bool {
        match switch {
            Switch::NoCooldowns => self.me.iter().next().is_some_and(|ability| ability.free),
            Switch::Hitboxes => self.hitboxes.0,
            Switch::FreeCamera => self.camera.free,
        }
    }

    /// Switches it the other way round. No cooldowns is the server's to switch: asked for, it
    /// shows once it has.
    fn flip(&mut self, switch: Switch) {
        let on = self.on(switch);
        match switch {
            Switch::NoCooldowns => request(&mut self.sender, RoomRequest::NoCooldowns(!on)),
            Switch::Hitboxes => self.hitboxes.0 = !on,
            Switch::FreeCamera => self.camera.free = !on,
        }
    }
}

/// Switches practice to the next map: everything starts over on it.
#[derive(Component)]
struct MapButton;

/// The map practice is on, shown on its button.
#[derive(Component)]
struct MapName;

/// "Practice", over the tools: clicking it folds them away or out (`render::fold_header`).
#[derive(Component)]
struct ToolsHeader {
    chevron: Entity,
    tools: Entity,
    open: bool,
}

/// A row of the panel: a control with its label on the left and `value` on the right.
fn row(tools: &mut ChildSpawnerCommands, control: impl Bundle, label: &str, value: impl Bundle) {
    let node = Node {
        padding: UiRect::axes(px(13.0 * 1.2), px(13.0 * 0.6)),
        border: UiRect::all(px(1.0)),
        justify_content: JustifyContent::SpaceBetween,
        column_gap: px(12.0),
        ..default()
    };
    tools.spawn((control, HudButton, button_fill(palette::ui::PANEL), node, BorderColor::all(palette::ui::MUTED.with_alpha(0.5)))).with_children(
        |row| {
            row.spawn(ui_text(label, 13.0, palette::ui::TEXT));
            row.spawn(value);
        },
    );
}

/// Shows the panel while we're practicing, and not otherwise.
fn show_tools(mut commands: Commands, room: Res<CurrentRoom>, tools: Query<Entity, With<Tools>>) {
    let practicing = room.0.as_ref().is_some_and(|r| r.practice);
    match (practicing, tools.single().ok()) {
        (true, None) => {
            let panel = commands.spawn((Tools, corner_panel(true, 0.0))).id();
            let chevron = commands.spawn(chevron(true)).id();
            let label = commands.spawn(ui_text("Practice", 12.0, palette::ui::MUTED)).id();
            let tools = commands
                .spawn(tools_node(true))
                .with_children(|tools| {
                    row(tools, MapButton, "Map", (MapName, ui_text("", 13.0, palette::ui::ACCENT)));
                    for switch in Switch::ALL {
                        // As wide for "ON" as for "OFF", so the panel keeps its width.
                        let state = (
                            Node { width: px(32.0), justify_content: JustifyContent::FlexEnd, ..default() },
                            children![(SwitchState(switch), ui_text("OFF", 13.0, palette::ui::MUTED))],
                        );
                        row(tools, switch, switch.label(), state);
                    }
                    dummies::spawn_buttons(tools);
                })
                .id();
            let header = commands.spawn((ToolsHeader { chevron, tools, open: true }, fold_header())).add_children(&[chevron, label]).id();
            commands.entity(panel).add_children(&[header, tools]);
        }
        (false, Some(tools)) => commands.entity(tools).despawn(),
        _ => {}
    }
}

fn close_tools(mut commands: Commands, tools: Query<Entity, With<Tools>>) {
    for tools in &tools {
        commands.entity(tools).despawn();
    }
}

/// Outside practice, every tool is off: none of them carries over into a real fight.
fn tools_off(mut hitboxes: ResMut<ShowHitboxes>, mut camera: ResMut<CameraMode>, mut placing: ResMut<Placing>) {
    hitboxes.set_if_neq(ShowHitboxes(false));
    if camera.free {
        camera.free = false;
    }
    placing.set_if_neq(Placing::default());
}

/// The tools under the header: folded away, they keep their width (so the panel stays the same
/// size) but take no height.
fn tools_node(open: bool) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Stretch,
        row_gap: px(6.0),
        margin: UiRect::top(px(if open { 8.0 } else { 0.0 })),
        height: if open { Val::Auto } else { px(0.0) },
        overflow: Overflow::clip(),
        ..default()
    }
}

/// A click on "Practice" folds the tools away or out.
/// Folded, the tools are hidden too, so they can't be clicked where they'd be.
fn fold_tools(
    mut headers: Query<(Ref<Interaction>, &mut ToolsHeader)>,
    mut tools: Query<(&mut Node, &mut Visibility)>,
    mut chevrons: Query<&mut UiTransform>,
) {
    for (interaction, mut header) in &mut headers {
        if !clicked(interaction) {
            continue;
        }
        header.open = !header.open;
        if let Ok((mut node, mut visibility)) = tools.get_mut(header.tools) {
            *node = tools_node(header.open);
            *visibility = shown(header.open);
        }
        if let Ok(mut chevron) = chevrons.get_mut(header.chevron) {
            *chevron = chevron_turn(header.open);
        }
    }
}

/// A click on a switch flips it; each shows whether it's on.
fn use_switches(
    mut switched: Switched,
    mut buttons: Query<(Ref<Interaction>, &Switch, &mut BorderColor)>,
    mut states: Query<(&SwitchState, &mut Text, &mut TextColor)>,
) {
    for (interaction, switch, mut border) in &mut buttons {
        if clicked(interaction) {
            switched.flip(*switch);
        }
        let edge = if switched.on(*switch) { palette::ui::ACCENT } else { palette::ui::MUTED.with_alpha(0.5) };
        border.set_if_neq(BorderColor::all(edge));
    }
    for (state, mut text, mut color) in &mut states {
        let (label, tint) = if switched.on(state.0) { ("ON", palette::ui::ACCENT) } else { ("OFF", palette::ui::MUTED) };
        if text.0 != label {
            text.0 = label.to_string();
        }
        color.set_if_neq(TextColor(tint));
    }
}

/// The map button asks the server for the next map (dummies placed go with the old one, so
/// placing stops), and shows the one we're on.
fn switch_map(
    room: Res<CurrentRoom>,
    button: Single<Ref<Interaction>, With<MapButton>>,
    mut name: Single<&mut Text, With<MapName>>,
    mut placing: ResMut<Placing>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    let Some(map) = room.0.as_ref().map(|r| r.map) else { return };
    if clicked(*button) {
        request(&mut sender, RoomRequest::SetMap(map.next()));
        placing.set_if_neq(Placing::default());
    }
    if name.0 != map.label() {
        name.0 = map.label().to_string();
    }
}
