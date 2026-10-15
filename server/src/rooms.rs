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
            (greet, handle_class_choices, handle_requests, take_out_idle, end_matches, send_updates).chain().in_set(RoomSystems),
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
    /// The last number a fighter no client controls (a target dummy, a zombie) was given: its
    /// `PeerId::Local`.
    next_local: u64,
    /// Lightyear rooms no longer used, to use again (there are only `u16::MAX` of them).
    free_net: Vec<RoomId>,
    /// The browser's list changed since it was last sent.
    list_changed: bool,
    /// The tick the fight is on (as `run_waves` last saw it), for what's set going by it.
    now: u32,
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
    /// Its run against the waves (or the last one), in the waves mode.
    run: Option<Run>,
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
        RoomSummary { key, name: room.name.clone(), mode: room.mode, map: room.map, players: room.members.len() as u8, started: room.started }
    }

    fn view(&self, key: RoomKey, room: &Room) -> RoomView {
        let members = room
            .members
            .iter()
            .filter_map(|link| self.guests.get(link))
            .map(|g| Member { guest_id: g.id, name: g.name.clone(), peer: g.peer, team: g.team, class: g.class, in_arena: g.playing && room.started, kills: g.kills, zombie_kills: g.zombie_kills })
            .collect();
        let run = room.run.as_ref().map(|run| RunView {
            wave: run.wave,
            phase: run.phase_view(self.now),
            lives: run.lives,
            enemies_left: run.enemies_left(),
            ready: run.ready.clone(),
            waiting: run.waiting.clone(),
            best: room.best,
            boss: boss_of(run.wave).map(|boss| boss.def().name.clone()),
            omen: run.omen,
            life_price: life_price(run.lives_bought),
        });
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
        self.rooms.get(&key).is_some_and(|room| room.hunting() || room.started && room.run.as_ref().is_some_and(Run::on))
    }

    /// The rooms with a run on.
    pub(crate) fn runs(&self) -> Vec<(RoomKey, InRoom)> {
        self.rooms.iter().filter(|(_, room)| room.started && room.run.as_ref().is_some_and(Run::on)).map(|(key, room)| (*key, room.in_room(*key))).collect()
    }

    pub(crate) fn run(&self, key: RoomKey) -> Option<&Run> {
        self.rooms.get(&key)?.run.as_ref()
    }

    /// Room `key`'s run, to change: its members see it again.
    pub(crate) fn run_mut(&mut self, key: RoomKey) -> Option<&mut Run> {
        let room = self.rooms.get_mut(&key)?;
        room.changed = true;
        room.run.as_mut()
    }

    /// Room `key`'s members see it again.
    pub(crate) fn touch(&mut self, key: RoomKey) {
        if let Some(room) = self.rooms.get_mut(&key) {
            room.changed = true;
        }
    }

    /// Room `key`'s members: their links and guest ids.
    pub(crate) fn party(&self, key: RoomKey) -> Vec<(Entity, u32)> {
        let Some(room) = self.rooms.get(&key) else { return Vec::new() };
        room.members.iter().filter_map(|link| self.guests.get(link).map(|g| (*link, g.id))).collect()
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

    pub(crate) fn now(&self) -> u32 {
        self.now
    }

    pub(crate) fn set_now(&mut self, now: u32) {
        self.now = now;
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
        let guest = self.guests.get_mut(&link).expect("a member");
        guest.zombie_kills += 1;
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
    players: Query<'w, 's, (Entity, &'static ControlledBy, &'static Pos), With<PlayerId>>,
    projectiles: Query<'w, 's, (Entity, &'static InRoom), With<Projectile>>,
    senders: Query<'w, 's, &'static mut MessageSender<LobbyEvent>>,
}

impl Ctx<'_, '_> {
    fn send(&mut self, link: Entity, event: LobbyEvent) {
        if let Ok(mut sender) = self.senders.get_mut(link) {
            sender.send::<Reliable>(event);
        }
    }

    fn player_of(&self, link: Entity) -> Option<Entity> {
        self.players.iter().find(|(_, c, _)| c.owner == link).map(|(player, ..)| player)
    }

    /// Where `link`'s fighter is, if it's in the arena.
    fn pos_of(&self, link: Entity) -> Option<Vec2> {
        self.players.iter().find(|(_, c, _)| c.owner == link).map(|(.., pos)| pos.0)
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
            let map = if mode.maps().contains(&map) { map } else { mode.maps()[0] };
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
                    // One called "Waves..." is a run against the waves, on from the off; "Waves 10"
                    // starts at wave 10 (for trying later waves out).
                    let mode = if name.to_lowercase().starts_with("waves") { Mode::Waves } else { Mode::Ffa };
                    let from_wave = name.split_whitespace().nth(1).and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=200).contains(n));
                    let key = create(lobby, allocator, ctx, link, name, mode, mode.maps()[0]);
                    let now = lobby.now;
                    let room = lobby.rooms.get_mut(&key).expect("just made");
                    room.started = true;
                    if mode.pve() {
                        let mut run = Run::new(now);
                        run.wave = from_wave.unwrap_or(1);
                        room.run = Some(run);
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
            room.changed = true;
            lobby.list_changed = true;
            // Onto a map the mode plays on.
            if !mode.maps().contains(&room.map) {
                switch_map(lobby, ctx, key, mode.maps()[0]);
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
            near_the_shop(lobby, ctx, link, key)?;
            let class = lobby.guests[&link].class.ok_or("Pick a fighter first")?;
            lobby.guests.get_mut(&link).expect("checked").satchel.buy(item, class)?;
            info!(room = key.0, guest = lobby.guests[&link].id, item = item.name(), level = lobby.guests[&link].satchel.level(item), "bought");
        }
        RoomRequest::BuyLife => {
            let key = room_key.ok_or("You're not in a room")?;
            near_the_shop(lobby, ctx, link, key)?;
            let price = lobby.run(key).map_or(0, |run| life_price(run.lives_bought));
            let satchel = &mut lobby.guests.get_mut(&link).expect("checked").satchel;
            if satchel.gold < price {
                return Err("Not enough gold");
            }
            satchel.gold -= price;
            let run = lobby.run_mut(key).expect("near the shop in a run");
            run.lives += 1;
            run.lives_bought += 1;
            info!(room = key.0, guest = lobby.guests[&link].id, price, "bought a life");
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
            let now = lobby.now;
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
            if room.mode.pve() {
                room.run = Some(Run::new(now));
                for member in &members {
                    if let Some(guest) = lobby.guests.get_mut(member) {
                        guest.satchel = Satchel { gold: START_GOLD, ..default() };
                    }
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

/// Whether `link` may trade now: in a run, their fighter by the shopkeeper.
fn near_the_shop(lobby: &Lobby, ctx: &Ctx, link: Entity, key: RoomKey) -> Result<(), &'static str> {
    let room = &lobby.rooms[&key];
    if !room.started || !room.run.as_ref().is_some_and(Run::on) {
        return Err("The shop is open in a run against the waves");
    }
    let shop = room.map.map().shop.ok_or("There's no shop here")?;
    let pos = ctx.pos_of(link).ok_or("You're not in the arena")?;
    if pos.distance(shop) > SHOP_RADIUS + 0.5 {
        return Err("Walk up to the merchant to trade");
    }
    Ok(())
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
    let gold = room.run.as_ref().filter(|run| room.started && run.on()).map(|run| run.catch_up_gold());
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
    ctx.commands.entity(link).try_remove::<Rooms>();
    lobby.list_changed = true;
    let Some(room) = lobby.rooms.get_mut(&key) else { return true };
    room.members.retain(|m| *m != link);
    room.changed = true;
    info!(room = key.0, guest = id, "left a room");
    match room.members.first() {
        None => {
            let mut room = lobby.rooms.remove(&key).expect("there");
            room.clear_things(&mut ctx.commands);
            for chest in room.run.as_mut().map(Run::take_chests).unwrap_or_default() {
                ctx.commands.entity(chest).try_despawn();
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
            let Some(class) = class.pickable() else { continue };
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

/// A disconnected client leaves their room (their fighter goes with the link anyway) and stops
/// being a guest.
fn forget_disconnected(trigger: On<Add, Disconnected>, mut lobby: ResMut<Lobby>, mut ctx: Ctx) {
    let link = trigger.entity;
    leave(&mut lobby, &mut ctx, link);
    if let Some(guest) = lobby.guests.remove(&link) {
        info!(guest = guest.id, name = guest.name, "guest left");
    }
}
