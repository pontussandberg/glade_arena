//! Fighters, projectiles, nova bursts, frost on
//! slowed and frozen fighters, the destination marker and the HUD, plus mouse and keyboard input.
//! The scene itself is in `arena/`, the camera in `camera.rs`, the server browser in
//! `browser.rs`, a room's lobby in `lobby/` and the ESC menu in `esc_menu.rs`.

use arena_shared::abilities::AbilityKind;
use arena_shared::config::*;
use arena_shared::protocol::*;
use bevy::platform::collections::HashMap;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::Me;

use crate::abilities::SeenCasts;
use crate::camera::CameraPlaced;
use crate::rooms::load_setting;
use crate::arena::{self, palette, to_world};

mod shots;
pub(crate) use shots::*;
mod effects;
use effects::*;
mod hud;
pub(crate) use hud::*;
mod input;
pub(crate) use input::*;
mod glide;
use glide::*;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(forget_body);
        app.add_observer(strike_home);
        app.add_plugins((
            arena::ArenaPlugin,
            crate::camera::CameraPlugin,
            crate::casting::CastingPlugin,
            crate::browser::BrowserPlugin,
            crate::lobby::LobbyPlugin,
            crate::map_pick::MapPickPlugin,
            crate::esc_menu::EscMenuPlugin,
            crate::feedback::FeedbackPlugin,
            crate::rig::RigPlugin,
            crate::action_bar::ActionBarPlugin,
            crate::minimap::MinimapPlugin,
            crate::pickups::PickupsPlugin,
            crate::stat_frame::StatFramePlugin,
            crate::tooltip::TooltipPlugin,
            crate::practice::PracticePlugin,
        ));
        app.add_plugins((crate::waygates::WaygatesPlugin, crate::scoreboard::ScoreboardPlugin, crate::waves::WavesPlugin, crate::fx::FxPlugin));
        app.insert_resource(ControlsOpen(load_setting(CONTROLS_KEY).as_deref() == Some("open")));
        app.add_systems(Startup, setup_scene);
        // Right after the network's updates are in, before anything this frame reads positions.
        app.init_resource::<crate::feedback::LatestClock>();
        app.add_systems(
            RunFixedMainLoop,
            (crate::feedback::advance_latest_clock, glide_me).chain().in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        );
        app.add_systems(
            Update,
            (
                (read_local_input.in_set(crate::PlayerControls).in_set(CameraPlaced), show_destination).chain(),
                (add_visuals, sync_transforms).chain().before(crate::rig::Posing),
                (fly_shots, fade_wakes, fly_impacts, crate::rig::empty_hands).chain().after(crate::rig::Posing),
                (show_novas, grow_bursts, end_fleeting, show_frost),
                align_to_world.after(crate::rig::Posing).after(fly_shots),
                toggle_range_circle,
                fit_range_circle,
                (update_status, update_key_hints, fold_controls),
                (mark_relations, light_buttons),
            ),
        );
    }
}

/// What a fighter's figure's colors are multiplied by: its class's tint (`ClassDef::tint`), white
/// for most.
#[derive(Component, Clone, Copy)]
pub(crate) struct Tint(pub(crate) Color);

impl Tint {
    pub(crate) fn of(class: ClassId) -> Tint {
        let (r, g, b) = class.def().tint();
        Tint(Color::linear_rgb(r, g, b))
    }
}

/// What an elite's twist is marked in.
pub(crate) fn affix_color(affix: Affix) -> Color {
    match affix {
        Affix::Swift => palette::HASTE,
        Affix::Vampiric => Color::srgb_u8(0xE0, 0x30, 0x40),
        Affix::Volatile => palette::TORCH_FLAME,
        Affix::Frostbound => palette::ICE,
        Affix::Splitting => Color::srgb_u8(0xB0, 0x70, 0xF0),
    }
}

/// Who a fighter is to you (kept up to date on each fighter by `mark_relations`). Only its health
/// bar and minimap dot show it; its body, shots and swings look the same whoever's
/// they are.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Relation {
    You,
    Ally,
    Enemy,
    /// A zombie: a foe to everyone, but no player.
    Monster,
}

impl Relation {
    /// Whether it's us, and else whether it's on our team.
    pub(crate) fn of(is_me: bool, allied: bool) -> Self {
        match (is_me, allied) {
            (true, _) => Self::You,
            (false, true) => Self::Ally,
            (false, false) => Self::Enemy,
        }
    }

    pub(crate) fn color(self) -> Color {
        match self {
            Self::You => palette::YOU,
            Self::Ally => palette::ALLY,
            Self::Enemy => palette::ENEMY,
            Self::Monster => palette::MONSTER,
        }
    }
}

/// Meshes shared by every player and projectile (one figure, one range circle per class),
/// and materials per player, so attacking doesn't create and upload new GPU assets.
#[derive(Resource)]
pub(crate) struct Visuals {
    fighters: HashMap<ClassId, Handle<Mesh>>,
    /// A circle at the edge of a class's auto-attack range, shown around you.
    /// Each class's range circle, plain and empowered (`AttackDef::edge_reach`).
    range_circles: HashMap<ClassId, [Handle<Mesh>; 2]>,
    /// Each nova's burst (see `arena::NovaMeshes`), by its class and slot.
    novas: HashMap<(ClassId, AbilitySlot), arena::NovaMeshes<Handle<Mesh>>>,
    /// The ring an elite monster stands in, and its material by its twist.
    elite_ring: Handle<Mesh>,
    elite_rings: HashMap<Affix, Handle<StandardMaterial>>,
    /// The ice around a frozen (rooted) fighter's feet, and the frost under a slowed one's.
    ice_prison: Handle<Mesh>,
    frost_rune: Handle<Mesh>,
    /// Per class and shot (auto-attack: false, Q: true).
    projectiles: HashMap<(ClassId, Option<AbilitySlot>), arena::ShotLook<Handle<Mesh>>>,
    shot_tip: Handle<Mesh>,
    /// A piece of a thrown spear's wake.
    pub(super) wake: Handle<Mesh>,
    /// Deadeye's light on a primed javelin in flight.
    primed_aura: Handle<Mesh>,
    spear_shadow: Handle<Mesh>,
    /// A bit of a spear's impact: its flash, a splinter.
    impact: Handle<Mesh>,
    /// Per owner (`None` for looks that are the same for everyone: all but bodies, so a hit
    /// flashes just that fighter) and look.
    materials: HashMap<(Option<PeerId>, Look), Handle<StandardMaterial>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Look {
    Body,
    /// Projectiles glow, so a shot in flight is the easiest thing to spot: round bolts glow pale.
    Shot,
    /// Glowing ice in its mesh's colors: frostbolts.
    Ice,
    /// The see-through cold glow around a frostbolt and trailing it.
    Aura,
    /// Spirit: abilities glow spectral blue, whoever uses them.
    Spirit,
    /// Plain, for meshes that carry their own colors (a thrown javelin).
    Plain,
    /// The faint white wind a thrown spear leaves behind (its wake).
    Wind,
    /// The white glow on a thrown spear's point.
    Spark,
    /// Deadeye's pale light on a primed javelin.
    Deadeye,
    /// The faint dark streak on the ground under a thrown spear.
    Shadow,
    /// The faint circle at the edge of your auto-attack's range.
    Range,
    /// See-through, glowing ice: novas and frozen fighters, whoever's.
    Frost,
    /// Brighter, bluer see-through glow: the frost under a slowed fighter's feet.
    Chill,
    /// Casters' bolts: a fireball, a bolt of the void, a spark.
    Ember,
    Void,
    Storm,
}

impl Visuals {
    /// A class's body (the rest of the fighter is its rig, `rig/`).
    pub(crate) fn fighter(&self, class: ClassId) -> Handle<Mesh> {
        self.fighters[&class].clone()
    }

    pub(crate) fn material(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        owner: PeerId,
        look: Look,
    ) -> Handle<StandardMaterial> {
        self.materials
            .entry(((look == Look::Body).then_some(owner), look))
            .or_insert_with(|| {
                materials.add(match look {
                    // Fighters wear their own colors (in the mesh); who they are to you shows in
                    // their health bars.
                    Look::Body => arena::matte(Color::WHITE),
                    Look::Shot => arena::glow(palette::SILVER, 4.0),
                    Look::Ice => arena::glow(Color::WHITE, 4.0),
                    Look::Aura => arena::translucent(palette::FROST_GLOW, 0.35, 2.0),
                    Look::Spirit => arena::glow(palette::SPIRIT, 6.0),
                    Look::Plain => arena::matte(Color::WHITE),
                    Look::Wind => arena::translucent(Color::WHITE, 0.4, 1.0),
                    Look::Spark => arena::glow(Color::WHITE, 5.0),
                    Look::Deadeye => arena::glow(palette::DEADEYE, 9.0),
                    Look::Shadow => arena::translucent(palette::INK, 0.4, 0.0),
                    Look::Range => arena::translucent(Color::WHITE, 0.2, 1.0),
                    Look::Frost => arena::translucent(palette::ICE, 0.45, 1.6),
                    Look::Chill => arena::translucent(palette::FROST_GLOW, 0.75, 2.5),
                    Look::Ember => arena::glow(palette::TORCH_FLAME, 7.0),
                    Look::Void => arena::glow(Color::srgb_u8(0xA0, 0x60, 0xFF), 6.0),
                    Look::Storm => arena::glow(Color::srgb_u8(0xC8, 0xE4, 0xFF), 8.0),
                })
            })
            .clone()
    }
}

/// A fighter gone: its body's material goes from `Visuals` too (fighters no client controls, such
/// as zombies, come and go with new ids all match long). One still in use stays alive in its user.
fn forget_body(trigger: On<Remove, PlayerId>, players: Query<&PlayerId>, mut visuals: ResMut<Visuals>) {
    if let Ok(id) = players.get(trigger.entity) {
        visuals.materials.remove(&(Some(id.0), Look::Body));
    }
}

/// A child that keeps this orientation and size in the world, this high above the ground,
/// whatever its parent does (fighters turn, lean, crouch and are drawn bigger, see `rig/`): the range circle and the frost
/// stay flat on the ground.
#[derive(Component)]
pub(crate) struct WorldAligned(pub Quat, pub f32);

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        DestinationMarker,
        Mesh3d(meshes.add(Annulus::new(0.28, 0.4).mesh().resolution(12).build())),
        MeshMaterial3d(materials.add(arena::glow(palette::YOU, 2.0))),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
    ));
    commands.insert_resource(Visuals {
        fighters: ClassId::all().map(|c| (c, meshes.add(arena::fighter_mesh(c.def().figure())))).collect(),
        // Where an auto-attack stops reaching: a fighter whose body reaches over the circle can be
        // hit.
        range_circles: ClassId::all()
            .map(|c| {
                let mut circle = |empowered| {
                    let outer = c.def().attack.edge_reach(empowered);
                    meshes.add(Annulus::new(outer - RANGE_CIRCLE_WIDTH, outer).mesh().resolution(96).build())
                };
                (c, [circle(false), circle(true)])
            })
            .collect(),
        novas: ClassId::all()
            .flat_map(|c| c.def().abilities().map(move |(slot, ability)| (c, slot, ability.kind)))
            .filter_map(|(c, slot, kind)| {
                let AbilityKind::Nova(nova) = kind else { return None };
                let arena::NovaMeshes { disc, ring, shard, eruption } = arena::nova_meshes(nova.radius);
                let mut add = |mesh| meshes.add(mesh);
                Some(((c, slot), arena::NovaMeshes { disc: add(disc), ring: add(ring), shard: add(shard), eruption: add(eruption) }))
            })
            .collect(),
        elite_ring: meshes.add(Annulus::new(0.62, 0.8).mesh().resolution(24).build()),
        elite_rings: HashMap::default(),
        ice_prison: meshes.add(arena::ice_prison_mesh()),
        frost_rune: meshes.add(arena::frost_rune_mesh()),
        projectiles: ClassId::all()
            .flat_map(|c| std::iter::once(None).chain(c.def().abilities().map(|(slot, _)| Some(slot))).map(move |ability| (c, ability)))
            .filter_map(|(c, ability)| {
                let arena::ShotLook { mesh, glow, thrown, aura, spin } = arena::shot_look(c.def(), c.def().shot(ability)?, ability.is_some());
                let look = arena::ShotLook { mesh: meshes.add(mesh), glow, thrown, aura: aura.map(|a| meshes.add(a)), spin };
                Some(((c, ability), look))
            })
            .collect(),
        shot_tip: meshes.add(arena::shot_tip_mesh()),
        wake: meshes.add(arena::wake_mesh()),
        primed_aura: meshes.add(arena::primed_spear_aura()),
        spear_shadow: meshes.add(arena::spear_shadow_mesh()),
        impact: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
        materials: HashMap::default(),
    });
    commands
        .spawn(corner_panel(false, 8.0))
        .with_children(|corner| {
            let home = Node { padding: UiRect::all(px(5.0)), border: UiRect::all(px(1.0)), ..default() };
            corner
                .spawn((crate::esc_menu::HomeButton, HudButton, button_fill(palette::ui::PANEL), home, BorderColor::all(palette::ui::MUTED.with_alpha(0.5))))
                .with_children(|button| pixel_art(button, HOUSE, 2.0, palette::ui::TEXT));
            corner.spawn((Status, ui_text("connecting...", 12.0, palette::ui::TEXT)));
            let sign = corner.spawn(fold_sign(false, HUD_FOLD_SIGN, palette::ui::MUTED)).id();
            let label = corner.spawn(ui_text("Controls", 12.0, palette::ui::MUTED)).id();
            corner.spawn((ControlsHeader { sign }, fold_header())).add_children(&[sign, label]);
            // Two columns: the keys, as wide as the widest, then what they do, all left-aligned.
            corner.spawn((
                KeyHints,
                Node {
                    display: Display::Grid,
                    grid_template_columns: vec![GridTrack::auto(), GridTrack::auto()],
                    column_gap: px(10.0),
                    row_gap: px(4.0),
                    align_items: AlignItems::Center,
                    justify_items: JustifyItems::Start,
                    ..default()
                },
            ));
        });
}

/// Gives players and projectiles a mesh once their position is known: each class's own figure
/// and shot, the shot pointing the way it flies.
fn add_visuals(
    mut commands: Commands,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    players: Query<(Entity, &PlayerId, &ClassId, &Pos, Option<&Elite>), Without<Mesh3d>>,
    projectiles: Query<(Entity, &Projectile, &Pos), Without<Mesh3d>>,
    me: Query<&PlayerId, With<Me>>,
) {
    // Wait until we know which player is ours, so colors are right from the first frame.
    let Ok(me) = me.single() else { return };
    for (entity, id, class, pos, elite) in &players {
        let is_me = id.0 == me.0;
        let material = visuals.material(&mut materials, id.0, Look::Body);
        // A borrowed figure in its own colors.
        let tint = Tint::of(*class);
        if let Some(mut m) = materials.get_mut(&material) {
            m.base_color = tint.0;
        }
        // An elite stands in a ring of its twist's color.
        if let Some(elite) = elite {
            let ring = visuals.elite_rings.entry(elite.0).or_insert_with(|| materials.add(arena::translucent(affix_color(elite.0), 0.55, 3.0))).clone();
            commands.entity(entity).with_child((
                Mesh3d(visuals.elite_ring.clone()),
                MeshMaterial3d(ring),
                WorldAligned(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2), 0.05),
                NotShadowCaster,
            ));
        }
        let prison = commands
            .spawn((
                Mesh3d(visuals.ice_prison.clone()),
                MeshMaterial3d(visuals.material(&mut materials, id.0, Look::Frost)),
                WorldAligned(Quat::IDENTITY, 0.0),
                Visibility::Hidden,
            ))
            .id();
        let rune = commands
            .spawn((
                Mesh3d(visuals.frost_rune.clone()),
                MeshMaterial3d(visuals.material(&mut materials, id.0, Look::Chill)),
                WorldAligned(Quat::IDENTITY, 0.04),
                Visibility::Hidden,
            ))
            .id();
        commands
            .entity(entity)
            .insert((
                Mesh3d(visuals.fighters[class].clone()),
                MeshMaterial3d(material),
                Transform::from_translation(to_world(pos.0, 0.0)),
                SeenCasts::default(),
                Frost { prison, rune, shown: (false, false) },
                tint,
            ))
            .add_children(&[prison, rune]);
        if is_me && let Some([circle, _]) = visuals.range_circles.get(class).cloned() {
            commands.entity(entity).with_child((
                Mesh3d(circle),
                MeshMaterial3d(visuals.material(&mut materials, id.0, Look::Range)),
                WorldAligned(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2), 0.03),
                RangeCircle,
                Visibility::Hidden,
            ));
        }
    }
    for (entity, projectile, pos) in &projectiles {
        let Some(arena::ShotLook { mesh, glow, thrown, aura, spin }) =
            visuals.projectiles.get(&(projectile.class, projectile.ability)).cloned()
        else {
            continue;
        };
        // Never in its owner's colors: a shot looks the same whoever throws it.
        let look = match glow {
            arena::ShotGlow::Weapon => Look::Plain,
            arena::ShotGlow::Spirit => Look::Spirit,
            arena::ShotGlow::Ice => Look::Ice,
            arena::ShotGlow::Pale => Look::Shot,
            arena::ShotGlow::Ember => Look::Ember,
            arena::ShotGlow::Void => Look::Void,
            arena::ShotGlow::Storm => Look::Storm,
        };
        let body = visuals.material(&mut materials, projectile.owner, look);
        let mut shot = commands.entity(entity);
        shot.insert((
            Mesh3d(mesh),
            MeshMaterial3d(body),
            Transform::from_translation(to_world(pos.0, shot_height(thrown)))
                .with_rotation(Quat::from_rotation_y(projectile.dir.to_angle())),
        ));
        if spin != 0.0 {
            shot.insert(Spin(spin));
        }
        // A real weapon, in its own colors, gets a white glow on its point.
        if glow == arena::ShotGlow::Weapon {
            let spark = visuals.material(&mut materials, projectile.owner, Look::Spark);
            shot.with_child((Mesh3d(visuals.shot_tip.clone()), MeshMaterial3d(spark), NotShadowCaster));
        }
        if let Some(aura) = aura {
            let material = visuals.material(&mut materials, projectile.owner, Look::Aura);
            shot.with_child((Mesh3d(aura), MeshMaterial3d(material)));
        }
        // A primed javelin (Deadeye's sure crit) flies lit as it was held, dripping its light.
        if thrown && projectile.crit {
            let light = visuals.material(&mut materials, projectile.owner, Look::Deadeye);
            shot.insert(crate::rig::ShotDrips::default()).with_child((Mesh3d(visuals.primed_aura.clone()), MeshMaterial3d(light), NotShadowCaster));
        }
        if thrown {
            let wake = visuals.material(&mut materials, projectile.owner, Look::Wind);
            shot.insert(Thrown { launch: None, wake, wake_at: None });
            // Its only shadow is the one straight under it, on its line: where it really is (the
            // sun's would fall meters off to the side), so none of it casts one, its point's spark
            // and its wind neither.
            let shadow = visuals.material(&mut materials, projectile.owner, Look::Shadow);
            let flat = Quat::from_rotation_y(projectile.dir.to_angle());
            shot.insert(NotShadowCaster).with_child((Mesh3d(visuals.spear_shadow.clone()), MeshMaterial3d(shadow), WorldAligned(flat, 0.03), NotShadowCaster));
        }
    }
}

fn align_to_world(
    parents: Query<&Transform, Without<WorldAligned>>,
    mut aligned: Query<(&WorldAligned, &ChildOf, &mut Transform)>,
) {
    for (aligned, child_of, mut transform) in &mut aligned {
        let Ok(parent) = parents.get(child_of.parent()) else { continue };
        // Undo the parent's turn, lean, crouch and size.
        let unturn = parent.rotation.inverse();
        let unscale = parent.scale.recip();
        let wanted = Transform::from_translation(unscale * (unturn * Vec3::Y * (aligned.1 - parent.translation.y)))
            .with_rotation(unturn * aligned.0)
            .with_scale(unscale);
        transform.set_if_neq(wanted);
    }
}

/// Who each fighter is to us, as `Relation`: ours, on our team, or not.
fn mark_relations(
    mut commands: Commands,
    me: Query<&Team, (With<Me>, With<PlayerId>)>,
    players: Query<(Entity, Has<Me>, Option<&Team>, Option<&Relation>), With<PlayerId>>,
) {
    let my_team = me.single().ok().copied();
    for (player, is_me, team, relation) in &players {
        let allied = my_team.zip(team).is_some_and(|(mine, theirs)| mine.allied(*theirs));
        let monster = team.is_some_and(|team| team.0 == arena_shared::rooms::ZOMBIES);
        let now = if monster { Relation::Monster } else { Relation::of(is_me, allied) };
        if relation != Some(&now) {
            commands.entity(player).insert(now);
        }
    }
}
