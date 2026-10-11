//! The practice tools, LoL-style: a panel in the top right corner while practicing, under a
//! "Practice" header that folds them away or out (like "Controls", open to begin with). Its one
//! tool so far, "Target dummy", places one: the next left click stands it where it's clicked
//! (the server takes each class in turn); a right click or ESC (`esc_menu`) cancels. Click it
//! again for another. A hint says so meanwhile. The placing click itself is read in
//! `render::read_local_input`, so it never also attacks.

use arena_shared::protocol::RoomRequest;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::arena::palette;
use crate::render::{GameUi, HudButton, button, clicked, ui_text};
use crate::rooms::{CurrentRoom, Screen, request};

pub struct DummiesPlugin;

impl Plugin for DummiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Placing>();
        app.add_systems(Update, (show_tools, fold_tools, use_tools, place_dummy, show_hint).chain().run_if(in_state(Screen::InGame)));
        app.add_systems(OnExit(Screen::InGame), close_tools);
    }
}

/// Whether we're placing a target dummy, and where the click (not yet sent) asked for it.
#[derive(Resource, Default)]
pub(crate) struct Placing {
    pub(crate) on: bool,
    pub(crate) at: Option<Vec2>,
}

/// The practice tools' panel, while practicing.
#[derive(Component)]
struct Tools;

#[derive(Component)]
struct DummyButton;

/// "Practice", over the tools: clicking it folds them away or out. Its chevron (a corner of a
/// square, turned) points down while open and right while folded.
#[derive(Component)]
struct ToolsHeader {
    chevron: Entity,
    tools: Entity,
    open: bool,
}

/// Shows the panel while we're practicing, and not otherwise.
fn show_tools(mut commands: Commands, room: Res<CurrentRoom>, tools: Query<Entity, With<Tools>>) {
    let practicing = room.0.as_ref().is_some_and(|r| r.practice);
    match (practicing, tools.single().ok()) {
        (true, None) => {
            let panel = commands
                .spawn((
                    Tools,
                    GameUi,
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(8.0),
                        right: px(8.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::FlexStart,
                        row_gap: px(8.0),
                        padding: UiRect::axes(px(10.0), px(8.0)),
                        ..default()
                    },
                    BackgroundColor(palette::ui::HOLLOW.with_alpha(0.6)),
                ))
                .id();
            let chevron = commands
                .spawn((
                    Node { width: px(5.0), height: px(5.0), border: UiRect { right: px(1.5), bottom: px(1.5), ..default() }, ..default() },
                    BorderColor::all(palette::ui::MUTED),
                    UiTransform::from_rotation(Rot2::degrees(45.0)),
                ))
                .id();
            let label = commands.spawn(ui_text("Practice", 12.0, palette::ui::MUTED)).id();
            let tools = commands
                .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Stretch, row_gap: px(6.0), ..default() })
                .with_children(|tools| {
                    tools.spawn((DummyButton, HudButton, button("Target dummy", 13.0, palette::ui::HOLLOW, palette::ui::LICHEN, palette::ui::MUTED.with_alpha(0.5))));
                })
                .id();
            let header = commands
                .spawn((
                    ToolsHeader { chevron, tools, open: true },
                    HudButton,
                    Interaction::default(),
                    Node { column_gap: px(8.0), align_items: AlignItems::Center, ..default() },
                ))
                .add_children(&[chevron, label])
                .id();
            commands.entity(panel).add_children(&[header, tools]);
        }
        (false, Some(tools)) => commands.entity(tools).despawn(),
        _ => {}
    }
}

fn close_tools(mut commands: Commands, tools: Query<Entity, With<Tools>>, mut placing: ResMut<Placing>) {
    for tools in &tools {
        commands.entity(tools).despawn();
    }
    *placing = Placing::default();
}

/// A click on "Practice" folds the tools away or out.
fn fold_tools(mut headers: Query<(Ref<Interaction>, &mut ToolsHeader)>, mut nodes: Query<&mut Node>, mut chevrons: Query<&mut UiTransform>) {
    for (interaction, mut header) in &mut headers {
        if !clicked(interaction) {
            continue;
        }
        header.open = !header.open;
        if let Ok(mut tools) = nodes.get_mut(header.tools) {
            tools.display = if header.open { Display::Flex } else { Display::None };
        }
        if let Ok(mut chevron) = chevrons.get_mut(header.chevron) {
            *chevron = UiTransform::from_rotation(Rot2::degrees(if header.open { 45.0 } else { -45.0 }));
        }
    }
}

/// The dummy button starts placing one (or, while placing, cancels it).
fn use_tools(buttons: Query<Ref<Interaction>, With<DummyButton>>, mut placing: ResMut<Placing>) {
    if buttons.iter().any(clicked) {
        placing.on = !placing.on;
    }
}

/// Says where to stand the clicked dummy.
fn place_dummy(mut placing: ResMut<Placing>, mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>) {
    if let Some(at) = placing.at.take() {
        request(&mut sender, RoomRequest::PlaceDummy(at));
    }
}

/// The hint at the top of the screen while placing.
#[derive(Component)]
struct Hint;

fn show_hint(mut commands: Commands, placing: Res<Placing>, hint: Query<Entity, With<Hint>>) {
    if !placing.is_changed() {
        return;
    }
    match (placing.on, hint.single().ok()) {
        (true, None) => {
            commands.spawn((
                Hint,
                GameUi,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(56.0),
                    width: percent(100.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Pickable::IGNORE,
                children![ui_text("Click to place a target dummy  ·  right-click or ESC to cancel", 14.0, palette::ui::SPROUT)],
            ));
        }
        (false, Some(hint)) => commands.entity(hint).despawn(),
        _ => {}
    }
}
