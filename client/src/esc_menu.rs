//! The in-game menu, opened and closed with ESC (or opened with the HUD's Home button): back to
//! the fight; in a lobby, back to it (to pick another fighter, still in the room) or out of it
//! altogether; in practice, home; and the settings (`settings`), in a pane under the buttons.
//! The room goes on without us. While it's open, the fight's controls and the camera's are off.
//! The menu is a frame (`frames::Frame::Menu`): the frames say when it's open, and this shows it.

use arena_shared::protocol::RoomRequest;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::PlayerControls;
use crate::camera::CameraControl;
use crate::arena::palette;
use crate::frames::{Frame, FrameKeys, OpenFrame, close_frames, route_keys};
use crate::render::{button, clicked, ui_text};
use crate::rooms::{CurrentRoom, Leaving, Screen, leave_room, request};
use crate::arena::palette::ui::ACCENT;

pub struct EscMenuPlugin;

impl Plugin for EscMenuPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, (PlayerControls, CameraControl).run_if(not(menu_open)));
        app.add_systems(Update, (home_button, menu_buttons, show_menu).chain().in_set(FrameKeys).after(route_keys).run_if(in_state(Screen::InGame)));
        app.add_systems(OnExit(Screen::InGame), show_menu.after(close_frames));
    }
}

/// The menu on screen, while it's open.
#[derive(Component)]
struct EscMenu;

/// The HUD's button that opens the menu (`render/` puts it in the top-left panel).
#[derive(Component)]
pub(crate) struct HomeButton;

#[derive(Component)]
struct ResumeButton;

/// Back to the lobby's character select, still in the room.
#[derive(Component)]
struct LobbyButton;

/// Out of the room, home.
#[derive(Component)]
struct LeaveButton;

/// Opens (and closes) the settings pane under the buttons.
#[derive(Component)]
struct SettingsButton;

/// The settings pane, while it's open.
#[derive(Component)]
struct SettingsPane;

fn menu_open(open: Res<OpenFrame>) -> bool {
    open.is(Frame::Menu)
}

/// The Home button opens the menu, in place of whatever frame is open.
fn home_button(home: Query<Ref<Interaction>, With<HomeButton>>, mut open: ResMut<OpenFrame>) {
    if home.iter().any(clicked) {
        open.open(Frame::Menu);
    }
}

/// The menu is on screen while its frame is open. Coming up, it stops us: stand still and stop
/// attacking while away from the controls.
fn show_menu(
    mut commands: Commands,
    open: Res<OpenFrame>,
    menu: Query<Entity, With<EscMenu>>,
    room: Res<CurrentRoom>,
    mut desired: ResMut<crate::DesiredInput>,
) {
    match menu.single() {
        Ok(menu) if !open.is(Frame::Menu) => {
            commands.entity(menu).despawn();
            return;
        }
        Err(_) if open.is(Frame::Menu) => {}
        _ => return,
    }
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
            BackgroundColor(palette::ui::BACKDROP.with_alpha(0.7)),
            GlobalZIndex(30),
        ))
        .with_children(|menu| {
            menu.spawn(ui_text("MENU", 12.0, ACCENT));
            menu.spawn(ui_text(title, 20.0, palette::ui::TEXT));
            menu.spawn(Node { height: px(12.0), ..default() });
            menu.spawn((ResumeButton, button("Back to the fight", 16.0, ACCENT, palette::ui::PANEL, ACCENT)));
            let quiet = |label: &'static str| button(label, 16.0, palette::ui::PANEL, palette::ui::TEXT, palette::ui::MUTED.with_alpha(0.5));
            if practice {
                menu.spawn((LeaveButton, quiet("Leave practice")));
            } else {
                menu.spawn((LobbyButton, quiet("Back to the lobby")));
                menu.spawn((LeaveButton, quiet("Leave the lobby")));
            }
            menu.spawn((SettingsButton, quiet("Settings")));
            menu.spawn(ui_text("ESC to close", 11.0, palette::ui::MUTED));
        });
}

#[allow(clippy::too_many_arguments)]
fn menu_buttons(
    mut commands: Commands,
    resume: Query<Ref<Interaction>, With<ResumeButton>>,
    lobby: Query<Ref<Interaction>, With<LobbyButton>>,
    leave: Query<Ref<Interaction>, With<LeaveButton>>,
    settings_button: Query<Ref<Interaction>, With<SettingsButton>>,
    pane: Query<Entity, With<SettingsPane>>,
    settings: Res<crate::settings::Settings>,
    mut leaving: ResMut<Leaving>,
    menu: Query<Entity, With<EscMenu>>,
    mut open: ResMut<OpenFrame>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if settings_button.iter().any(clicked) {
        match (pane.single(), menu.single()) {
            (Ok(pane), _) => commands.entity(pane).despawn(),
            (Err(_), Ok(menu)) => {
                commands.entity(menu).with_children(|menu| {
                    crate::settings::spawn_pane(menu, &settings).insert(SettingsPane);
                });
            }
            _ => {}
        }
    }
    if resume.iter().any(clicked) {
        // Or the click would be taken for an attack.
        mouse.reset(MouseButton::Left);
        open.close(Frame::Menu);
    }
    if lobby.iter().any(clicked) {
        request(&mut sender, RoomRequest::LeaveArena);
    }
    if leave.iter().any(clicked) {
        leave_room(&mut sender, &mut leaving);
    }
}
