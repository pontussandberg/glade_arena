//! Command edge cases: one server and one client, mostly with the client stepped at an
//! unhealthy rate.

use std::time::{Duration, Instant};

use arena_shared::map::Map;
use arena_shared::protocol::*;
use bevy::prelude::*;
use crate::common::*;

const CLIENT: u64 = 7;
/// A healthy client frame rate.
const SMOOTH: Option<Duration> = Some(Duration::from_micros(16_667));
/// A projectile class, so shots can be counted.
const CLASS: &str = "javelinist";

#[derive(Resource, Default)]
struct ShotsFired(usize);

fn count_shots(mut shots: ResMut<ShotsFired>, new: Query<(), Added<Projectile>>) {
    shots.0 += new.iter().count();
}

struct Pair {
    server: App,
    client: App,
}

impl Pair {
    fn new(port: u16) -> Self {
        let server = start_server(port, |app| {
            app.init_resource::<ShotsFired>();
            app.add_systems(FixedPostUpdate, count_shots);
        });
        Pair { server, client: start_client(CLIENT, port, CLASS, None) }
    }

    /// The server runs smoothly; the client only gets a frame every `client_frame`
    /// (`None`: the client is frozen, as in a backgrounded tab).
    fn run(&mut self, duration: Duration, client_frame: Option<Duration>) {
        let end = Instant::now() + duration;
        let mut next_client_frame = Instant::now();
        while Instant::now() < end {
            self.server.update();
            if let Some(frame) = client_frame
                && Instant::now() >= next_client_frame {
                    self.client.update();
                    next_client_frame += frame;
                }
            std::thread::sleep(Duration::from_millis(3));
        }
    }

    fn input(&mut self, input: PlayerInput) {
        set_input(&mut self.client, input);
    }

    /// Runs smoothly until `done`, failing after `timeout`.
    fn until(&mut self, timeout: Duration, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let end = Instant::now() + timeout;
        while !done(self) {
            assert!(Instant::now() < end, "timed out waiting for: {what}");
            self.run(Duration::from_millis(10), SMOOTH);
        }
    }

    /// A left click, leaving the destination alone, once the last attack is done (a click
    /// before then is dropped). Returns once the attack has started on the server.
    fn click_fire(&mut self) {
        self.until(Duration::from_secs(2), "the last attack is done", |p| p.attack().is_some_and(|a| a.swing.is_none()));
        edit_input(&mut self.client, |i| (i.fire, i.aim) = (true, Vec2::X));
        self.until(Duration::from_secs(1), "the attack starts", |p| p.attack().is_some_and(|a| a.swing.is_some()));
    }

    fn attack(&mut self) -> Option<AttackState> {
        player::<AttackState>(&mut self.server, CLIENT)
    }

    fn shots(&self) -> usize {
        self.server.world().resource::<ShotsFired>().0
    }

    fn server_pos(&mut self) -> Vec2 {
        server_player(&mut self.server, CLIENT).0
    }
}

/// Regression: a player once stayed stuck firing forever after letting go, at ~4 fps (found in a
/// headless browser). A click fires once, however slow the client.
#[test]
fn a_click_fires_once_at_4_fps() {
    let mut p = Pair::new(5898);
    let frame = Some(Duration::from_millis(250));
    // Until we have our player (slow under load at 4 fps), then a little longer to settle.
    let end = Instant::now() + Duration::from_secs(20);
    while !client_view(&mut p.client, CLIENT).is_some_and(|v| v.2) {
        assert!(Instant::now() < end, "never got our player at 4 fps");
        p.run(Duration::from_millis(250), frame);
    }
    p.run(Duration::from_secs(2), frame);

    // One click, so one shot.
    p.input(PlayerInput { aim: Vec2::X, fire: true, ..default() });
    p.run(Duration::from_millis(500), frame);
    p.input(PlayerInput::default());

    p.run(Duration::from_secs(2), frame);
    let after_release = p.shots();
    p.run(Duration::from_secs(3), frame);
    println!("shots after the click: {after_release}, 3s later: {}", p.shots());
    assert_eq!(after_release, 1, "one click should fire exactly once");
    assert_eq!(p.shots(), after_release, "server kept firing after the click");
}

/// Walking with the keys, a shot winding up slows us to its draw speed (`DRAW_SPEED`) from the
/// click till it goes off, held drawn or not; once it's off we're back up to full speed.
#[test]
fn a_shot_winding_up_slows_the_walk_till_it_goes_off() {
    use arena_shared::config::DRAW_SPEED;
    let mut p = Pair::new(5907);
    p.until(Duration::from_secs(5), "our player is spawned", |p| client_view(&mut p.client, CLIENT).is_some_and(|v| v.2));
    let start = p.server_pos();
    let way = [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y].into_iter().find(|&way| map().line_walkable(start, start + way * 16.0)).expect("open ground");
    let top = class_id(CLASS).def().move_speed * map().speed_factor_at(start);
    let speed = |p: &mut Pair| player::<Motion>(&mut p.server, CLIENT).map_or(0.0, |m| m.along.length());
    p.input(PlayerInput { walk: way, aim: way, ..default() });
    p.run(Duration::from_millis(800), SMOOTH);
    assert!((speed(&mut p) - top).abs() < top * 0.05, "walking at {} of {top} m/s", speed(&mut p));
    p.input(PlayerInput { walk: way, aim: way, fire: true, draw: true, ..default() });
    p.until(Duration::from_secs(1), "the attack starts", |p| p.attack().is_some_and(|a| a.swing.is_some()));
    // Through the windup and held drawn past it: slowed.
    p.run(windup(CLASS) + Duration::from_millis(400), SMOOTH);
    assert!(p.attack().is_some_and(|a| a.winding_up().is_some()), "not held drawn");
    assert!((speed(&mut p) - top * DRAW_SPEED).abs() < top * 0.05, "walking at {} of {top} m/s winding up", speed(&mut p));
    // Let go: it's off, and we speed up again.
    p.input(PlayerInput { walk: way, aim: way, ..default() });
    p.until(Duration::from_millis(500), "the shot goes off", |p| p.shots() == 1);
    p.run(Duration::from_millis(700), SMOOTH);
    assert!((speed(&mut p) - top).abs() < top * 0.05, "walking at {} of {top} m/s after the shot", speed(&mut p));
}

/// A shot's button held keeps it drawn past its windup, never going off, until it's let go; then
/// it goes off at once, the one shot.
#[test]
fn a_shot_held_drawn_goes_off_once_when_let_go() {
    let mut p = Pair::new(5906);
    p.until(Duration::from_secs(5), "our player is spawned", |p| client_view(&mut p.client, CLIENT).is_some_and(|v| v.2));
    p.input(PlayerInput { aim: Vec2::X, fire: true, draw: true, ..default() });
    p.until(Duration::from_secs(1), "the attack starts", |p| p.attack().is_some_and(|a| a.swing.is_some()));
    // Well past its windup, still held: drawn, and no shot.
    p.run(windup(CLASS) + Duration::from_millis(600), SMOOTH);
    let drawn = p.attack().and_then(|a| a.winding_up());
    assert!(drawn.is_some_and(|swing| swing.held > 0), "not held drawn: {drawn:?}");
    assert_eq!(p.shots(), 0, "went off while held");
    p.input(PlayerInput { aim: Vec2::X, ..default() });
    p.until(Duration::from_millis(500), "the shot goes off as it's let go", |p| p.shots() == 1);
    p.run(Duration::from_secs(1), SMOOTH);
    assert_eq!(p.shots(), 1, "more than the one shot");
}

/// Attacking cancels the walk: once the attack is off you stand still until you click again. An
/// order to move during the windup walks at once, the attack going off on the way (every player
/// is in action mode: `ActionMode`); one after it went off walks too, while the attack timer runs
/// on.
#[test]
fn attacking_cancels_the_walk_and_walking_off_keeps_the_windup() {
    let mut p = Pair::new(5893);
    p.until(Duration::from_secs(5), "our player is spawned", |p| client_view(&mut p.client, CLIENT).is_some_and(|v| v.2));

    // Fire while walking.
    let start = p.server_pos();
    p.input(PlayerInput { move_to: Some(Map::center(Map::tile_of(-start))), ..default() });
    p.run(Duration::from_millis(300), SMOOTH);
    p.click_fire();
    p.until(Duration::from_secs(1), "the attack goes off on the server", |p| p.attack().is_some_and(|a| a.released()));
    p.run(Duration::from_millis(100), SMOOTH); // let anything in flight land
    let after_attack = p.server_pos();
    p.run(Duration::from_millis(250), SMOOTH);
    let drift = after_attack.distance(p.server_pos());
    println!("moved {drift:.2} units in the 0.25 s after attacking");
    assert!(drift < 0.01, "kept walking to the old destination after attacking");

    // An order to move during the windup: off we go, and the attack goes off on the way.
    let (here, shots) = (p.server_pos(), p.shots());
    let goal = map().nearest_walkable(Map::tile_of(here) + IVec2::new(0, 4), 2).expect("open ground nearby");
    p.click_fire();
    edit_input(&mut p.client, |i| i.move_to = Some(Map::center(goal)));
    p.until(Duration::from_secs(2), "we walk off mid-windup", |p| here.distance(p.server_pos()) > 2.0);
    p.until(Duration::from_secs(1), "the attack goes off on the way", |p| p.shots() == shots + 1);
    let shots = p.shots();

    // One once it has gone off: the shot is out, and its timer runs on as we walk.
    p.click_fire();
    p.until(Duration::from_secs(1), "the attack goes off", |p| p.attack().is_some_and(|a| a.released()));
    let from = p.server_pos();
    edit_input(&mut p.client, |i| i.move_to = Some(Map::center(Map::tile_of(here))));
    p.until(Duration::from_secs(1), "we walk off in the backswing", |p| from.distance(p.server_pos()) > 0.3);
    assert!(p.attack().is_some_and(|a| a.swing.is_some()), "walking off cut the attack timer short");
    p.run(Duration::from_millis(100), SMOOTH);
    assert_eq!(p.shots(), shots + 1, "walking off in the backswing lost the shot");
}

/// A client that freezes mid-walk (frozen tab, hang): its fighter carries on with its last order
/// on the server, as in League, and walks all the way to the click. Once the client is back, its
/// next command takes over.
#[test]
fn frozen_client_keeps_its_last_order() {
    let mut p = Pair::new(5897);
    p.until(Duration::from_secs(5), "our player is spawned", |p| client_view(&mut p.client, CLIENT).is_some_and(|v| v.2));

    // A click a few meters off, so the walk is still going when the client freezes.
    let start = p.server_pos();
    let goal = map().nearest_walkable(Map::tile_of(start) + IVec2::new(0, 5), 2).expect("open ground nearby");
    p.input(PlayerInput { move_to: Some(Map::center(goal)), ..default() });
    p.run(Duration::from_millis(300), SMOOTH);
    assert_ne!(p.server_pos(), start, "the walk didn't start");
    // Under the 3 s it takes the server to drop a silent client.
    p.run(Duration::from_secs(2), None);
    assert_eq!(p.server_pos(), Map::center(goal), "a frozen client's fighter should finish its walk");

    // Back: a new click is walked to.
    let back = Map::center(Map::tile_of(start));
    p.input(PlayerInput { move_to: Some(back), ..default() });
    p.until(Duration::from_secs(5), "we walk back once unfrozen", |p| p.server_pos() == back);
}

/// Walking with the keys (free camera): the server walks the player that way at its speed,
/// overriding a click, the client shows the same spot, and letting go stops it.
#[test]
fn key_walking_moves_on_the_server_and_stops_on_release() {
    let mut p = Pair::new(5887);
    p.until(Duration::from_secs(5), "our player is spawned", |p| client_view(&mut p.client, CLIENT).is_some_and(|v| v.2));

    let start = p.server_pos();
    let dir = (-start).normalize();
    p.input(PlayerInput { walk: dir, move_to: Some(Map::center(Map::tile_of(start) + IVec2::new(0, -6))), ..default() });
    // A second, some of it spent getting the input there.
    p.run(Duration::from_secs(1), SMOOTH);
    let walked = p.server_pos() - start;
    println!("walked {walked} in 1 s from {start}, keys toward {dir}");
    assert!(walked.length() > 2.0, "the keys didn't walk us");
    assert!(walked.normalize().dot(dir) > 0.95, "walked {walked}, not along {dir}");

    p.input(PlayerInput::default());
    p.run(Duration::from_millis(300), SMOOTH);
    let stopped = p.server_pos();
    p.run(Duration::from_millis(300), SMOOTH);
    assert!(stopped.distance(p.server_pos()) < 0.01, "kept walking after the keys were let go");
    let shown = client_view(&mut p.client, CLIENT).unwrap().0;
    assert!(shown.distance(p.server_pos()) < 0.05, "client shows {shown}, server has {}", p.server_pos());
}
