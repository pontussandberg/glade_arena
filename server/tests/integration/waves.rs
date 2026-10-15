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

/// A Jav's javelin goes through a line of monsters, hitting each; and the hit knocks them back.
#[test]
fn javelins_pierce_monsters_and_knock_them_back() {
    let mut p = Party::new(5865, "javelinist", false);
    p.ask(RoomRequest::QuickJoin("Waves 3".into()));
    p.until(Duration::from_secs(10), "in the run", |p| p.satchel().is_some_and(|s| s.gold > 0));
    p.ask(RoomRequest::Ready(true));
    p.until(Duration::from_secs(40), "two monsters about", |p| {
        let world = p.server.world_mut();
        world.query::<(&Team, &Health)>().iter(world).filter(|(t, h)| t.0 == ZOMBIES && h.alive()).count() >= 2
    });
    // Out in the open east of the camp, two monsters in a row ahead of us, held still.
    let me = Vec2::new(10.5, 0.5);
    let world = p.server.world_mut();
    let monsters: Vec<Entity> = world.query::<(Entity, &Team, &Health)>().iter(world).filter(|(_, t, h)| t.0 == ZOMBIES && h.alive()).map(|(e, ..)| e).take(2).collect();
    let spots = [me + Vec2::new(3.0, 0.0), me + Vec2::new(5.0, 0.0)];
    let hold = |p: &mut Party| {
        let world = p.server.world_mut();
        for (monster, spot) in monsters.iter().zip(spots) {
            let mut m = world.entity_mut(*monster);
            m.get_mut::<Pos>().unwrap().0 = spot;
            // Frozen to the spot: they don't walk, but a knockback still moves them.
            m.get_mut::<Chilled>().unwrap().rooted = Span { from: 0, until: u32::MAX };
        }
        place(&mut p.server, 1, me);
    };
    for _ in 0..30 {
        hold(&mut p);
        p.update();
    }
    let health = |p: &mut Party| [monsters[0], monsters[1]].map(|m| p.server.world().get::<Health>(m).map_or(0, |h| h.0));
    let before = health(&mut p);
    set_input(&mut p.client, PlayerInput { aim: Vec2::X, fire: true, ..default() });
    p.until(Duration::from_secs(3), "both hit by one javelin", |p| {
        let now = health(p);
        now[0] < before[0] && now[1] < before[1]
    });
    // The knockback takes a few ticks to slide out.
    for _ in 0..20 {
        p.update();
    }
    let pushed = p.server.world().get::<Pos>(monsters[0]).unwrap().0;
    assert!(pushed.x > spots[0].x + 0.1, "the first wasn't knocked back: {pushed}");
}

/// A Frost Mage's frostbolt shatters on the monster it hits: the one beside it is hit too.
#[test]
fn frostbolts_shatter_over_those_round_their_mark() {
    let mut p = Party::new(5866, "frost_mage", false);
    p.ask(RoomRequest::QuickJoin("Waves 3".into()));
    p.until(Duration::from_secs(10), "in the run", |p| p.satchel().is_some_and(|s| s.gold > 0));
    p.ask(RoomRequest::Ready(true));
    p.until(Duration::from_secs(40), "two monsters about", |p| {
        let world = p.server.world_mut();
        world.query::<(&Team, &Health)>().iter(world).filter(|(t, h)| t.0 == ZOMBIES && h.alive()).count() >= 2
    });
    // Out in the open east of the camp: one monster straight ahead, one beside it, out of the
    // bolt's way; both held still.
    let me = Vec2::new(10.5, 0.5);
    let world = p.server.world_mut();
    let monsters: Vec<Entity> = world.query::<(Entity, &Team, &Health)>().iter(world).filter(|(_, t, h)| t.0 == ZOMBIES && h.alive()).map(|(e, ..)| e).take(2).collect();
    let spots = [me + Vec2::new(4.0, 0.0), me + Vec2::new(4.3, 1.3)];
    let hold = |p: &mut Party| {
        let world = p.server.world_mut();
        for (monster, spot) in monsters.iter().zip(spots) {
            let mut m = world.entity_mut(*monster);
            m.get_mut::<Pos>().unwrap().0 = spot;
            m.get_mut::<Chilled>().unwrap().rooted = Span { from: 0, until: u32::MAX };
        }
        place(&mut p.server, 1, me);
    };
    for _ in 0..30 {
        hold(&mut p);
        p.update();
    }
    let health = |p: &mut Party| [monsters[0], monsters[1]].map(|m| p.server.world().get::<Health>(m).map_or(0, |h| h.0));
    let before = health(&mut p);
    set_input(&mut p.client, PlayerInput { aim: Vec2::X, fire: true, ..default() });
    p.until(Duration::from_secs(3), "both hit by one frostbolt", |p| {
        hold(p);
        let now = health(p);
        now[0] < before[0] && now[1] < before[1]
    });
}

/// Open ground near `p` on `map`.
fn open_near(map: MapId, p: Vec2) -> Vec2 {
    let map = map.map();
    map.nearest_walkable(arena_shared::map::Map::tile_of(p), 3).map(arena_shared::map::Map::center).expect("open ground")
}

/// How many effects of `kind` the server has up now.
fn effects(server: &mut App, kind: FxKind) -> usize {
    let world = server.world_mut();
    world.query::<&Fx>().iter(world).filter(|fx| fx.kind == kind).count()
}

/// Whether the server has the merchant out.
fn merchant_out(server: &mut App) -> bool {
    let world = server.world_mut();
    world.query_filtered::<(), With<Shopkeeper>>().iter(world).count() == 1
}

/// The notice the client was last shown.
fn notice(p: &Party) -> Option<String> {
    p.client.world().resource::<arena_client::rooms::Notice>().0.clone()
}

/// Practice on a PvP map: the merchant is there, giving items away. And Deadeye's primed javelin
/// counts as the ability dealing damage: it sets off an Echo Stone, where a plain javelin doesn't.
#[test]
fn practice_shops_free_and_deadeye_sets_off_ability_items() {
    let mut p = Party::new(5867, "javelinist", false);
    p.ask(RoomRequest::Practice(MapId::Keep));
    p.until(Duration::from_secs(10), "practicing", |p| p.room().is_some_and(|r| r.practice) && p.satchel().is_some());
    p.until(Duration::from_secs(5), "the merchant out", |p| merchant_out(&mut p.server));
    let shop = MapId::Keep.map().shop.expect("the keep has a merchant");
    let me = open_near(MapId::Keep, shop + Vec2::new(-1.0, 0.0));
    for _ in 0..5 {
        place(&mut p.server, 1, me);
        p.update();
    }
    p.ask(RoomRequest::Buy(Item::EchoStone));
    p.until(Duration::from_secs(3), "given the echo stone", |p| p.satchel().is_some_and(|s| s.level(Item::EchoStone) == 1));
    assert_eq!(p.satchel().unwrap().gold, 0, "it wasn't free");

    // A dummy straight ahead.
    let target = open_near(MapId::Keep, me + Vec2::new(4.0, 0.0));
    assert!(MapId::Keep.map().shot_clear(me, target), "no clear shot at the dummy");
    p.ask(RoomRequest::PlaceDummy { at: target, walks: false });
    let dummy = |p: &mut Party| {
        let world = p.server.world_mut();
        world.query_filtered::<(Entity, &Pos, &Health), Without<Shopkeeper>>().iter(world).find(|(_, pos, _)| pos.0.distance(target) < 0.5).map(|(e, _, h)| (e, h.0))
    };
    p.until(Duration::from_secs(3), "the dummy up", |p| dummy(p).is_some());
    let aim = target - me;

    // A plain javelin: a hit, no echo.
    let full = dummy(&mut p).unwrap().1;
    set_input(&mut p.client, PlayerInput { aim, fire: true, ..default() });
    let mut echoed = false;
    p.until(Duration::from_secs(3), "a plain javelin hits", |p| {
        place(&mut p.server, 1, me);
        echoed |= effects(&mut p.server, FxKind::Warn(FxColor::Spirit)) > 0;
        dummy(p).is_some_and(|(_, h)| h < full)
    });
    set_input(&mut p.client, PlayerInput { aim, ..default() });
    for _ in 0..40 {
        place(&mut p.server, 1, me);
        echoed |= effects(&mut p.server, FxKind::Warn(FxColor::Spirit)) > 0;
        p.update();
    }
    assert!(!echoed, "a plain javelin set off the echo stone");

    // Deadeye, then the javelin it primes: an echo where it hit.
    edit_input(&mut p.client, |i| i.order_cast(arena_shared::abilities::AbilitySlot(0), aim));
    p.until(Duration::from_secs(2), "primed", |p| attack_state(&mut p.server, 1).primed.is_some());
    // Clicked until it goes (a click while the last throw recovers is dropped).
    p.until(Duration::from_secs(4), "the primed javelin echoes", |p| {
        edit_input(&mut p.client, |i| (i.aim, i.fire) = (aim, true));
        place(&mut p.server, 1, me);
        effects(&mut p.server, FxKind::Warn(FxColor::Spirit)) > 0
    });
}

/// A PvP match has the merchant too, but no gold to start with: only kills earn it, each its
/// killer's own.
#[test]
fn pvp_has_a_merchant_and_kills_pay_their_killer() {
    let mut p = Party::new(5868, "revenant", false);
    p.ask(RoomRequest::Create { name: "Brawl".into(), mode: Mode::Ffa, map: MapId::Keep });
    p.until(Duration::from_secs(5), "in the room", |p| p.room().is_some());
    p.ask(RoomRequest::Start);
    p.until(Duration::from_secs(10), "fighting", |p| player::<Health>(&mut p.server, 1).is_some() && p.satchel().is_some());
    assert_eq!(p.satchel().unwrap().gold, 0, "a PvP match started with gold");
    p.until(Duration::from_secs(5), "the merchant out", |p| merchant_out(&mut p.server));

    // The shop's open, but there's nothing to pay with.
    let shop = MapId::Keep.map().shop.unwrap();
    let me = open_near(MapId::Keep, shop + Vec2::new(-1.0, 0.0));
    for _ in 0..5 {
        place(&mut p.server, 1, me);
        p.update();
    }
    p.ask(RoomRequest::Buy(Item::Bloodfang));
    p.until(Duration::from_secs(3), "turned away for want of gold", |p| notice(p).as_deref() == Some("Not enough gold"));

    // A zombie, brought next to us on its last legs, killed: its gold is ours.
    p.until(Duration::from_secs(60), "a zombie about", |p| {
        let world = p.server.world_mut();
        world.query::<(&Team, &Health)>().iter(world).any(|(t, h)| t.0 == ZOMBIES && h.alive())
    });
    let world = p.server.world_mut();
    let zombie = world.query::<(Entity, &Team, &Health)>().iter(world).find(|(_, t, h)| t.0 == ZOMBIES && h.alive()).map(|(e, ..)| e).unwrap();
    // Out where the glaive's lane comes down (it starts a way ahead of the revenant).
    let map = MapId::Keep.map();
    let spot = [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y]
        .map(|dir| me + dir * 2.4)
        .into_iter()
        .find(|&spot| map.walkable_at(spot) && map.line_walkable(me, spot))
        .expect("open ground round us for the zombie");
    let hold = |p: &mut Party| {
        let world = p.server.world_mut();
        let mut z = world.entity_mut(zombie);
        z.get_mut::<Pos>().unwrap().0 = spot;
        z.get_mut::<Chilled>().unwrap().rooted = Span { from: 0, until: u32::MAX };
        if let Some(mut health) = z.get_mut::<Health>() {
            health.0 = health.0.min(1);
        }
        place(&mut p.server, 1, me);
    };
    for _ in 0..10 {
        hold(&mut p);
        p.update();
    }
    p.until(Duration::from_secs(5), "paid for the zombie", |p| {
        edit_input(&mut p.client, |i| (i.aim, i.fire) = (spot - me, true));
        hold(p);
        p.satchel().is_some_and(|s| s.gold == arena_shared::items::PVP_ZOMBIE_GOLD)
    });
}

/// Only an auto-attack's own hits set off on-hit items: a Ricochet Charm's bounce doesn't count
/// toward the Thunder Crown's every-so-many hits.
#[test]
fn bounces_dont_set_off_on_hit_items() {
    let mut p = Party::new(5869, "javelinist", false);
    p.ask(RoomRequest::Practice(MapId::Keep));
    p.until(Duration::from_secs(10), "practicing", |p| p.room().is_some_and(|r| r.practice) && p.satchel().is_some());
    let shop = MapId::Keep.map().shop.unwrap();
    let me = open_near(MapId::Keep, shop + Vec2::new(-1.0, 0.0));
    for _ in 0..5 {
        place(&mut p.server, 1, me);
        p.update();
    }
    for item in [Item::RicochetCharm, Item::ThunderCrown] {
        p.ask(RoomRequest::Buy(item));
        p.until(Duration::from_secs(3), "given the item", |p| p.satchel().is_some_and(|s| s.level(item) == 1));
    }
    let (every, ..) = arena_shared::items::thunder_crown(1);

    // A dummy straight ahead, and another off to its side for the javelin to bounce on to.
    let map = MapId::Keep.map();
    let first = open_near(MapId::Keep, me + Vec2::new(4.0, 0.0));
    let second = [Vec2::Y, Vec2::NEG_Y].map(|side| first + side * 2.5).into_iter().find(|&at| map.walkable_at(at)).expect("room for a second dummy");
    for at in [first, second] {
        p.ask(RoomRequest::PlaceDummy { at, walks: false });
    }
    let hits = |p: &mut Party, at: Vec2| {
        let world = p.server.world_mut();
        world.query_filtered::<(&Pos, &RecentHits), Without<Shopkeeper>>().iter(world).find(|(pos, _)| pos.0.distance(at) < 0.5).map(|(_, hits)| hits.seq())
    };
    p.until(Duration::from_secs(3), "both dummies up", |p| hits(p, first).is_some() && hits(p, second).is_some());
    let (first_from, second_from) = (hits(&mut p, first).unwrap(), hits(&mut p, second).unwrap());

    // Javelins until the second dummy's taken more bounces than the crown needs hits.
    let mut bolts = std::collections::HashSet::new();
    let watch = |p: &mut Party, bolts: &mut std::collections::HashSet<Entity>| {
        place(&mut p.server, 1, me);
        let world = p.server.world_mut();
        bolts.extend(world.query::<(Entity, &Fx)>().iter(world).filter(|(_, fx)| fx.kind == FxKind::Bolt).map(|(e, _)| e));
    };
    p.until(Duration::from_secs(20), "bounces enough", |p| {
        edit_input(&mut p.client, |i| (i.aim, i.fire) = (first - me, true));
        watch(p, &mut bolts);
        hits(p, second).unwrap() - second_from > every
    });
    // What's still flying lands.
    for _ in 0..90 {
        watch(&mut p, &mut bolts);
        p.update();
    }
    let bounces = hits(&mut p, second).unwrap() - second_from;
    // The first dummy's hits: the javelins, and the bolts that came down on it.
    let javelins = hits(&mut p, first).unwrap() - first_from - bolts.len() as u32;
    assert!(bounces > every, "the javelins didn't bounce: {bounces}");
    assert_eq!(bolts.len() as u32, javelins / every, "{javelins} javelins and {bounces} bounces called down {} bolts", bolts.len());
}

/// Practice's slow zombies: three side by side where asked, coming for us (not for a target
/// dummy nearer them) at half a zombie's pace.
#[test]
fn practice_places_three_slow_zombies() {
    let mut p = Party::new(5877, "revenant", false);
    p.ask(RoomRequest::Practice(MapId::Keep));
    p.until(Duration::from_secs(10), "practicing", |p| p.room().is_some_and(|r| r.practice) && player::<Health>(&mut p.server, 1).is_some());
    let shop = MapId::Keep.map().shop.unwrap();
    let me = open_near(MapId::Keep, shop + Vec2::new(-1.0, 0.0));
    let at = open_near(MapId::Keep, me + Vec2::new(0.0, -6.0));
    let dummy = open_near(MapId::Keep, at + Vec2::new(0.0, -2.0));
    for _ in 0..5 {
        place(&mut p.server, 1, me);
        p.update();
    }
    p.ask(RoomRequest::PlaceDummy { at: dummy, walks: false });
    p.ask(RoomRequest::PlaceZombies { at });
    let zombies = |p: &mut Party| {
        let world = p.server.world_mut();
        world.query::<(&Team, &Pos)>().iter(world).filter(|(t, _)| t.0 == ZOMBIES).map(|(_, pos)| pos.0).collect::<Vec<_>>()
    };
    p.until(Duration::from_secs(3), "three zombies", |p| zombies(p).len() == 3);
    // And we see them.
    p.until(Duration::from_secs(3), "three zombies in sight", |p| {
        let world = p.client.world_mut();
        world.query::<&Team>().iter(world).filter(|t| t.0 == ZOMBIES).count() == 3
    });
    let placed = zombies(&mut p);
    assert!(placed.iter().all(|z| z.distance(at) < 1.5), "not where asked: {placed:?}");
    let apart = placed.iter().flat_map(|a| placed.iter().map(move |b| a.distance(*b))).filter(|d| *d > 0.0).fold(f32::MAX, f32::min);
    assert!(apart > 0.5, "on top of each other: {placed:?}");

    // A second and a half's walk: toward us, at half a zombie's speed.
    let start = Instant::now();
    p.until(Duration::from_secs(3), "a while gone by", |p| {
        place(&mut p.server, 1, me);
        start.elapsed() >= Duration::from_millis(1500)
    });
    let moved = zombies(&mut p);
    let walked = placed.iter().zip(&moved).map(|(a, b)| a.distance(*b)).fold(0.0f32, f32::max) / start.elapsed().as_secs_f32();
    let speed = class_id("zombie").def().move_speed;
    assert!(walked > speed * 0.25, "they didn't walk: {walked} m/s");
    assert!(walked < speed * 0.7, "not slow: {walked} m/s, a zombie walks {speed} m/s");
    let nearer = |zs: &[Vec2]| zs.iter().map(|z| z.distance(me)).sum::<f32>();
    assert!(nearer(&moved) < nearer(&placed), "they didn't come for us: {placed:?} -> {moved:?}");
}

/// A Void Acolyte blinks away from whoever comes close: gone from beside them, back well away.
#[test]
fn acolytes_blink_away_when_closed_in_on() {
    let mut p = Party::new(5878, "revenant", false);
    p.ask(RoomRequest::QuickJoin("Waves 13".into()));
    p.until(Duration::from_secs(10), "in the run", |p| p.satchel().is_some_and(|s| s.gold > 0));
    p.ask(RoomRequest::Ready(true));
    let acolyte = class_id("void_acolyte");
    let find = |p: &mut Party| {
        let world = p.server.world_mut();
        world.query::<(Entity, &ClassId, &Pos, &Health)>().iter(world).find(|(_, c, _, h)| **c == acolyte && h.alive()).map(|(e, _, pos, _)| (e, pos.0))
    };
    p.until(Duration::from_secs(60), "an acolyte about", |p| find(p).is_some());
    let (entity, _) = find(&mut p).unwrap();
    let pos = |p: &mut Party| p.server.world().get::<Pos>(entity).map(|pos| pos.0);
    // Up close (and kept from harm), until it's gone from there: a jump from where it was last
    // seen, whichever frame it comes in.
    let mut jumped = None;
    let mut last: Option<Vec2> = None;
    p.until(Duration::from_secs(8), "the acolyte blinks", |p| {
        let Some(now) = pos(p) else { return false };
        if let Some(was) = last.filter(|was| was.distance(now) > 4.0) {
            jumped = Some((was, now));
            return true;
        }
        last = Some(now);
        let map = MapId::Wilds.map();
        let beside = [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y].map(|d| now + d * 1.5).into_iter().find(|s| map.walkable_at(*s)).unwrap_or(now);
        place(&mut p.server, 1, beside);
        if let Some(mut health) = p.server.world_mut().query::<(&PlayerId, &mut Health)>().iter_mut(p.server.world_mut()).find(|(id, _)| id.0 == peer(1)).map(|(_, h)| h) {
            health.0 = health.0.max(100);
        }
        false
    });
    let (before, after) = jumped.unwrap();
    assert!(after.distance(before) > 4.0, "blinked only {}", after.distance(before));
    assert!(effects(&mut p.server, FxKind::Blink) > 0, "no blink to be seen");
}

/// Leaving practice leaves nothing of it behind: no dummies, zombies, shots or effects of the
/// old room, and practising again starts afresh.
#[test]
fn leaving_practice_leaves_nothing_behind() {
    let mut p = Party::new(5879, "javelinist", false);
    let leftovers = |p: &mut Party, key: RoomKey| {
        let world = p.server.world_mut();
        world.query::<(Entity, &arena_server::rooms::InRoom, Option<&Name>)>().iter(world).filter(|(_, r, _)| r.key == key).map(|(e, _, n)| format!("{e} {n:?}")).collect::<Vec<_>>()
    };
    for round in 0..2 {
        p.ask(RoomRequest::Practice(MapId::Keep));
        p.until(Duration::from_secs(10), "practicing", |p| p.room().is_some_and(|r| r.practice) && player::<Health>(&mut p.server, 1).is_some());
        let key = p.room().unwrap().key;
        let shop = MapId::Keep.map().shop.unwrap();
        let me = open_near(MapId::Keep, shop + Vec2::new(-1.0, 0.0));
        let near = open_near(MapId::Keep, me + Vec2::new(0.0, -5.0));
        place(&mut p.server, 1, me);
        p.ask(RoomRequest::PlaceDummy { at: open_near(MapId::Keep, me + Vec2::new(4.0, 0.0)), walks: true });
        p.ask(RoomRequest::PlaceZombies { at: near });
        p.until(Duration::from_secs(3), "zombies out", |p| {
            let world = p.server.world_mut();
            world.query::<&Team>().iter(world).filter(|t| t.0 == ZOMBIES).count() == 3
        });
        // Mid-fight: a javelin in the air.
        for _ in 0..40 {
            edit_input(&mut p.client, |i| (i.aim, i.fire) = (near - me, true));
            p.update();
        }
        p.ask(RoomRequest::Leave);
        p.until(Duration::from_secs(5), "out of practice", |p| p.room().is_none());
        for _ in 0..10 {
            p.update();
        }
        let left = leftovers(&mut p, key);
        assert!(left.is_empty(), "round {round}: left behind in the old room: {left:?}");
        // Nor does the client go on seeing any of it: no fighters, shots, pickups or effects out of
        // a room.
        let seen = |p: &mut Party| {
            let world = p.client.world_mut();
            world
                .query_filtered::<(Entity, Option<&Name>), Or<(With<PlayerId>, With<Projectile>, With<Pickup>, With<Fx>)>>()
                .iter(world)
                .map(|(e, name)| format!("{e} {name:?}"))
                .collect::<Vec<_>>()
        };
        let end = Instant::now() + Duration::from_secs(2);
        while !seen(&mut p).is_empty() && Instant::now() < end {
            p.update();
        }
        let seen = seen(&mut p);
        assert!(seen.is_empty(), "round {round}: the client still sees the old room's things: {seen:?}");
    }
}

/// A fury stone quickens attacks, never walking: its rush (`Fury`) and no haste, no frenzy.
#[test]
fn a_fury_stone_quickens_attacks_only() {
    let mut p = Party::new(5884, "revenant", false);
    p.ask(RoomRequest::QuickJoin("Waves".into()));
    p.until(Duration::from_secs(10), "in the run", |p| player::<Health>(&mut p.server, 1).is_some());
    let stone = MapId::Wilds.map().pickups.iter().find(|(_, kind)| *kind == PickupKind::Fury).map(|(at, _)| *at).expect("a fury stone in the wilds");
    p.until(Duration::from_secs(3), "the rush", |p| {
        place(&mut p.server, 1, stone);
        player::<Fury>(&mut p.server, 1).is_some_and(|f| f.0.strength > 0.0)
    });
    let now = p.server.world().resource::<lightyear::prelude::LocalTimeline>().tick().0;
    let fury = player::<Fury>(&mut p.server, 1).unwrap();
    assert!(fury.0.at(now as f32) > 0.2, "barely any rush: {fury:?}");
    assert_eq!(player::<Hasted>(&mut p.server, 1).unwrap().speed_factor(now), 1.0, "it hasted walking");
    assert_eq!(player::<Frenzy>(&mut p.server, 1).unwrap().at(now as f32), 0.0, "it frenzied");
}
