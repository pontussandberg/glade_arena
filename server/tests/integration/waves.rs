//! The waves mode end to end: a party's run opening at the shop, buying from the merchant, a wave
//! coming once everyone's ready, the bot fighting it off, and the next one coming with the shop
//! open in between. And a long run (ignored: `cargo test -p arena-server --test integration
//! long_run -- --ignored --nocapture`) that plays wave after wave with a bot that shops, printing
//! how each goes, for tuning.

use std::time::{Duration, Instant};

use arena_client::bot::BotPlugin;
use arena_client::rooms::{CurrentRoom, Me as Guest};
use arena_shared::items::{Item, Satchel};
use arena_shared::map::MapId;
use arena_shared::protocol::*;
use arena_shared::rooms::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::common::*;

/// A server and one guest, stepped together.
struct Party {
    server: App,
    client: App,
}

impl Party {
    fn new(port: u16, class: &str, bot: bool) -> Party {
        let server = start_server(port, |_| {});
        let client = start_guest_with(1, port, class, |client| {
            if bot {
                client.add_plugins(BotPlugin);
            }
        });
        let mut party = Party { server, client };
        party.until(Duration::from_secs(15), "welcomed", |p| p.client.world().get_resource::<Guest>().is_some());
        party
    }

    fn update(&mut self) {
        self.server.update();
        self.client.update();
        std::thread::sleep(Duration::from_millis(2));
    }

    fn until(&mut self, timeout: Duration, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let end = Instant::now() + timeout;
        while !done(self) {
            assert!(Instant::now() < end, "timed out waiting for: {what}");
            self.update();
        }
    }

    fn ask(&mut self, request: RoomRequest) {
        let world = self.client.world_mut();
        let mut sender = world.query_filtered::<&mut MessageSender<RoomRequest>, With<Client>>();
        sender.single_mut(world).expect("a client").send::<Reliable>(request);
    }

    fn room(&self) -> Option<RoomView> {
        self.client.world().resource::<CurrentRoom>().0.clone()
    }

    fn run(&self) -> Option<RunView> {
        self.room().and_then(|room| room.run)
    }

    fn satchel(&mut self) -> Option<Satchel> {
        player::<Satchel>(&mut self.client, 1)
    }

    /// Monsters the client sees, alive.
    fn monsters(&mut self) -> usize {
        let world = self.client.world_mut();
        let mut q = world.query::<(&Team, &Health)>();
        q.iter(world).filter(|(team, health)| team.0 == ZOMBIES && health.alive()).count()
    }

    /// Into a fresh waves room, started, in the arena.
    fn start_run(&mut self) {
        self.ask(RoomRequest::Create { name: "Waves".into(), mode: Mode::Waves, map: MapId::Crossing });
        self.until(Duration::from_secs(5), "in the room", |p| p.room().is_some());
        let room = self.room().unwrap();
        assert_eq!((room.mode, room.map), (Mode::Waves, MapId::Wilds), "the waves are on the wilds, whatever map was asked for");
        self.ask(RoomRequest::Start);
        self.until(Duration::from_secs(10), "in the arena with gold", |p| p.satchel().is_some_and(|s| s.gold > 0));
    }

    /// Our fighter, by the merchant.
    fn to_the_shop(&mut self) {
        let shop = MapId::Wilds.map().shop.unwrap();
        for _ in 0..5 {
            place(&mut self.server, 1, shop + Vec2::new(0.0, -1.5));
            self.update();
        }
    }
}

#[test]
fn a_run_opens_at_the_shop_and_waves_come_and_go() {
    // The bot stands still with no monsters about, and fights them once they come.
    let mut p = Party::new(5860, "javelinist", true);
    p.start_run();
    let run = p.run().expect("a run");
    assert_eq!(run.wave, 1);
    assert!(matches!(run.phase, Phase::Shop { .. }), "it opens with the shop: {:?}", run.phase);
    // Shopping: only by the merchant.
    p.ask(RoomRequest::Buy(Item::StormCoil));
    p.until(Duration::from_secs(3), "refused away from the merchant", |p| {
        p.client.world().resource::<arena_client::rooms::Notice>().0.as_deref() == Some("Walk up to the merchant to trade")
    });
    p.to_the_shop();
    let gold = p.satchel().unwrap().gold;
    p.ask(RoomRequest::Buy(Item::StormCoil));
    p.until(Duration::from_secs(3), "bought", |p| p.satchel().is_some_and(|s| s.level(Item::StormCoil) == 1));
    assert_eq!(p.satchel().unwrap().gold, gold - Item::StormCoil.cost(0));
    // Another class's item, never.
    p.ask(RoomRequest::Buy(Item::GravecallerRift));
    p.until(Duration::from_secs(3), "refused another class's", |p| {
        p.client.world().resource::<arena_client::rooms::Notice>().0.as_deref() == Some("Not for your class")
    });
    // Ready: the wave comes, and monsters with it.
    p.ask(RoomRequest::Ready(true));
    p.until(Duration::from_secs(8), "the wave comes", |p| p.run().is_some_and(|r| r.phase == Phase::Fight));
    p.until(Duration::from_secs(15), "monsters come out", |p| p.monsters() > 0);
    // The bot fights it off (and the shop opens again, the next wave coming).
    p.until(Duration::from_secs(120), "the first wave cleared", |p| p.run().is_some_and(|r| r.wave == 2 && matches!(r.phase, Phase::Shop { .. })));
    assert!(p.satchel().unwrap().gold > gold - Item::StormCoil.cost(0), "clearing a wave pays");
}

/// A bot plays on and on, buying what it can between waves, until the party falls (or wave 30):
/// how long each wave takes, the bot's health and gold after it, and what it carries.
#[test]
#[ignore]
fn long_run() {
    let class = std::env::var("ARENA_CLASS").unwrap_or("revenant".into());
    // `ARENA_PORT` to run several at once.
    let port = std::env::var("ARENA_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5861);
    let mut p = Party::new(port, &class, true);
    // `ARENA_FROM_WAVE` starts the run at a later wave.
    match std::env::var("ARENA_FROM_WAVE").ok().and_then(|w| w.parse::<u32>().ok()) {
        Some(wave) => {
            p.ask(RoomRequest::QuickJoin(format!("Waves {wave}")));
            p.until(Duration::from_secs(10), "in the run", |p| p.satchel().is_some_and(|s| s.gold > 0));
        }
        None => p.start_run(),
    }
    let mut reported = Instant::now();
    let wishlist = [Item::StormCoil, Item::EmberHeart, Item::Bloodfang, Item::WispLantern, Item::ThunderCrown, Item::BrambleMantle];
    let mut wave_started = Instant::now();
    let mut last_wave = 0;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(60 * 30) {
        p.update();
        let Some(run) = p.run() else { continue };
        if let Phase::Over { reached } = run.phase {
            println!("fell on wave {reached} after {:.0}s", started.elapsed().as_secs_f32());
            return;
        }
        if run.wave >= 30 {
            println!("reached wave 30 after {:.0}s", started.elapsed().as_secs_f32());
            return;
        }
        // A wave dragging on: who's left, and where.
        if run.phase == Phase::Fight && wave_started.elapsed() > Duration::from_secs(60) && reported.elapsed() > Duration::from_secs(30) {
            reported = Instant::now();
            let me = server_player(&mut p.server, 1).0;
            let world = p.server.world_mut();
            let mut q = world.query::<(&ClassId, &Team, &Pos, &Health)>();
            let left: Vec<String> = q
                .iter(world)
                .filter(|(_, team, _, health)| team.0 == ZOMBIES && health.alive())
                .map(|(class, _, pos, health)| format!("{} at {:.0},{:.0} ({})", class.def().name, pos.x, pos.y, health.0))
                .collect();
            println!("wave {} dragging ({:.0}s), me at {:.0},{:.0}: {left:?}", run.wave, wave_started.elapsed().as_secs_f32(), me.x, me.y);
        }
        if run.wave != last_wave && matches!(run.phase, Phase::Shop { .. }) {
            let health = player::<Health>(&mut p.client, 1).map_or(0, |h| h.0);
            let satchel = p.satchel().unwrap_or_default();
            println!(
                "wave {:2} cleared in {:5.1}s, health {health}, gold {}, items {:?}",
                last_wave,
                wave_started.elapsed().as_secs_f32(),
                satchel.gold,
                satchel.items().collect::<Vec<_>>()
            );
            last_wave = run.wave;
            // Shop: whatever's affordable on the wishlist, raising what's carried.
            p.to_the_shop();
            for _ in 0..3 {
                for item in wishlist {
                    p.ask(RoomRequest::Buy(item));
                }
                for _ in 0..20 {
                    p.update();
                }
            }
            p.ask(RoomRequest::Ready(true));
            p.until(Duration::from_secs(10), "the wave", |p| p.run().is_some_and(|r| r.phase == Phase::Fight || matches!(r.phase, Phase::Over { .. })));
            wave_started = Instant::now();
        }
    }
}

/// Two bots in one run, late enough that someone goes down: the other picks them up (stands over
/// them), and they're back in the same wave. Ignored: it plays real waves until that happens.
#[test]
#[ignore]
fn coop_picks_the_fallen_up() {
    let port = 5864;
    let mut server = start_server(port, |_| {});
    let mut clients: Vec<App> = (1..=2).map(|id| start_guest_with(id, port, "revenant", |c| { c.add_plugins(BotPlugin); })).collect();
    let step = |server: &mut App, clients: &mut Vec<App>| {
        server.update();
        for c in clients.iter_mut() {
            c.update();
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let end = Instant::now() + Duration::from_secs(30);
    while clients.iter().any(|c| c.world().get_resource::<Guest>().is_none()) {
        assert!(Instant::now() < end, "welcomed");
        step(&mut server, &mut clients);
    }
    let join = |clients: &mut Vec<App>| {
        for c in clients.iter_mut() {
            let world = c.world_mut();
            let mut sender = world.query_filtered::<&mut MessageSender<RoomRequest>, With<Client>>();
            sender.single_mut(world).unwrap().send::<Reliable>(RoomRequest::QuickJoin("Waves 8".into()));
        }
    };
    join(&mut clients);
    // Someone goes down, and is back up while the same wave's on (a lost run is played again).
    let mut downed: Vec<(Entity, u32)> = Vec::new();
    let end = Instant::now() + Duration::from_secs(900);
    loop {
        assert!(Instant::now() < end, "no one was picked up");
        step(&mut server, &mut clients);
        let run = clients[0].world().resource::<CurrentRoom>().0.as_ref().and_then(|r| r.run.clone().filter(|_| r.started));
        let Some(run) = run else {
            if clients[0].world().resource::<CurrentRoom>().0.as_ref().is_some_and(|r| !r.started) {
                join(&mut clients);
            }
            continue;
        };
        if run.phase != Phase::Fight {
            continue;
        }
        let world = server.world_mut();
        let now_down: Vec<Entity> = world.query_filtered::<Entity, With<Downed>>().iter(world).collect();
        for entity in &now_down {
            if !downed.iter().any(|(e, _)| e == entity) {
                downed.push((*entity, run.wave));
            }
        }
        let mut alive = world.query::<&Health>();
        let picked_up = downed.iter().any(|&(e, wave)| wave == run.wave && !now_down.contains(&e) && alive.get(world, e).is_ok_and(|h| h.alive()));
        if picked_up {
            break;
        }
        downed.retain(|&(_, wave)| wave == run.wave);
    }
}
