//! Guests and rooms. A connected client says `Hello` and becomes a guest with a name; from the
//! server browser it creates or joins a room. Each room is its own arena in this one world: its
//! fighters, projectiles and pickups carry `InRoom` (gameplay only ever pairs things in the same
//! room) and lightyear `Rooms` (only the room's members are sent them).
//!
//! The room's leader (its creator, then the longest-standing member) picks free-for-all or red
//! vs blue and starts the match, which takes everyone in; members pick their team and class, and
//! once it's on go in and out of the arena as they like (back to the room's lobby to pick another
//! class), latecomers included. Leaving the room takes a member's fighter out of the arena; the
//! room is gone when its last member leaves. A member idle for `AfkTimeout` is taken out as if
//! they'd left.
//!
//! Practice is a room of one's own: never listed, no one else can join, started from the off.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use arena_shared::protocol::*;
use arena_shared::rooms::*;
use bevy::prelude::*;
use lightyear::prelude::input::native::ActionState;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use crate::{spawn_dummy, spawn_pickups, spawn_player};

/// Server-only: the room a fighter, projectile or pickup is in.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InRoom {
    pub key: RoomKey,
    /// The lightyear room its members are in, which decides who's sent it.
    pub net: RoomId,
}

impl InRoom {
    /// What makes it replicate to this room's members only.
    pub fn rooms(self) -> Rooms {
        Rooms::single(self.net)
    }
}

/// How long a member may stay idle in a room (`AFK_TIMEOUT` unless a test sets it).
#[derive(Resource, Clone, Copy, Debug)]
pub struct AfkTimeout(pub Duration);

/// How often idle members are looked for.
const AFK_CHECK_EVERY: Duration = Duration::from_secs(5);
/// The server browser's list is sent at most this often.
const LIST_EVERY: Duration = Duration::from_millis(500);

/// The systems that handle guests' messages; `place_players` goes after them, so a fighter
/// spawned this frame is placed before its first replication.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RoomSystems;

pub struct RoomsPlugin;

impl Plugin for RoomsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Lobby>();
        if !app.world().contains_resource::<AfkTimeout>() {
            app.insert_resource(AfkTimeout(AFK_TIMEOUT));
        }
        app.add_observer(forget_disconnected);
        app.add_systems(
            Update,
            // A class picked with a request (practice, the way in) is known when it's handled.
            (greet, handle_class_choices, handle_requests, take_out_idle, send_updates).chain().in_set(RoomSystems),
        );
        app.add_systems(FixedUpdate, note_activity);
    }
}

/// Every guest and room.
#[derive(Resource, Default)]
pub struct Lobby {
    /// By their client's link.
    guests: HashMap<Entity, Guest>,
    rooms: BTreeMap<RoomKey, Room>,
    next_guest: u32,
    next_room: u32,
    /// The last target dummy's number (its `PeerId::Local`).
    next_dummy: u64,
    /// Lightyear rooms no longer used, to use again (there are only `u16::MAX` of them).
    free_net: Vec<RoomId>,
    /// The browser's list changed since it was last sent.
    list_changed: bool,
}

struct Guest {
    id: u32,
    name: String,
    peer: PeerId,
    room: Option<RoomKey>,
    team: u8,
    class: Option<ClassId>,
    /// Wants to be in the arena: their fighter is there, or will be once the match is on and
    /// they've picked a class.
    playing: bool,
    /// When they last did anything (`Time<Real>`), for the AFK timeout.
    active_at: Duration,
    /// Their latest input, to notice when it changes.
    last_input: PlayerInput,
}

struct Room {
    name: String,
    mode: Mode,
    net: RoomId,
    /// A link in `members`.
    leader: Entity,
    started: bool,
    /// Never listed, no one joins.
    practice: bool,
    /// Links, in the order they joined.
    members: Vec<Entity>,
    pickups: Vec<Entity>,
    /// Target dummies, in the order they were placed (practice only).
    dummies: Vec<Entity>,
    /// Members need to see it again.
    changed: bool,
}

impl Lobby {
    /// Who's in `room`, and how many on each team.
    fn on_team(&self, room: &Room, team: u8) -> usize {
        room.members.iter().filter(|link| self.guests.get(link).is_some_and(|g| g.team == team)).count()
    }

    /// The team with fewer members (red when even).
    fn smaller_team(&self, room: &Room) -> u8 {
        if self.on_team(room, BLUE) < self.on_team(room, RED) { BLUE } else { RED }
    }

    fn summary(&self, key: RoomKey, room: &Room) -> RoomSummary {
        RoomSummary { key, name: room.name.clone(), mode: room.mode, players: room.members.len() as u8, started: room.started }
    }

    fn view(&self, key: RoomKey, room: &Room) -> RoomView {
        let members = room
            .members
            .iter()
            .filter_map(|link| self.guests.get(link))
            .map(|g| Member { guest_id: g.id, name: g.name.clone(), peer: g.peer, team: g.team, class: g.class, in_arena: g.playing && room.started })
            .collect();
        let leader = self.guests.get(&room.leader).map_or(0, |g| g.id);
        RoomView { key, name: room.name.clone(), mode: room.mode, started: room.started, practice: room.practice, leader, members }
    }

    fn list(&self) -> Vec<RoomSummary> {
        self.rooms.iter().filter(|(_, room)| !room.practice).map(|(key, room)| self.summary(*key, room)).collect()
    }
}

/// What handling a request needs besides the lobby.
#[derive(bevy::ecs::system::SystemParam)]
struct Ctx<'w, 's> {
    commands: Commands<'w, 's>,
    time: Res<'w, Time<Real>>,
    players: Query<'w, 's, (Entity, &'static ControlledBy), With<PlayerId>>,
    senders: Query<'w, 's, &'static mut MessageSender<LobbyEvent>>,
}

impl Ctx<'_, '_> {
    fn send(&mut self, link: Entity, event: LobbyEvent) {
        if let Ok(mut sender) = self.senders.get_mut(link) {
            sender.send::<Reliable>(event);
        }
    }

    fn player_of(&self, link: Entity) -> Option<Entity> {
        self.players.iter().find(|(_, c)| c.owner == link).map(|(player, _)| player)
    }
}

/// `Hello`: becomes a guest, called what they were last time if that's a guest's name no one
/// else has, or else a new one. Answered with who they are and the rooms there are.
fn greet(
    mut links: Query<(Entity, &RemoteId, &mut MessageReceiver<Hello>), With<ClientOf>>,
    mut lobby: ResMut<Lobby>,
    mut ctx: Ctx,
) {
    for (link, remote, mut receiver) in &mut links {
        for Hello { guest_name: stored } in receiver.receive() {
            if lobby.guests.contains_key(&link) {
                continue;
            }
            let taken = |name: &str| lobby.guests.values().any(|g| g.name == name);
            let name = match stored {
                Some(name) if is_guest_name(&name) && !taken(&name) => name,
                // A free pair, or once they're nearly all taken, one with a number.
                _ => (0..)
                    .map(|tries| {
                        let name = guest_name(fastrand::u32(..GUEST_NAMES));
                        if tries < 64 { name } else { format!("{name} {}", fastrand::u16(2..10_000)) }
                    })
                    .find(|name| !taken(name))
                    .expect("a free name"),
            };
            lobby.next_guest += 1;
            let id = lobby.next_guest;
            info!(client = ?remote.0, guest = id, name, "guest arrived");
            lobby.guests.insert(link, Guest {
                id,
                name: name.clone(),
                peer: remote.0,
                room: None,
                team: NO_TEAM,
                class: None,
                playing: false,
                active_at: ctx.time.elapsed(),
                last_input: PlayerInput::default(),
            });
            ctx.send(link, LobbyEvent::Welcome { guest_id: id, name });
            let list = lobby.list();
            ctx.send(link, LobbyEvent::RoomList(list));
        }
    }
}

/// Room requests from guests, in the order they came.
fn handle_requests(
    mut links: Query<(Entity, &mut MessageReceiver<RoomRequest>), With<ClientOf>>,
    mut lobby: ResMut<Lobby>,
    mut allocator: ResMut<RoomAllocator>,
    mut ctx: Ctx,
) {
    for (link, mut receiver) in &mut links {
        for request in receiver.receive() {
            let now = ctx.time.elapsed();
            let Some(guest) = lobby.guests.get_mut(&link) else { continue };
            guest.active_at = now;
            if let Err(why) = handle(&mut lobby, &mut allocator, &mut ctx, link, request) {
                ctx.send(link, LobbyEvent::Refused(why.into()));
            }
        }
    }
}

fn handle(lobby: &mut Lobby, allocator: &mut RoomAllocator, ctx: &mut Ctx, link: Entity, request: RoomRequest) -> Result<(), &'static str> {
    let room_key = lobby.guests[&link].room;
    match request {
        RoomRequest::Create { name, mode } => {
            let name = clean_room_name(&name).ok_or("Give the room a name")?;
            leave(lobby, ctx, link);
            let key = create(lobby, allocator, ctx, link, name, mode);
            join(lobby, ctx, link, key)?;
        }
        RoomRequest::Join(key) => {
            if room_key == Some(key) {
                return Ok(());
            }
            let room = lobby.rooms.get(&key).filter(|room| !room.practice).ok_or("That room is gone")?;
            if room.members.len() >= room.mode.capacity() {
                return Err("That room is full");
            }
            leave(lobby, ctx, link);
            join(lobby, ctx, link, key)?;
        }
        RoomRequest::QuickJoin(name) => {
            let name = clean_room_name(&name).ok_or("Give the room a name")?;
            let found = lobby
                .rooms
                .iter()
                .find(|(_, room)| !room.practice && room.name == name && room.members.len() < room.mode.capacity())
                .map(|(key, _)| *key);
            if room_key.is_some() && room_key == found {
                return Ok(());
            }
            leave(lobby, ctx, link);
            let key = match found {
                Some(key) => key,
                None => {
                    let key = create(lobby, allocator, ctx, link, name, Mode::Ffa);
                    lobby.rooms.get_mut(&key).expect("just made").started = true;
                    key
                }
            };
            lobby.guests.get_mut(&link).expect("a guest").playing = true;
            join(lobby, ctx, link, key)?;
        }
        RoomRequest::Practice => {
            leave(lobby, ctx, link);
            let key = create(lobby, allocator, ctx, link, "Practice".into(), Mode::Ffa);
            let room = lobby.rooms.get_mut(&key).expect("just made");
            room.started = true;
            room.practice = true;
            lobby.guests.get_mut(&link).expect("a guest").playing = true;
            join(lobby, ctx, link, key)?;
        }
        RoomRequest::EnterArena => {
            let key = room_key.ok_or("You're not in a room")?;
            if !lobby.rooms[&key].started {
                return Err("The match hasn't started");
            }
            // In now, or once the class arrives.
            lobby.guests.get_mut(&link).expect("checked").playing = true;
            lobby.rooms.get_mut(&key).expect("in it").changed = true;
            enter_arena(lobby, ctx, link);
        }
        RoomRequest::LeaveArena => {
            let key = room_key.ok_or("You're not in a room")?;
            lobby.guests.get_mut(&link).expect("checked").playing = false;
            lobby.rooms.get_mut(&key).expect("in it").changed = true;
            if let Some(player) = ctx.player_of(link) {
                info!(room = key.0, guest = lobby.guests[&link].id, "back to the lobby");
                ctx.commands.entity(player).try_despawn();
            }
        }
        RoomRequest::Leave => {
            if leave(lobby, ctx, link) {
                ctx.send(link, LobbyEvent::Left(LeaveReason::Left));
                ctx.send(link, LobbyEvent::RoomList(lobby.list()));
            }
        }
        RoomRequest::SetMode(mode) => {
            let key = room_key.ok_or("You're not in a room")?;
            let room = &lobby.rooms[&key];
            if room.leader != link {
                return Err("Only the leader can change the mode");
            }
            if room.started {
                return Err("The match has started");
            }
            if room.mode == mode {
                return Ok(());
            }
            if room.members.len() > mode.capacity() {
                return Err("Too many in the room for that");
            }
            // Everyone on a team, taking turns; or no teams.
            let members = room.members.clone();
            for (n, member) in members.iter().enumerate() {
                if let Some(guest) = lobby.guests.get_mut(member) {
                    guest.team = match mode {
                        Mode::Ffa => NO_TEAM,
                        Mode::Teams => TEAMS[n % 2],
                    };
                }
            }
            let room = lobby.rooms.get_mut(&key).expect("checked");
            room.mode = mode;
            room.changed = true;
            lobby.list_changed = true;
        }
        RoomRequest::SetTeam(team) => {
            let key = room_key.ok_or("You're not in a room")?;
            let room = &lobby.rooms[&key];
            if room.mode != Mode::Teams || !TEAMS.contains(&team) {
                return Err("There are no teams in this room");
            }
            if lobby.guests[&link].team == team {
                return Ok(());
            }
            if lobby.on_team(room, team) >= MAX_PER_TEAM {
                return Err("That team is full");
            }
            lobby.guests.get_mut(&link).expect("checked").team = team;
            lobby.rooms.get_mut(&key).expect("checked").changed = true;
            // Mid-match: switch sides at once.
            if let Some(player) = ctx.player_of(link) {
                ctx.commands.entity(player).insert(Team(team));
            }
        }
        RoomRequest::PlaceDummy(at) => {
            let key = room_key.ok_or("You're not in a room")?;
            let room = lobby.rooms.get_mut(&key).expect("in it");
            if !room.practice {
                return Err("Target dummies are for practice");
            }
            if room.dummies.len() >= MAX_DUMMIES {
                return Err("That's enough dummies");
            }
            if !at.is_finite() || !arena_shared::map::map().walkable_at(at) {
                return Err("A dummy can't stand there");
            }
            // Each class in turn.
            let class = ClassId::all().nth(room.dummies.len() % ClassId::all().count()).expect("a class");
            let in_room = InRoom { key, net: room.net };
            lobby.next_dummy += 1;
            let dummy = spawn_dummy(&mut ctx.commands, PeerId::Local(lobby.next_dummy), class, in_room, at);
            lobby.rooms.get_mut(&key).expect("in it").dummies.push(dummy);
            info!(room = key.0, class = class.def().name, "target dummy placed");
        }
        RoomRequest::Start => {
            let key = room_key.ok_or("You're not in a room")?;
            let room = lobby.rooms.get_mut(&key).expect("in it");
            if room.leader != link {
                return Err("Only the leader can start the match");
            }
            if room.started {
                return Ok(());
            }
            room.started = true;
            room.changed = true;
            lobby.list_changed = true;
            info!(room = key.0, name = room.name, mode = ?room.mode, players = room.members.len(), "match started");
            let members = room.members.clone();
            for member in members {
                if let Some(guest) = lobby.guests.get_mut(&member) {
                    guest.playing = true;
                }
                enter_arena(lobby, ctx, member);
            }
        }
    }
    Ok(())
}

/// The most target dummies a practice room may have.
const MAX_DUMMIES: usize = 12;

/// A new room, led by `leader` (who still has to `join` it).
fn create(lobby: &mut Lobby, allocator: &mut RoomAllocator, ctx: &mut Ctx, leader: Entity, name: String, mode: Mode) -> RoomKey {
    lobby.next_room += 1;
    let key = RoomKey(lobby.next_room);
    let net = lobby.free_net.pop().unwrap_or_else(|| allocator.allocate());
    let pickups = spawn_pickups(&mut ctx.commands, InRoom { key, net });
    info!(room = key.0, name, ?mode, leader = lobby.guests[&leader].id, "room created");
    lobby.rooms.insert(key, Room { name, mode, net, leader, started: false, practice: false, members: Vec::new(), pickups, dummies: Vec::new(), changed: true });
    key
}

/// Into room `key`, which has room for one more; on the smaller team if there are teams. In a
/// started match they enter the arena once they've picked a class.
fn join(lobby: &mut Lobby, ctx: &mut Ctx, link: Entity, key: RoomKey) -> Result<(), &'static str> {
    let room = lobby.rooms.get(&key).ok_or("That room is gone")?;
    let team = match room.mode {
        Mode::Ffa => NO_TEAM,
        Mode::Teams => lobby.smaller_team(room),
    };
    let net = room.net;
    let room = lobby.rooms.get_mut(&key).expect("checked");
    room.members.push(link);
    room.changed = true;
    lobby.list_changed = true;
    let guest = lobby.guests.get_mut(&link).expect("a guest");
    guest.room = Some(key);
    guest.team = team;
    info!(room = key.0, guest = guest.id, "joined a room");
    ctx.commands.entity(link).insert(Rooms::single(net));
    enter_arena(lobby, ctx, link);
    Ok(())
}

/// Spawns `link`'s fighter if their room has started, they want in, have picked a class and
/// aren't in yet.
fn enter_arena(lobby: &Lobby, ctx: &mut Ctx, link: Entity) {
    let Some(guest) = lobby.guests.get(&link) else { return };
    let (Some(key), Some(class)) = (guest.room, guest.class) else { return };
    if !guest.playing {
        return;
    }
    let room = &lobby.rooms[&key];
    if !room.started || ctx.player_of(link).is_some() {
        return;
    }
    info!(room = key.0, guest = guest.id, class = class.def().name, "entered the arena");
    let in_room = InRoom { key, net: room.net };
    spawn_player(&mut ctx.commands, link, guest.peer, class, in_room, Team(guest.team));
}

/// Out of their room, if they're in one: their fighter goes, and so does the room if it's
/// empty now; if they led it, the next member leads. Whether they were in one.
fn leave(lobby: &mut Lobby, ctx: &mut Ctx, link: Entity) -> bool {
    let Some(guest) = lobby.guests.get_mut(&link) else { return false };
    let Some(key) = guest.room.take() else { return false };
    guest.team = NO_TEAM;
    guest.playing = false;
    let id = guest.id;
    if let Some(player) = ctx.player_of(link) {
        ctx.commands.entity(player).try_despawn();
    }
    ctx.commands.entity(link).try_remove::<Rooms>();
    lobby.list_changed = true;
    let Some(room) = lobby.rooms.get_mut(&key) else { return true };
    room.members.retain(|m| *m != link);
    room.changed = true;
    info!(room = key.0, guest = id, "left a room");
    match room.members.first() {
        None => {
            let room = lobby.rooms.remove(&key).expect("there");
            for thing in room.pickups.into_iter().chain(room.dummies) {
                ctx.commands.entity(thing).try_despawn();
            }
            lobby.free_net.push(room.net);
            info!(room = key.0, "room closed");
        }
        Some(next) if room.leader == link => room.leader = *next,
        Some(_) => {}
    }
    true
}

/// `ChooseClass`: the class they'll enter the arena as (now, if the match is on and they want
/// in: a class given straight away, as when quick-joining or practicing).
fn handle_class_choices(
    mut links: Query<(Entity, &mut MessageReceiver<ChooseClass>), With<ClientOf>>,
    mut lobby: ResMut<Lobby>,
    mut ctx: Ctx,
) {
    for (link, mut receiver) in &mut links {
        for ChooseClass(class) in receiver.receive() {
            let now = ctx.time.elapsed();
            let Some(class) = class.checked() else { continue };
            let Some(guest) = lobby.guests.get_mut(&link) else { continue };
            guest.active_at = now;
            guest.class = Some(class);
            if let Some(key) = guest.room
                && let Some(room) = lobby.rooms.get_mut(&key)
            {
                room.changed = true;
            }
            enter_arena(&lobby, &mut ctx, link);
        }
    }
}

/// A fighter whose input changes isn't idle.
fn note_activity(
    time: Res<Time<Real>>,
    mut lobby: ResMut<Lobby>,
    players: Query<(&ControlledBy, &ActionState<PlayerInput>), With<PlayerId>>,
) {
    for (controlled_by, input) in &players {
        if let Some(guest) = lobby.guests.get_mut(&controlled_by.owner)
            && guest.last_input != input.0
        {
            guest.last_input = input.0;
            guest.active_at = time.elapsed();
        }
    }
}

/// Members idle for `AfkTimeout` are taken out of their room.
fn take_out_idle(mut lobby: ResMut<Lobby>, timeout: Res<AfkTimeout>, mut next: Local<Duration>, mut ctx: Ctx) {
    let now = ctx.time.elapsed();
    if now < *next {
        return;
    }
    *next = now + AFK_CHECK_EVERY.min(timeout.0);
    let idle: Vec<Entity> = lobby
        .guests
        .iter()
        .filter(|(_, g)| g.room.is_some() && now.saturating_sub(g.active_at) > timeout.0)
        .map(|(link, _)| *link)
        .collect();
    for link in idle {
        info!(guest = lobby.guests[&link].id, "idle too long, taken out of the room");
        leave(&mut lobby, &mut ctx, link);
        ctx.send(link, LobbyEvent::Left(LeaveReason::Afk));
        ctx.send(link, LobbyEvent::RoomList(lobby.list()));
    }
}

/// Members see their room again when it changed; guests in the browser see the list again when
/// it changed (at most every `LIST_EVERY`).
fn send_updates(mut lobby: ResMut<Lobby>, mut list_sent_at: Local<Option<Duration>>, mut ctx: Ctx) {
    let changed: Vec<RoomKey> = lobby.rooms.iter().filter(|(_, room)| room.changed).map(|(key, _)| *key).collect();
    for key in changed {
        let room = &lobby.rooms[&key];
        let view = lobby.view(key, room);
        for member in room.members.clone() {
            ctx.send(member, LobbyEvent::Room(view.clone()));
        }
        lobby.rooms.get_mut(&key).expect("there").changed = false;
    }
    let now = ctx.time.elapsed();
    if lobby.list_changed && list_sent_at.is_none_or(|at| now >= at + LIST_EVERY) {
        lobby.list_changed = false;
        *list_sent_at = Some(now);
        let list = lobby.list();
        let browsing: Vec<Entity> = lobby.guests.iter().filter(|(_, g)| g.room.is_none()).map(|(link, _)| *link).collect();
        for link in browsing {
            ctx.send(link, LobbyEvent::RoomList(list.clone()));
        }
    }
}

/// A disconnected client leaves their room (their fighter goes with the link anyway) and stops
/// being a guest.
fn forget_disconnected(trigger: On<Add, Disconnected>, mut lobby: ResMut<Lobby>, mut ctx: Ctx) {
    let link = trigger.entity;
    leave(&mut lobby, &mut ctx, link);
    if let Some(guest) = lobby.guests.remove(&link) {
        info!(guest = guest.id, name = guest.name, "guest left");
    }
}
