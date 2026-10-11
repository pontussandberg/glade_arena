//! The in-game menu, opened and closed with ESC: back to the fight; in a lobby, back to it (to
//! pick another fighter, still in the room) or out of it altogether; in practice, home. The room
//! goes on without us. While it's open, the fight's controls and the camera's are off.

use arena_shared::protocol::RoomRequest;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::PlayerControls;
use crate::camera::CameraControl;
use crate::arena::palette;
use crate::render::{button, clicked, ui_text};
use crate::rooms::{CurrentRoom, Leaving, Screen, leave_room, request};
use crate::stat_frame::ACCENT;

pub struct EscMenuPlugin;

impl Plugin for EscMenuPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, (PlayerControls, CameraControl).run_if(not(menu_open)));
        app.add_systems(Update, (toggle_menu, menu_buttons).chain().run_if(in_state(Screen::InGame)));
        app.add_systems(OnExit(Screen::InGame), close_menu);
    }
}

/// The menu, while it's open.
#[derive(Component)]
struct EscMenu;

#[derive(Component)]
struct ResumeButton;

/// Back to the lobby's character select, still in the room.
#[derive(Component)]
struct LobbyButton;

/// Out of the room, home.
#[derive(Component)]
struct LeaveButton;

fn menu_open(menu: Query<(), With<EscMenu>>) -> bool {
    !menu.is_empty()
}

fn toggle_menu(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    menu: Query<Entity, With<EscMenu>>,
    room: Res<CurrentRoom>,
    mut desired: ResMut<crate::DesiredInput>,
    mut aiming: ResMut<crate::casting::Aiming>,
    mut placing: ResMut<crate::dummies::Placing>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    // ESC first cancels placing a target dummy (`dummies.rs`).
    if placing.on {
        placing.on = false;
        return;
    }
    // ESC first drops a normal cast's aim; the next one opens the menu.
    if aiming.0 {
        aiming.0 = false;
        return;
    }
    if let Ok(menu) = menu.single() {
        commands.entity(menu).despawn();
        return;
    }
    // Stand still and stop attacking while away from the controls.
    desired.0 = default();
    let practice = room.0.as_ref().is_none_or(|r| r.practice);
    let title = match &room.0 {
        Some(r) if !r.practice => format!("{}  ·  {}", r.name, r.mode.label()),
        _ => "Practice".to_string(),
    };
    commands
        .spawn((
            EscMenu,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(12.0),
                ..default()
            },
            BackgroundColor(palette::ui::SHADOW.with_alpha(0.7)),
            GlobalZIndex(30),
        ))
        .with_children(|menu| {
            menu.spawn(ui_text("MENU", 12.0, ACCENT));
            menu.spawn(ui_text(title, 20.0, palette::ui::LICHEN));
            menu.spawn(Node { height: px(12.0), ..default() });
            menu.spawn((ResumeButton, button("Back to the fight", 16.0, ACCENT, palette::ui::HOLLOW, ACCENT)));
            let quiet = |label: &'static str| button(label, 16.0, palette::ui::HOLLOW, palette::ui::LICHEN, palette::ui::MUTED.with_alpha(0.5));
            if practice {
                menu.spawn((LeaveButton, quiet("Leave practice")));
            } else {
                menu.spawn((LobbyButton, quiet("Back to the lobby")));
                menu.spawn((LeaveButton, quiet("Leave the lobby")));
            }
            menu.spawn(ui_text("ESC to close", 11.0, palette::ui::MUTED));
        });
}

#[allow(clippy::too_many_arguments)]
fn menu_buttons(
    mut commands: Commands,
    resume: Query<Ref<Interaction>, With<ResumeButton>>,
    lobby: Query<Ref<Interaction>, With<LobbyButton>>,
    leave: Query<Ref<Interaction>, With<LeaveButton>>,
    mut leaving: ResMut<Leaving>,
    menu: Query<Entity, With<EscMenu>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if resume.iter().any(clicked) {
        // Or the click would be taken for an attack.
        mouse.reset(MouseButton::Left);
        for menu in &menu {
            commands.entity(menu).despawn();
        }
    }
    if lobby.iter().any(clicked) {
        request(&mut sender, RoomRequest::LeaveArena);
    }
    if leave.iter().any(clicked) {
        leave_room(&mut sender, &mut leaving);
    }
}

fn close_menu(mut commands: Commands, menu: Query<Entity, With<EscMenu>>) {
    for menu in &menu {
        commands.entity(menu).despawn();
    }
}
