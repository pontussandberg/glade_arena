//! Guests and rooms. A connected client says `Hello` and becomes a guest with a name; from the
//! server browser it creates or joins a room. Each room is its own arena in this one world: its
//! fighters, projectiles and pickups carry `InRoom` (gameplay only ever pairs things in the same
//! room) and lightyear `Rooms` (only the room's members are sent them).
//!
//! The room's leader (its creator, then the longest-standing member) picks free-for-all or two
//! teams and starts the match, which takes everyone in; members pick their team and class, and
//! once it's on go in and out of the arena as they like (back to the room's lobby to pick another
//! class), latecomers included. Leaving the room takes a member's fighter out of the arena; the
//! room is gone when its last member leaves. A member idle for `AfkTimeout` is taken out as if
//! they'd left.
//!
//! A match started from the lobby is won by the first fighter (or team) to `KILLS_TO_WIN` kills,
//! zombies roaming the arena meanwhile (`zombies.rs`); `MATCH_OVER_AFTER` later everyone is back
//! in the lobby, the result shown there until the next match starts. A quick-joined room's match
//! never ends, and has no zombies.
//!
//! A waves room (`Mode::Waves`) plays on the wilds instead: its start opens a run (`waves.rs`),
//! everyone on one side against the monsters, buying items from its shopkeeper between waves,
//! until the party falls; then everyone is back in the lobby, the wave they reached shown there.
//!
//! Practice is a room of one's own: never listed, no one else can join, started from the off.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use arena_shared::map::{Map, MapId};
use arena_shared::protocol::*;
use arena_shared::rooms::*;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use arena_shared::items::Satchel;

use crate::waves::{Run, RunPhase, START_GOLD, boss_of};
use crate::{Dead, NeedsSpawnPoint, Patrol, spawn_dummy, spawn_gates, spawn_pickups, spawn_player, spawn_shopkeeper};

/// Server-only: the room a fighter, projectile, pickup or waygate is in. The map it plays on is
/// the room's (`Lobby::map`).
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
/// How long after a match is won everyone is taken back to the lobby: time to see who won.
const MATCH_OVER_AFTER: Duration = Duration::from_secs(5);

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
            (greet, handle_class_choices, handle_requests, take_out_idle, end_matches, send_updates, end_lingering).chain().in_set(RoomSystems),
        );
        app.add_systems(FixedUpdate, note_activity);
    }
}

/// A lightyear room (`net`) its client (`link`) has just left, still sent to them until `until`
/// (a tick): what's despawned of the room as they leave it (its fighters, dummies, monsters,
/// shots, all of it when they were the last in it) is still sent to them, so none of it stays on
/// their screen. Without a `link`, a closed room's, free to use again only then.
#[derive(Debug)]
struct Linger {
    link: Option<Entity>,
    net: RoomId,
    until: u32,
}

/// How many ticks a client still gets a room's news after leaving it (`Linger`).
const LINGER_TICKS: u32 = 4;

/// Every guest and room.
#[derive(Resource, Default)]
pub struct Lobby {
    /// By their client's link.
    guests: HashMap<Entity, Guest>,
    rooms: BTreeMap<RoomKey, Room>,
    next_guest: u32,
    next_room: u32,
    /// The last number a fighter no client controls (a target dummy, a zombie) was given: its
    /// `PeerId::Local`.
    next_local: u64,
    /// Lightyear rooms no longer used, to use again (there are only `u16::MAX` of them).
    free_net: Vec<RoomId>,
    /// Lightyear rooms a client has just left, still sent to them a moment (`Linger`).
    lingering: Vec<Linger>,
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
    /// Players they killed in their room's match (or its last one), and zombies.
    kills: u32,
    zombie_kills: u32,
    /// Their gold and items in their room's run (the waves mode).
    satchel: Satchel,
}

struct Room {
    name: String,
    mode: Mode,
    map: MapId,
    net: RoomId,
    /// A link in `members`.
    leader: Entity,
    started: bool,
    /// Never listed, no one joins.
    practice: bool,
    /// Links, in the order they joined.
    members: Vec<Entity>,
    /// What lies on its map: its pickups and waygates.
    things: Vec<Entity>,
    /// Target dummies, in the order they were placed (practice only).
    dummies: Vec<Entity>,
    /// Kills that win its match; `None`: it never ends (practice, quick join).
    goal: Option<u32>,
    /// Each team's kills (`TEAMS` order), in teams.
    team_kills: [u32; 2],
    /// Who won the match (or the last one).
    winner: Option<Winner>,
    /// When the won match is over (`Time<Real>`), everyone back in the lobby.
    ends_at: Option<Duration>,
    /// Members need to see it again.
    changed: bool,
    /// Its run against the waves (or the last one), in the waves mode, and what its members were
    /// last sent of it.
    run: Option<Run>,
    shown_run: Option<RunView>,
    /// The furthest wave it's reached.
    best: u32,
}

impl Lobby {
    /// The map `room` plays on (the default one if it's gone).
    pub fn map(&self, room: &InRoom) -> &'static Map {
        self.rooms.get(&room.key).map_or(MapId::default(), |r| r.map).map()
    }

    /// Who's in `room`, and how many on each team.
    fn on_team(&self, room: &Room, team: u8) -> usize {
        room.members.iter().filter(|link| self.guests.get(link).is_some_and(|g| g.team == team)).count()
    }

    /// The team with fewer members (the first when even).
    fn smaller_team(&self, room: &Room) -> u8 {
        TEAMS.into_iter().min_by_key(|&team| self.on_team(room, team)).expect("two teams")
    }

    fn summary(&self, key: RoomKey, room: &Room) -> RoomSummary {
        RoomSummary { key, name: room.name.clone(), mode: room.mode, map: room.map, players: room.members.len() as u8, started: room.started, closed: !room.joinable() }
    }

    fn view(&self, key: RoomKey, room: &Room) -> RoomView {
        let members = room
            .members
            .iter()
            .filter_map(|link| self.guests.get(link))
            .map(|g| Member { guest_id: g.id, name: g.name.clone(), peer: g.peer, team: g.team, class: g.class, in_arena: g.playing && room.started, kills: g.kills, zombie_kills: g.zombie_kills })
            .collect();
        // The run as `refresh_run` last found it.
        let run = room.shown_run.clone();
        let leader = self.guests.get(&room.leader).map_or(0, |g| g.id);
        RoomView {
            key,
            name: room.name.clone(),
            mode: room.mode,
            map: room.map,
            started: room.started,
            practice: room.practice,
            leader,
            members,
            goal: room.goal,
            team_kills: room.team_kills,
            winner: room.winner.clone(),
            run,
        }
    }

    /// What `room`'s members see of its run at `now`.
    fn run_view(room: &Room, now: u32) -> Option<RunView> {
        room.run.as_ref().map(|run| RunView {
            wave: run.wave,
            phase: run.phase_view(now),
            enemies_left: run.enemies_left(),
            ready: run.ready.clone(),
            waiting: run.waiting.clone(),
            best: room.best,
            boss: boss_of(run.wave).map(|boss| boss.def().name.clone()),
            omen: run.omen,
        })
    }

    /// Room `key`'s members see its run again if what they'd see of it changed (the countdown, a
    /// kill, a life spent, someone ready): checked every tick, sent only then.
    pub(crate) fn refresh_run(&mut self, key: RoomKey, now: u32) {
        let Some(room) = self.rooms.get_mut(&key) else { return };
        let view = Lobby::run_view(room, now);
        if room.shown_run != view {
            room.shown_run = view;
            room.changed = true;
        }
    }

    fn list(&self) -> Vec<RoomSummary> {
        self.rooms.iter().filter(|(_, room)| !room.practice).map(|(key, room)| self.summary(*key, room)).collect()
    }

    /// A number for a new fighter no client controls (a target dummy, a zombie).
    pub(crate) fn next_local(&mut self) -> PeerId {
        self.next_local += 1;
        PeerId::Local(self.next_local)
    }

    /// The rooms whose match is on and not yet won, zombies roaming them.
    pub(crate) fn hunting(&self) -> impl Iterator<Item = InRoom> + '_ {
        self.rooms.iter().filter(|(_, room)| room.hunting()).map(|(key, room)| room.in_room(*key))
    }

    /// Whether monsters belong in room `key` now: zombies on the hunt, or a run on.
    pub(crate) fn keeps_monsters(&self, key: RoomKey) -> bool {
        self.rooms.get(&key).is_some_and(|room| room.hunting() || room.live_run().is_some() || room.practice)
    }

    /// The rooms with a run on.
    pub(crate) fn runs(&self) -> Vec<(RoomKey, InRoom)> {
        self.rooms.iter().filter(|(_, room)| room.live_run().is_some()).map(|(key, room)| (*key, room.in_room(*key))).collect()
    }

    /// Room `key`'s run (or its last), whether it's on or not.
    pub(crate) fn run(&self, key: RoomKey) -> Option<&Run> {
        self.rooms.get(&key)?.run.as_ref()
    }

    /// Whether room `key` is against the waves.
    pub(crate) fn pve(&self, key: RoomKey) -> bool {
        self.rooms.get(&key).is_some_and(|room| room.mode.pve())
    }

    /// How many of room `key`'s members are in its arena (or on their way in).
    pub(crate) fn fighting(&self, key: RoomKey) -> usize {
        let Some(room) = self.rooms.get(&key) else { return 0 };
        room.members.iter().filter(|link| self.guests.get(link).is_some_and(|g| g.playing && g.class.is_some())).count()
    }

    /// Whether room `key` has a run on.
    pub(crate) fn run_on(&self, key: RoomKey) -> bool {
        self.rooms.get(&key).is_some_and(|room| room.live_run().is_some())
    }

    /// Room `key`'s run, to change (its members see it again once `refresh_run` finds they'd see
    /// something new).
    pub(crate) fn run_mut(&mut self, key: RoomKey) -> Option<&mut Run> {
        self.rooms.get_mut(&key)?.run.as_mut()
    }

    /// Room `key`'s members: their links and guest ids.
    pub(crate) fn party(&self, key: RoomKey) -> Vec<(Entity, u32)> {
        let Some(room) = self.rooms.get(&key) else { return Vec::new() };
        room.members.iter().filter_map(|link| self.guests.get(link).map(|g| (*link, g.id))).collect()
    }

    /// `peer` earned `amount` gold in room `key`: against the waves, the whole party gets it; in a
    /// PvP match, they alone do.
    pub(crate) fn reward(&mut self, key: RoomKey, peer: PeerId, amount: u32) {
        if self.pve(key) {
            self.give_gold(key, amount);
        } else if let Some(guest) = self.guests.values_mut().find(|g| g.room == Some(key) && g.peer == peer) {
            guest.satchel.gold += amount;
        }
    }

    /// Everyone in room `key` gets `amount` more gold.
    pub(crate) fn give_gold(&mut self, key: RoomKey, amount: u32) {
        let Some(room) = self.rooms.get(&key) else { return };
        for link in &room.members {
            if let Some(guest) = self.guests.get_mut(link) {
                guest.satchel.gold += amount;
            }
        }
    }

    /// Room `key` has been as far as `wave`.
    pub(crate) fn note_best(&mut self, key: RoomKey, wave: u32) {
        if let Some(room) = self.rooms.get_mut(&key) {
            room.best = room.best.max(wave);
        }
    }

    /// Room `key`'s run is lost: everyone back in the lobby at `ends_at` (`Time<Real>`).
    /// A run is under way: the lobby list is sent again, showing it closed.
    pub(crate) fn close_run(&mut self) {
        self.list_changed = true;
    }

    pub(crate) fn lose_run(&mut self, key: RoomKey, ends_at: Duration) {
        if let Some(room) = self.rooms.get_mut(&key) {
            if let Some(run) = &room.run {
                room.best = room.best.max(run.wave);
            }
            room.ends_at = Some(ends_at);
            room.changed = true;
        }
    }

    /// The satchel the guest on `link` has, if they're one.
    pub(crate) fn satchel(&self, link: Entity) -> Option<Satchel> {
        self.guests.get(&link).map(|g| g.satchel)
    }

    /// The guest id of whoever's on `link`.
    pub(crate) fn guest_id(&self, link: Entity) -> Option<u32> {
        self.guests.get(&link).map(|g| g.id)
    }



    /// Everyone's kills in room `key` back to none, and no winner.
    fn clear_score(&mut self, key: RoomKey) {
        let Some(room) = self.rooms.get_mut(&key) else { return };
        room.team_kills = [0; 2];
        room.winner = None;
        room.changed = true;
        for member in &room.members {
            if let Some(guest) = self.guests.get_mut(member) {
                guest.kills = 0;
                guest.zombie_kills = 0;
            }
        }
    }

    /// `killer` killed a zombie in room `key`: counted if they're a member. How many they had
    /// killed before it.
    pub(crate) fn count_zombie_kill(&mut self, key: RoomKey, killer: PeerId) -> Option<u32> {
        let room = self.rooms.get_mut(&key)?;
        let link = room.members.iter().copied().find(|link| self.guests.get(link).is_some_and(|g| g.peer == killer))?;
        room.changed = true;
        let pvp = !room.mode.pve();
        let guest = self.guests.get_mut(&link).expect("a member");
        guest.zombie_kills += 1;
        if pvp {
            guest.satchel.gold += arena_shared::items::PVP_ZOMBIE_GOLD;
        }
        Some(guest.zombie_kills - 1)
    }

    /// `killer` killed `victim` in room `key`, at `now`: it counts toward the match if both are
    /// members (not zombies), not the same, and it isn't won yet. Reaching the goal wins it.
    fn count_kill(&mut self, key: RoomKey, killer: PeerId, victim: PeerId, now: Duration) {
        let Some(room) = self.rooms.get(&key) else { return };
        let Some(goal) = room.goal.filter(|_| room.winner.is_none()) else { return };
        let member = |peer: PeerId| room.members.iter().copied().find(|link| self.guests.get(link).is_some_and(|g| g.peer == peer));
        let (Some(link), Some(_)) = (member(killer), member(victim)) else { return };
        if killer == victim {
            return;
        }
        let guest = self.guests.get_mut(&link).expect("a member");
        guest.kills += 1;
        guest.satchel.gold += arena_shared::items::PVP_KILL_GOLD;
        let (id, team, kills) = (guest.id, guest.team, guest.kills);
        let room = self.rooms.get_mut(&key).expect("checked");
        room.changed = true;
        let side = TEAMS.iter().position(|&t| t == team).filter(|_| room.mode == Mode::Teams);
        let score = match side {
            Some(side) => {
                room.team_kills[side] += 1;
                room.team_kills[side]
            }
            None => kills,
        };
        debug!(room = key.0, guest = id, kills, score, "kill counted");
        if score >= goal {
            // Won by a team: whoever is on its side now wins with it.
            let winner = match side {
                Some(_) => {
                    let side_of = |on: bool| -> Vec<u32> {
                        let room = &self.rooms[&key];
                        room.members.iter().filter_map(|link| self.guests.get(link)).filter(|g| (g.team == team) == on).map(|g| g.id).collect()
                    };
                    Winner::Team { winners: side_of(true), losers: side_of(false) }
                }
                None => Winner::Guest(id),
            };
            let room = self.rooms.get_mut(&key).expect("checked");
            info!(room = key.0, ?winner, "match won");
            room.winner = Some(winner);
            room.ends_at = Some(now + MATCH_OVER_AFTER);
        }
    }
}

/// What handling a request needs besides the lobby.
#[derive(bevy::ecs::system::SystemParam)]
struct Ctx<'w, 's> {
    commands: Commands<'w, 's>,
    time: Res<'w, Time<Real>>,
    timeline: Res<'w, LocalTimeline>,
    players: Query<'w, 's, (Entity, &'static ControlledBy, &'static Pos, &'static ClassId), With<PlayerId>>,
    projectiles: Query<'w, 's, (Entity, &'static InRoom), With<Projectile>>,
    /// Everything in a room: its fighters, monsters, decoys, shots, effects, pickups, gates.
    in_rooms: Query<'w, 's, (Entity, &'static InRoom)>,
    zombies: Query<'w, 's, (Entity, &'static InRoom, Has<crate::Dead>), With<crate::Zombie>>,
    senders: Query<'w, 's, &'static mut MessageSender<LobbyEvent>>,
}

impl Ctx<'_, '_> {
    fn send(&mut self, link: Entity, event: LobbyEvent) {
        if let Ok(mut sender) = self.senders.get_mut(link) {
            sender.send::<Reliable>(event);
        }
    }

    fn player_of(&self, link: Entity) -> Option<Entity> {
        self.players.iter().find(|(_, c, ..)| c.owner == link).map(|(player, ..)| player)
    }

    /// Where `link`'s fighter is, and what it fights as, if it's in the arena.
    fn fighter_of(&self, link: Entity) -> Option<(Vec2, ClassId)> {
        self.players.iter().find(|(_, c, ..)| c.owner == link).map(|(_, _, pos, class)| (pos.0, *class))
    }

    /// The server's tick.
    fn now(&self) -> u32 {
        self.timeline.tick().0
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
                kills: 0,
                zombie_kills: 0,
                satchel: Satchel::default(),
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
        RoomRequest::Create { name, mode, map } => {
            let name = clean_room_name(&name).ok_or("Give the room a name")?;
            leave(lobby, ctx, link);
            // Each mode on its own maps: the waves on theirs, fights on any of the others.
            let map = mode.fit_map(map);
            let key = create(lobby, allocator, ctx, link, name, mode, map);
            // A lobby's match is won by kills (a quick-joined room's never ends); the waves'
            // goes on until the party falls.
            if !mode.pve() {
                lobby.rooms.get_mut(&key).expect("just made").goal = Some(KILLS_TO_WIN);
            }
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
            if !room.joinable() {
                return Err("That run is under way: join one before its first wave");
            }
            leave(lobby, ctx, link);
            join(lobby, ctx, link, key)?;
        }
        RoomRequest::QuickJoin(name) => {
            let name = clean_room_name(&name).ok_or("Give the room a name")?;
            let found = lobby
                .rooms
                .iter()
                // A started one: a quick-joined room's match is on from the off (one whose run was
                // lost is back in its lobby, and made again).
                .find(|(_, room)| !room.practice && room.started && room.joinable() && room.name == name && room.members.len() < room.mode.capacity())
                .map(|(key, _)| *key);
            if room_key.is_some() && room_key == found {
                return Ok(());
            }
            leave(lobby, ctx, link);
            let key = match found {
                Some(key) => key,
                None => {
                    // One called "Waves..." is a run against the waves, on from the off; "Waves 10"
                    // starts at wave 10 (for trying later waves out).
                    let mode = if name.to_lowercase().starts_with("waves") { Mode::Waves } else { Mode::Ffa };
                    let from_wave = name.split_whitespace().nth(1).and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=200).contains(n));
                    // The default map, if it's one of the mode's; else the mode's own (the wilds).
                    let map = mode.maps().iter().copied().find(|&map| map == MapId::default()).unwrap_or(mode.maps()[0]);
                    let key = create(lobby, allocator, ctx, link, name, mode, map);
                    let now = ctx.now();
                    let room = lobby.rooms.get_mut(&key).expect("just made");
                    room.started = true;
                    if mode.pve() {
                        room.run = Some(Run::new(now, from_wave.unwrap_or(1)));
                    }
                    key
                }
            };
            lobby.guests.get_mut(&link).expect("a guest").playing = true;
            join(lobby, ctx, link, key)?;
        }
        RoomRequest::Practice(map) => {
            leave(lobby, ctx, link);
            let key = create(lobby, allocator, ctx, link, "Practice".into(), Mode::Ffa, map);
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
            // Everyone on a team, taking turns; or no teams; or all on one, against the waves.
            let members = room.members.clone();
            for (n, member) in members.iter().enumerate() {
                if let Some(guest) = lobby.guests.get_mut(member) {
                    guest.team = mode_team(mode, n);
                }
            }
            lobby.clear_score(key);
            let room = lobby.rooms.get_mut(&key).expect("checked");
            room.mode = mode;
            room.goal = (!mode.pve()).then_some(KILLS_TO_WIN);
            // The last run was against the waves: gone with them.
            if !mode.pve() {
                room.run = None;
                room.shown_run = None;
            }
            room.changed = true;
            lobby.list_changed = true;
            // Onto a map the mode plays on.
            let fit = mode.fit_map(room.map);
            if fit != room.map {
                switch_map(lobby, ctx, key, fit);
            }
        }
        RoomRequest::SetMap(map) => {
            let key = room_key.ok_or("You're not in a room")?;
            let room = lobby.rooms.get_mut(&key).expect("in it");
            if room.leader != link {
                return Err("Only the leader can change the map");
            }
            if room.started && !room.practice {
                return Err("The match has started");
            }
            if room.map == map {
                return Ok(());
            }
            if !room.practice && !room.mode.maps().contains(&map) {
                return Err("That map isn't for this mode");
            }
            switch_map(lobby, ctx, key, map);
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
        RoomRequest::PlaceDummy { at, walks } => {
            let key = room_key.ok_or("You're not in a room")?;
            let room = lobby.rooms.get_mut(&key).expect("in it");
            if !room.practice {
                return Err("Target dummies are for practice");
            }
            if room.dummies.len() >= MAX_DUMMIES {
                return Err("That's enough dummies");
            }
            if !at.is_finite() || !room.map.map().walkable_at(at) {
                return Err("A dummy can't stand there");
            }
            let patrol = if walks { Some(Patrol::through(room.map.map(), at).ok_or("No room for a dummy to walk there")?) } else { None };
            // Each class in turn.
            let class = ClassId::playable().nth(room.dummies.len() % ClassId::playable().count()).expect("a class");
            let in_room = room.in_room(key);
            let dummy = spawn_dummy(&mut ctx.commands, lobby.next_local(), class, in_room, at);
            let room = lobby.rooms.get_mut(&key).expect("in it");
            if let Some(patrol) = patrol {
                ctx.commands.entity(dummy).insert(patrol);
            }
            room.dummies.push(dummy);
            info!(room = key.0, class = class.def().name, walks, "target dummy placed");
        }
        RoomRequest::PlaceZombies { at } => {
            let key = room_key.ok_or("You're not in a room")?;
            let room = lobby.rooms.get(&key).expect("in it");
            if !room.practice {
                return Err("Zombies to order are for practice");
            }
            if ctx.zombies.iter().filter(|(_, r, dead)| r.key == key && !dead).count() + 3 > MAX_PRACTICE_ZOMBIES {
                return Err("That's enough zombies");
            }
            let map = room.map.map();
            if !at.is_finite() || !map.walkable_at(at) {
                return Err("Zombies can't stand there");
            }
            let in_room = room.in_room(key);
            // Side by side, across the way to us (each where there's room, else on the spot).
            let toward = ctx.fighter_of(link).and_then(|(pos, _)| (pos - at).try_normalize()).unwrap_or(Vec2::Y);
            let side = toward.perp() * 1.1;
            for spot in [at - side, at, at + side] {
                let spot = if map.walkable_at(spot) && map.line_walkable(at, spot) { spot } else { at };
                let zombie = crate::zombies::spawn_zombie(&mut ctx.commands, lobby.next_local(), in_room, spot);
                // Sent to clients from the start: placed already, they skip `place_players`, where
                // others start being sent.
                ctx.commands.entity(zombie).insert((crate::zombies::Sluggish(crate::zombies::PRACTICE_ZOMBIE_SPEED), crate::replicated_npc()));
            }
            info!(room = key.0, "practice zombies placed");
        }
        RoomRequest::NoCooldowns(on) => {
            let key = room_key.ok_or("You're not in a room")?;
            if !lobby.rooms[&key].practice {
                return Err("No cooldowns is for practice");
            }
            let player = ctx.player_of(link).ok_or("You're not in the arena")?;
            ctx.commands.entity(player).entry::<Cooldowns>().and_modify(move |mut cooldowns| cooldowns.free = on);
        }
        RoomRequest::Buy(item) => {
            let key = room_key.ok_or("You're not in a room")?;
            // For the fighter they are, whatever they've picked to be next.
            let class = near_the_shop(lobby, ctx, link, key)?;
            let free = lobby.rooms[&key].practice;
            lobby.guests.get_mut(&link).expect("checked").satchel.buy(item, class, free)?;
            info!(room = key.0, guest = lobby.guests[&link].id, item = item.name(), level = lobby.guests[&link].satchel.level(item), "bought");
        }
        RoomRequest::Sell(slot) => {
            let key = room_key.ok_or("You're not in a room")?;
            near_the_shop(lobby, ctx, link, key)?;
            lobby.guests.get_mut(&link).expect("checked").satchel.sell(slot as usize).ok_or("Nothing there to sell")?;
        }
        RoomRequest::Ready(ready) => {
            let key = room_key.ok_or("You're not in a room")?;
            let id = lobby.guests[&link].id;
            let run = lobby.run_mut(key).filter(|run| matches!(run.phase, RunPhase::Shop { .. })).ok_or("No wave to be ready for")?;
            run.ready.retain(|r| *r != id);
            if ready {
                run.ready.push(id);
            }
        }
        RoomRequest::Start => {
            let key = room_key.ok_or("You're not in a room")?;
            let now = ctx.now();
            let room = lobby.rooms.get_mut(&key).expect("in it");
            if room.leader != link {
                return Err("Only the leader can start the match");
            }
            if room.started {
                return Ok(());
            }
            room.started = true;
            room.ends_at = None;
            lobby.list_changed = true;
            info!(room = key.0, name = room.name, mode = ?room.mode, players = room.members.len(), "match started");
            let members = room.members.clone();
            // Every match starts with empty hands: against the waves, with some gold.
            let gold = if room.mode.pve() { START_GOLD } else { 0 };
            if room.mode.pve() {
                room.run = Some(Run::new(now, 1));
            }
            for member in &members {
                if let Some(guest) = lobby.guests.get_mut(member) {
                    guest.satchel = Satchel { gold, ..default() };
                }
            }
            lobby.clear_score(key);
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
/// The most zombies a practice room may have about.
const MAX_PRACTICE_ZOMBIES: usize = 12;

/// A new room on `map`, led by `leader` (who still has to `join` it).
#[allow(clippy::too_many_arguments)]
fn create(lobby: &mut Lobby, allocator: &mut RoomAllocator, ctx: &mut Ctx, leader: Entity, name: String, mode: Mode, map: MapId) -> RoomKey {
    lobby.next_room += 1;
    let key = RoomKey(lobby.next_room);
    let net = lobby.free_net.pop().unwrap_or_else(|| allocator.allocate());
    let things = spawn_room_things(&mut ctx.commands, InRoom { key, net }, map);
    info!(room = key.0, name, ?mode, leader = lobby.guests[&leader].id, "room created");
    lobby.rooms.insert(key, Room {
        name,
        mode,
        map,
        net,
        leader,
        started: false,
        practice: false,
        members: Vec::new(),
        things,
        dummies: Vec::new(),
        goal: None,
        team_kills: [0; 2],
        winner: None,
        ends_at: None,
        changed: true,
        run: None,
        shown_run: None,
        best: 0,
    });
    key
}

/// What lies on `map` in `room`: its pickups and waygates, and its shopkeeper if it has a shop.
fn spawn_room_things(commands: &mut Commands, room: InRoom, map: MapId) -> Vec<Entity> {
    let mut things = spawn_pickups(commands, room, map.map());
    things.extend(spawn_gates(commands, room, map.map()));
    if let Some(at) = map.map().shop {
        things.push(spawn_shopkeeper(commands, room, at));
    }
    things
}

/// Room `key` onto `map`: what lay on the old map goes, and the new one's comes. Practice may
/// change it with everyone in: they start over on the new map (from a spawn point), and shots in
/// the air are gone with the old one.
fn switch_map(lobby: &mut Lobby, ctx: &mut Ctx, key: RoomKey, map: MapId) {
    let Some(room) = lobby.rooms.get_mut(&key) else { return };
    info!(room = key.0, from = ?room.map, to = ?map, "map changed");
    room.map = map;
    room.changed = true;
    lobby.list_changed = true;
    room.clear_things(&mut ctx.commands);
    let in_room = room.in_room(key);
    room.things = spawn_room_things(&mut ctx.commands, in_room, map);
    for (projectile, _) in ctx.projectiles.iter().filter(|(_, r)| **r == in_room) {
        ctx.commands.entity(projectile).try_despawn();
    }
    // Zombies (practice's) stood on the old map: gone with it.
    for (zombie, ..) in ctx.zombies.iter().filter(|(_, r, _)| **r == in_room) {
        ctx.commands.entity(zombie).try_despawn();
    }
    for &member in &room.members {
        if let Some(player) = ctx.player_of(member) {
            ctx.commands.entity(player).insert(NeedsSpawnPoint);
        }
    }
}

/// The team a mode puts its `n`th member on.
fn mode_team(mode: Mode, n: usize) -> u8 {
    match mode {
        Mode::Ffa => NO_TEAM,
        Mode::Teams => TEAMS[n % 2],
        Mode::Waves => TEAM_A,
    }
}

/// Whether `link` may trade now: in a run, their fighter by the shopkeeper. What it fights as.
fn near_the_shop(lobby: &Lobby, ctx: &Ctx, link: Entity, key: RoomKey) -> Result<ClassId, &'static str> {
    let room = &lobby.rooms[&key];
    if !room.practice && !(room.started && if room.mode.pve() { room.live_run().is_some() } else { room.hunting() }) {
        return Err("The shop is open while a match is on");
    }
    let shop = room.map.map().shop.ok_or("There's no shop here")?;
    let (pos, class) = ctx.fighter_of(link).ok_or("You're not in the arena")?;
    if pos.distance(shop) > SHOP_RADIUS + 0.5 {
        return Err("Walk up to the merchant to trade");
    }
    Ok(class)
}

impl Room {
    /// Despawns what lies on its map, and its dummies.
    fn clear_things(&mut self, commands: &mut Commands) {
        for thing in self.things.drain(..).chain(self.dummies.drain(..)) {
            commands.entity(thing).try_despawn();
        }
    }

    fn in_room(&self, key: RoomKey) -> InRoom {
        InRoom { key, net: self.net }
    }

    /// Its run, while it's on.
    fn live_run(&self) -> Option<&Run> {
        self.run.as_ref().filter(|run| self.started && run.on())
    }

    /// Whether anyone may join it now: anything but a run against the waves under way (once its
    /// first wave has come, or it's lost).
    fn joinable(&self) -> bool {
        !self.started || !self.mode.pve() || self.run.as_ref().is_some_and(|run| run.on() && !run.fought)
    }

    /// Its match is on and not yet won.
    fn hunting(&self) -> bool {
        self.started && self.goal.is_some() && self.winner.is_none()
    }
}

/// Into room `key`, which has room for one more; on the smaller team if there are teams. In a
/// started match they enter the arena once they've picked a class.
fn join(lobby: &mut Lobby, ctx: &mut Ctx, link: Entity, key: RoomKey) -> Result<(), &'static str> {
    let room = lobby.rooms.get(&key).ok_or("That room is gone")?;
    let team = match room.mode {
        Mode::Teams => lobby.smaller_team(room),
        mode => mode_team(mode, 0),
    };
    // Into a run under way: with about what everyone else has earned.
    let gold = room.live_run().map(Run::catch_up_gold);
    let net = room.net;
    let room = lobby.rooms.get_mut(&key).expect("checked");
    room.members.push(link);
    room.changed = true;
    lobby.list_changed = true;
    let guest = lobby.guests.get_mut(&link).expect("a guest");
    guest.room = Some(key);
    guest.team = team;
    guest.kills = 0;
    guest.zombie_kills = 0;
    guest.satchel = Satchel { gold: gold.unwrap_or_default(), ..default() };
    info!(room = key.0, guest = guest.id, "joined a room");
    ctx.commands.entity(link).insert(lobby.nets_of(link, Some(net)));
    enter_arena(lobby, ctx, link);
    Ok(())
}

/// Spawns `link`'s fighter if their room has started, they want in, have picked a class and
/// aren't in yet. Items carried that the class can't (they were another class) are given back.
fn enter_arena(lobby: &mut Lobby, ctx: &mut Ctx, link: Entity) {
    let Some(guest) = lobby.guests.get_mut(&link) else { return };
    let (Some(key), Some(class)) = (guest.room, guest.class) else { return };
    if !guest.playing {
        return;
    }
    let room = &lobby.rooms[&key];
    if !room.started || ctx.player_of(link).is_some() {
        return;
    }
    guest.satchel.refund_unfit(class);
    info!(room = key.0, guest = guest.id, class = class.def().name, "entered the arena");
    let in_room = room.in_room(key);
    spawn_player(&mut ctx.commands, link, guest.peer, class, in_room, Team(guest.team));
}

/// Out of their room, if they're in one: their fighter goes, and so does the room if it's
/// empty now; if they led it, the next member leads. Whether they were in one.
fn leave(lobby: &mut Lobby, ctx: &mut Ctx, link: Entity) -> bool {
    let Some(guest) = lobby.guests.get_mut(&link) else { return false };
    let Some(key) = guest.room.take() else { return false };
    guest.team = NO_TEAM;
    guest.playing = false;
    guest.satchel = Satchel::default();
    let id = guest.id;
    if let Some(player) = ctx.player_of(link) {
        ctx.commands.entity(player).try_despawn();
    }
    // Still sent the room's news a moment: what goes as they leave goes from their screen too.
    let until = ctx.now() + LINGER_TICKS;
    lobby.list_changed = true;
    let Some(room) = lobby.rooms.get_mut(&key) else { return true };
    lobby.lingering.push(Linger { link: Some(link), net: room.net, until });
    let room = lobby.rooms.get_mut(&key).expect("there");
    room.members.retain(|m| *m != link);
    room.changed = true;
    info!(room = key.0, guest = id, "left a room");
    match room.members.first() {
        None => {
            let mut room = lobby.rooms.remove(&key).expect("there");
            room.clear_things(&mut ctx.commands);
            // And whatever else was in it: monsters, decoys, shots still flying, effects.
            for (thing, _) in ctx.in_rooms.iter().filter(|(_, r)| r.key == key) {
                ctx.commands.entity(thing).try_despawn();
            }
            if let Some(run) = room.run.as_mut() {
                run.clear_chests(&mut ctx.commands);
            }
            // Free again once no one's sent its news any more.
            lobby.lingering.push(Linger { link: None, net: room.net, until: until + 1 });
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
            let Some(class) = class.pickable() else { continue };
            let Some(guest) = lobby.guests.get_mut(&link) else { continue };
            guest.active_at = now;
            guest.class = Some(class);
            if let Some(key) = guest.room
                && let Some(room) = lobby.rooms.get_mut(&key)
            {
                room.changed = true;
            }
            enter_arena(&mut lobby, &mut ctx, link);
        }
    }
}

/// A fighter whose intent changes (by its player's commands) isn't idle.
fn note_activity(
    time: Res<Time<Real>>,
    mut lobby: ResMut<Lobby>,
    players: Query<(&ControlledBy, &PlayerInput), With<PlayerId>>,
) {
    for (controlled_by, input) in &players {
        if let Some(guest) = lobby.guests.get_mut(&controlled_by.owner)
            && guest.last_input != *input
        {
            guest.last_input = *input;
            guest.active_at = time.elapsed();
        }
    }
}

/// A fighter killed by another player counts toward their room's match (`Lobby::count_kill`): in
/// the tick it happens, before the victim (or the killer) can leave.
pub(crate) fn count_kills(time: Res<Time<Real>>, mut lobby: ResMut<Lobby>, killed: Query<(&PlayerId, &InRoom, &Dead), Added<Dead>>) {
    for (victim, room, dead) in &killed {
        lobby.count_kill(room.key, dead.killer, victim.0, time.elapsed());
    }
}

/// A won match is over once its result has shown a while: every fighter leaves the arena (the
/// zombies go too, `bury_zombies`, and shots in the air), and the room is back in its lobby, not
/// started, its leader free to change it and start the next.
fn end_matches(mut lobby: ResMut<Lobby>, mut ctx: Ctx) {
    let now = ctx.time.elapsed();
    let over: Vec<RoomKey> = lobby.rooms.iter().filter(|(_, room)| room.ends_at.is_some_and(|at| now >= at)).map(|(key, _)| *key).collect();
    for key in over {
        let room = lobby.rooms.get_mut(&key).expect("there");
        room.started = false;
        room.ends_at = None;
        room.changed = true;
        let in_room = room.in_room(key);
        let members = room.members.clone();
        lobby.list_changed = true;
        for (projectile, _) in ctx.projectiles.iter().filter(|(_, r)| **r == in_room) {
            ctx.commands.entity(projectile).try_despawn();
        }
        for member in members {
            if let Some(guest) = lobby.guests.get_mut(&member) {
                guest.playing = false;
            }
            if let Some(player) = ctx.player_of(member) {
                ctx.commands.entity(player).try_despawn();
            }
        }
        info!(room = key.0, "match over, back to the lobby");
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

impl Lobby {
    /// The lightyear rooms `link` is sent: the room they're in (`now`, if it's just been decided)
    /// and any they've just left (`Linger`).
    fn nets_of(&self, link: Entity, now: Option<RoomId>) -> Rooms {
        let current = now.or_else(|| self.guests.get(&link).and_then(|g| g.room).and_then(|key| self.rooms.get(&key)).map(|room| room.net));
        Rooms::from(current.into_iter().chain(self.lingering.iter().filter(|l| l.link == Some(link)).map(|l| l.net)))
    }
}

/// The rooms clients left a moment ago stop being sent to them, and closed ones are free to use
/// again (`Linger`).
fn end_lingering(mut commands: Commands, timeline: Res<LocalTimeline>, mut lobby: ResMut<Lobby>) {
    let now = timeline.tick().0;
    if lobby.lingering.iter().all(|l| now < l.until) {
        return;
    }
    let (done, still): (Vec<Linger>, Vec<Linger>) = std::mem::take(&mut lobby.lingering).into_iter().partition(|l| now >= l.until);
    lobby.lingering = still;
    for linger in done {
        match linger.link {
            Some(link) => {
                let nets = lobby.nets_of(link, None);
                if nets.rooms().next().is_some() {
                    commands.entity(link).try_insert(nets);
                } else {
                    commands.entity(link).try_remove::<Rooms>();
                }
            }
            None => lobby.free_net.push(linger.net),
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
