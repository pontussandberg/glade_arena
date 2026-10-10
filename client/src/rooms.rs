//! Guests and rooms, client side, without rendering (bots and tests use it too): says `Hello`,
//! keeps what the server tells us about rooms, and moves between the screens:
//!
//! - `Connecting` until the server welcomes us
//! - `Home`: picking a fighter, then practice (a room of our own) or a lobby to create or join
//! - `Room`: in a room's lobby, picking a team and a class (and, as its leader, starting it)
//! - `InGame`: our fighter is in the room's arena; when it's taken out (back to the lobby), back
//!   to `Room`, or `Home` from practice or once out of the room
//!
//! With `QuickJoin` set (bots, tests, a class on the command line), it joins that room as soon as
//! it's welcomed, and the class it was given takes it straight into the arena.

use arena_shared::protocol::*;
use arena_shared::rooms::*;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::InputMarker;
use lightyear::prelude::*;

use crate::ChosenClass;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    #[default]
    Connecting,
    Home,
    Room,
    InGame,
}

/// Picking a fighter, at home or in a room's lobby: the character select is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Picking;

impl ComputedStates for Picking {
    type SourceStates = Screen;

    fn compute(screen: Screen) -> Option<Self> {
        matches!(screen, Screen::Home | Screen::Room).then_some(Picking)
    }
}

/// Who the server says we are.
#[derive(Resource, Clone, Debug)]
pub struct Me {
    pub guest_id: u32,
    pub name: String,
}

/// The rooms there are, as the server last listed them.
#[derive(Resource, Default, Debug)]
pub struct RoomList(pub Vec<RoomSummary>);

/// The room we're in, if any.
#[derive(Resource, Default, Debug)]
pub struct CurrentRoom(pub Option<RoomView>);

/// Something to tell the player: why a request was refused, or why we were taken out of a room.
#[derive(Resource, Default, Debug)]
pub struct Notice(pub Option<String>);

/// The room to join (or make and start) as soon as we're welcomed, skipping the browser.
#[derive(Resource, Default, Clone, Debug)]
pub struct QuickJoin(pub Option<String>);

/// We asked to leave our room: once our fighter's gone, home rather than the room's lobby.
#[derive(Resource, Default, Debug)]
pub struct Leaving(pub bool);

/// The guest name we had last time, to ask for again.
#[derive(Resource, Default, Clone, Debug)]
struct StoredName(Option<String>);

pub struct RoomsNetPlugin {
    pub quick_join: Option<String>,
    pub guest_name: Option<String>,
}

impl Plugin for RoomsNetPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Screen>().add_computed_state::<Picking>();
        app.init_resource::<RoomList>().init_resource::<CurrentRoom>().init_resource::<Notice>().init_resource::<Leaving>();
        app.insert_resource(QuickJoin(self.quick_join.clone()));
        app.insert_resource(StoredName(self.guest_name.clone()));
        app.add_systems(Update, (say_hello, read_lobby_events, send_class_choice, enter_game).chain());
    }
}

/// Asks the server for something about rooms.
pub fn request(sender: &mut MessageSender<RoomRequest>, request: RoomRequest) {
    sender.send::<Reliable>(request);
}

/// Out of our room, home.
pub fn leave_room(sender: &mut MessageSender<RoomRequest>, leaving: &mut Leaving) {
    leaving.0 = true;
    request(sender, RoomRequest::Leave);
}

/// Once connected (again after a reconnect): who we were last time.
fn say_hello(stored: Res<StoredName>, client: Single<(&mut MessageSender<Hello>, Ref<Connected>), With<Client>>) {
    let (mut sender, connected) = client.into_inner();
    if connected.is_added() {
        sender.send::<Reliable>(Hello { guest_name: stored.0.clone() });
    }
}

#[allow(clippy::too_many_arguments)]
fn read_lobby_events(
    mut commands: Commands,
    mut client: Query<(&mut MessageReceiver<LobbyEvent>, &mut MessageSender<RoomRequest>), With<Client>>,
    quick_join: Res<QuickJoin>,
    mut list: ResMut<RoomList>,
    mut room: ResMut<CurrentRoom>,
    mut notice: ResMut<Notice>,
    mut leaving: ResMut<Leaving>,
    screen: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
) {
    let Ok((mut receiver, mut sender)) = client.single_mut() else { return };
    for event in receiver.receive() {
        match event {
            LobbyEvent::Welcome { guest_id, name } => {
                info!(guest_id, name, "welcome");
                save_guest_name(&name);
                commands.insert_resource(Me { guest_id, name });
                if let Some(name) = &quick_join.0 {
                    request(&mut sender, RoomRequest::QuickJoin(name.clone()));
                }
                if *screen.get() == Screen::Connecting {
                    next.set(Screen::Home);
                }
            }
            LobbyEvent::RoomList(rooms) => list.0 = rooms,
            LobbyEvent::Room(view) => {
                // Practice goes straight in: home until our fighter's there.
                if room.0.is_none() {
                    notice.0 = None;
                    if !view.practice {
                        next.set(Screen::Room);
                    }
                }
                room.0 = Some(view);
            }
            LobbyEvent::Left(reason) => {
                room.0 = None;
                leaving.0 = false;
                notice.0 = match reason {
                    LeaveReason::Left => None,
                    LeaveReason::Afk => Some("You were idle too long and left the room".into()),
                };
                if *screen.get() != Screen::InGame {
                    next.set(Screen::Home);
                }
            }
            LobbyEvent::Refused(why) => notice.0 = Some(why),
        }
    }
}

/// Our class, to the server: when it's picked, and when we join a room with one already picked.
fn send_class_choice(
    chosen: Res<ChosenClass>,
    room: Res<CurrentRoom>,
    mut joined: Local<Option<RoomKey>>,
    mut sender: Single<&mut MessageSender<ChooseClass>, With<Client>>,
) {
    let key = room.0.as_ref().map(|r| r.key);
    let just_joined = key.is_some() && *joined != key;
    *joined = key;
    if key.is_none() {
        return;
    }
    if let Some(class) = chosen.0
        && (chosen.is_changed() || just_joined)
    {
        sender.send::<Reliable>(ChooseClass(class));
    }
}

/// Into the arena once our fighter is there; out when it's gone: to the room's lobby, or home
/// from practice or once we've left the room.
fn enter_game(
    screen: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    room: Res<CurrentRoom>,
    leaving: Res<Leaving>,
    me: Query<(), (With<PlayerId>, With<InputMarker<PlayerInput>>)>,
) {
    match (screen.get(), me.is_empty()) {
        (Screen::Home | Screen::Room, false) => next.set(Screen::InGame),
        (Screen::InGame, true) => {
            let in_lobby = room.0.as_ref().is_some_and(|r| !r.practice) && !leaving.0;
            next.set(if in_lobby { Screen::Room } else { Screen::Home });
        }
        _ => {}
    }
}

/// The browser keeps our guest name (`localStorage`), to be called the same after a reload.
const GUEST_NAME_KEY: &str = "arena.guest";

pub fn load_guest_name() -> Option<String> {
    load_setting(GUEST_NAME_KEY)
}

fn save_guest_name(name: &str) {
    save_setting(GUEST_NAME_KEY, name);
}

/// Something the browser keeps for us (`localStorage`) across reloads. Nothing outside a browser.
pub fn load_setting(key: &str) -> Option<String> {
    #[cfg(target_family = "wasm")]
    {
        let storage = web_sys::window()?.local_storage().ok()??;
        storage.get_item(key).ok()?
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = key;
        None
    }
}

pub fn save_setting(key: &str, value: &str) {
    #[cfg(target_family = "wasm")]
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(key, value);
    }
    #[cfg(not(target_family = "wasm"))]
    let _ = (key, value);
}
