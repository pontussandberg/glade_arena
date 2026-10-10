//! Rooms end to end: guests creating, joining and leaving rooms, teams, rooms kept apart, and
//! idle members taken out.

use std::time::{Duration, Instant};

use arena_client::rooms::{CurrentRoom, Me, Notice, RoomList};
use arena_server::rooms::AfkTimeout;
use arena_shared::protocol::*;
use arena_shared::rooms::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::common::*;

/// A server and some clients, stepped together.
struct Group {
    server: App,
    clients: Vec<App>,
}

impl Group {
    fn update(&mut self) {
        self.server.update();
        for client in &mut self.clients {
            client.update();
        }
        std::thread::sleep(Duration::from_millis(3));
    }

    fn until(&mut self, timeout: Duration, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let end = Instant::now() + timeout;
        while !done(self) {
            assert!(Instant::now() < end, "timed out waiting for: {what}");
            self.update();
        }
    }

    fn run(&mut self, duration: Duration) {
        let end = Instant::now() + duration;
        while Instant::now() < end {
            self.update();
        }
    }

    fn ask(&mut self, client: usize, request: RoomRequest) {
        let world = self.clients[client].world_mut();
        let mut sender = world.query_filtered::<&mut MessageSender<RoomRequest>, With<Client>>();
        sender.single_mut(world).expect("a client").send::<Reliable>(request);
    }

    fn room(&self, client: usize) -> Option<RoomView> {
        self.clients[client].world().resource::<CurrentRoom>().0.clone()
    }

    fn me(&self, client: usize) -> Option<Me> {
        self.clients[client].world().get_resource::<Me>().cloned()
    }

    fn rooms_listed(&self, client: usize) -> Vec<RoomSummary> {
        self.clients[client].world().resource::<RoomList>().0.clone()
    }
}

/// Guests that stay in the browser until told otherwise; ids 1, 2, ...
fn guests(port: u16, classes: &[&str], setup: impl FnOnce(&mut App)) -> Group {
    let server = start_server(port, setup);
    let clients = classes.iter().enumerate().map(|(n, class)| start_guest(n as u64 + 1, port, class)).collect();
    let mut g = Group { server, clients };
    g.until(Duration::from_secs(15), "everyone welcomed", |g| (0..g.clients.len()).all(|c| g.me(c).is_some()));
    g
}

fn pickups_on_server(server: &mut App) -> usize {
    server.world_mut().query::<&Pickup>().iter(server.world()).count()
}

#[test]
fn rooms_are_listed_joined_and_closed_with_the_leader_passed_on() {
    let mut g = guests(5870, &["revenant", "javelinist"], |_| {});
    g.ask(0, RoomRequest::Create { name: "  Friends \n".into(), mode: Mode::Ffa });
    g.until(Duration::from_secs(5), "B sees the room listed", |g| g.rooms_listed(1).len() == 1);
    let listed = g.rooms_listed(1)[0].clone();
    assert_eq!(listed.name, "Friends");
    assert!(!listed.started);
    assert_eq!(pickups_on_server(&mut g.server), arena_shared::map::PICKUP_SPOTS.len());

    g.ask(1, RoomRequest::Join(listed.key));
    g.until(Duration::from_secs(5), "both in the room", |g| g.room(0).is_some_and(|r| r.members.len() == 2));
    let a = g.me(0).unwrap().guest_id;
    let b = g.me(1).unwrap().guest_id;
    assert_eq!(g.room(1).unwrap().leader, a);

    // Only the leader starts it.
    g.ask(1, RoomRequest::Start);
    g.until(Duration::from_secs(5), "B told it can't start", |g| g.clients[1].world().resource::<Notice>().0.is_some());
    assert!(!g.room(0).unwrap().started);

    // The leader leaves: B leads, A is back in the browser.
    g.ask(0, RoomRequest::Leave);
    g.until(Duration::from_secs(5), "B leads", |g| g.room(1).is_some_and(|r| r.leader == b && r.members.len() == 1));
    assert!(g.room(0).is_none());

    // The last one leaves: the room is gone, its pickups too.
    g.ask(1, RoomRequest::Leave);
    g.until(Duration::from_secs(5), "the room is gone", |g| g.rooms_listed(0).is_empty() && g.room(1).is_none());
    g.run(Duration::from_millis(100));
    assert_eq!(pickups_on_server(&mut g.server), 0);
}

#[test]
fn rooms_are_kept_apart() {
    let mut g = guests(5871, &["revenant", "javelinist"], |_| {});
    g.ask(0, RoomRequest::QuickJoin("One".into()));
    g.ask(1, RoomRequest::QuickJoin("Two".into()));
    g.until(Duration::from_secs(10), "both fighting on the server", |g| {
        player::<Health>(&mut g.server, A).is_some() && player::<Health>(&mut g.server, B).is_some()
    });
    g.run(Duration::from_millis(1500));
    assert!(sees(&mut g.clients[0], A).is_some(), "A should see itself");
    assert!(sees(&mut g.clients[0], B).is_none(), "A sees B, in another room");
    assert!(sees(&mut g.clients[1], A).is_none(), "B sees A, in another room");
    let pickups_seen = |client: &mut App| client.world_mut().query::<&Pickup>().iter(client.world()).count();
    assert_eq!(pickups_seen(&mut g.clients[0]), arena_shared::map::PICKUP_SPOTS.len(), "A sees only its room's pickups");

    // Even standing on the same spot, they can't hit each other.
    let spot = Vec2::new(-22.5, -6.5);
    place(&mut g.server, A, spot);
    place(&mut g.server, B, spot + Vec2::X);
    g.run(Duration::from_millis(300));
    let full = server_player(&mut g.server, B).1;
    edit_input(&mut g.clients[0], |i| {
        i.aim = Vec2::X;
        i.fire = true;
    });
    g.run(Duration::from_millis(1500));
    assert_eq!(server_player(&mut g.server, B).1, full, "a swing hit someone in another room");
}

#[test]
fn allies_cant_hurt_each_other() {
    let mut g = guests(5872, &["revenant", "javelinist"], |_| {});
    g.ask(0, RoomRequest::Create { name: "Teams".into(), mode: Mode::Teams });
    g.until(Duration::from_secs(5), "listed", |g| g.rooms_listed(1).len() == 1);
    let key = g.rooms_listed(1)[0].key;
    g.ask(1, RoomRequest::Join(key));
    g.until(Duration::from_secs(5), "B on the other team", |g| {
        g.room(0).is_some_and(|r| r.members.len() == 2 && r.on_team(RED) == 1 && r.on_team(BLUE) == 1)
    });
    // B joins A's team, and the leader starts.
    g.ask(1, RoomRequest::SetTeam(RED));
    g.until(Duration::from_secs(5), "both red", |g| g.room(0).is_some_and(|r| r.on_team(RED) == 2));
    g.ask(0, RoomRequest::Start);
    g.until(Duration::from_secs(10), "both fighting", |g| {
        player::<Health>(&mut g.server, A).is_some() && player::<Health>(&mut g.server, B).is_some()
    });
    g.run(Duration::from_millis(1500));
    assert_eq!(player::<Team>(&mut g.clients[0], B), Some(Team(RED)), "A sees B on its team");

    let spot = Vec2::new(-22.5, -6.5);
    place(&mut g.server, A, spot);
    place(&mut g.server, B, spot + Vec2::X);
    g.run(Duration::from_millis(300));
    let full = server_player(&mut g.server, B).1;
    edit_input(&mut g.clients[0], |i| {
        i.aim = Vec2::X;
        i.fire = true;
    });
    g.run(Duration::from_millis(1500));
    assert_eq!(server_player(&mut g.server, B).1, full, "an ally's swing hurt");

    // On the other side, the same swings land.
    g.ask(1, RoomRequest::SetTeam(BLUE));
    g.until(Duration::from_secs(3), "B hit once on the other team", |g| server_player(&mut g.server, B).1 < full);
}

#[test]
fn idle_members_are_taken_out() {
    let mut g = guests(5873, &["javelinist"], |server| {
        server.insert_resource(AfkTimeout(Duration::from_secs(2)));
    });
    g.ask(0, RoomRequest::QuickJoin("Idle".into()));
    g.until(Duration::from_secs(10), "in the arena", |g| player::<Health>(&mut g.server, A).is_some());
    g.until(Duration::from_secs(10), "taken out for idling", |g| g.room(0).is_none());
    assert!(player::<Health>(&mut g.server, A).is_none(), "the idle fighter is still in the arena");
    assert!(g.clients[0].world().resource::<Notice>().0.is_some(), "no word on why");
    g.run(Duration::from_millis(100));
    assert!(g.rooms_listed(0).is_empty(), "the empty room is still listed");
}

#[test]
fn practice_is_unlisted_and_fighters_step_out_to_the_lobby() {
    let mut g = guests(5874, &["revenant", "javelinist"], |_| {});
    g.ask(0, RoomRequest::Practice);
    g.until(Duration::from_secs(10), "A practicing", |g| player::<Health>(&mut g.server, A).is_some());
    assert!(g.room(0).unwrap().practice);
    g.run(Duration::from_millis(700));
    assert!(g.rooms_listed(1).is_empty(), "practice is listed");

    // B starts a lobby; A leaves practice and joins it mid-match, in once they ask.
    g.ask(1, RoomRequest::Create { name: "Lobby".into(), mode: Mode::Ffa });
    g.until(Duration::from_secs(5), "B in its lobby", |g| g.room(1).is_some());
    g.ask(1, RoomRequest::Start);
    g.until(Duration::from_secs(10), "B fighting", |g| player::<Health>(&mut g.server, B).is_some());
    g.ask(0, RoomRequest::Leave);
    g.until(Duration::from_secs(5), "A home, the lobby listed", |g| g.room(0).is_none() && g.rooms_listed(0).len() == 1);
    assert!(player::<Health>(&mut g.server, A).is_none(), "A still practicing");
    let key = g.rooms_listed(0)[0].key;
    g.ask(0, RoomRequest::Join(key));
    g.until(Duration::from_secs(5), "A in the lobby", |g| g.room(1).is_some_and(|r| r.members.len() == 2));
    g.run(Duration::from_millis(300));
    assert!(player::<Health>(&mut g.server, A).is_none(), "A went in without asking");
    g.ask(0, RoomRequest::EnterArena);
    g.until(Duration::from_secs(5), "A fighting", |g| player::<Health>(&mut g.server, A).is_some());

    // Back to the lobby: out of the arena, still in the room.
    g.ask(0, RoomRequest::LeaveArena);
    g.until(Duration::from_secs(5), "A out of the arena", |g| player::<Health>(&mut g.server, A).is_none());
    g.until(Duration::from_secs(5), "B sees A in the lobby", |g| {
        g.room(1).is_some_and(|r| r.members.len() == 2 && r.members.iter().filter(|m| m.in_arena).count() == 1)
    });
    assert!(g.room(0).is_some(), "A left the room");
}
