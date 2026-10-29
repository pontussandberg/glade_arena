//! Picking a class inside practice: a row of frames across the top of the screen, one per
//! playable class, each showing its figure's face — the head and eyes rendered to a little
//! image by a camera of its own, off in its own render layer. Clicking one re-picks on the
//! spot: `ChosenClass` changes, `send_class_choice` tells the server, and the server respawns
//! us as the pick (`handle_class_choices`).

use arena_shared::protocol::ClassId;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use crate::plain::PlainMaterial;
use crate::{ChosenClass, Me};
use crate::arena::{self, palette};
use crate::render::{GameUi, HudButton, clicked};
use crate::rooms::CurrentRoom;

/// Each playable class's face, drawn by its tiny camera in the first frames (`rest_faces`).
#[derive(Resource)]
pub(super) struct FaceIcons(Vec<(ClassId, Handle<Image>)>);

/// A face's head, eyes, lights or camera: there for the first frames only (`rest_faces`).
#[derive(Component)]
pub(super) struct FaceScene;

/// How many frames the faces are drawn before their scenes go: a face never changes, so what
/// its camera drew lasts, but its first frames may still be waiting on shaders.
const FACE_FRAMES: u32 = 60;

/// How many pixels a face image is, each way.
const FACE_PIXELS: u32 = 128;
/// The first render layer the faces hide on (one per class, counting up).
const FACE_LAYER: usize = 16;
/// How brightly the eyes glow on the face (`rig`'s look, without its bloom).
const FACE_EYE_GLOW: f32 = 6.0;

/// The row across the top of the screen, while practicing.
#[derive(Component)]
pub(super) struct PickerRow;

/// One class's frame in the row.
#[derive(Component)]
pub(super) struct PickFrame(ClassId);

/// Builds every face: for each playable class, its head and eyes alone on a render layer of
/// their own, lit by their own little sun, watched by a camera drawing into the icon's image.
pub(super) fn load_faces(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<PlainMaterial>>, mut images: ResMut<Assets<Image>>) {
    let body = materials.add(arena::matte(Color::WHITE));
    let eyes_glow = materials.add(arena::glow(palette::SOUL, FACE_EYE_GLOW));
    let mut icons = Vec::new();
    for (i, class) in ClassId::playable().enumerate() {
        let layer = RenderLayers::layer(FACE_LAYER + i);
        let size = Extent3d { width: FACE_PIXELS, height: FACE_PIXELS, depth_or_array_layers: 1 };
        let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let image = images.add(image);
        let rig = arena::fighter_rig(class.def().figure());
        commands.spawn((FaceScene, Mesh3d(meshes.add(rig.head)), MeshMaterial3d(body.clone()), Transform::IDENTITY, layer.clone()));
        commands.spawn((FaceScene, Mesh3d(meshes.add(rig.eyes)), MeshMaterial3d(eyes_glow.clone()), Transform::IDENTITY, layer.clone()));
        commands.spawn((
            FaceScene,
            PointLight { intensity: 140_000.0, range: 5.0, shadow_maps_enabled: false, ..default() },
            Transform::from_xyz(0.8, 0.6, 0.5),
            layer.clone(),
        ));
        // A softer fill from the other side, so the shadowed half of the face still reads.
        commands.spawn((
            FaceScene,
            PointLight { intensity: 50_000.0, range: 5.0, shadow_maps_enabled: false, ..default() },
            Transform::from_xyz(-0.7, 0.2, 0.6),
            layer.clone(),
        ));
        commands.spawn((
            FaceScene,
            Camera3d::default(),
            // Each face before the main view, into its own image, over nothing.
            Camera { order: -(FACE_LAYER as isize + i as isize), clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(image.clone().into()),
            // From front-right, a touch above: a three-quarter portrait of the face.
            Transform::from_xyz(0.6, 0.3, 0.26).looking_at(Vec3::new(0.0, 0.16, 0.0), Vec3::Y),
            layer,
        ));
        icons.push((class, image));
    }
    commands.insert_resource(FaceIcons(icons));
}

/// Takes the faces' scenes down once their cameras have drawn enough frames: the images they
/// drew into stay as they are, and a view per class every frame cost more than the arena itself.
pub(super) fn rest_faces(mut commands: Commands, mut frames: Local<u32>, scenes: Query<Entity, With<FaceScene>>) {
    if scenes.is_empty() {
        return;
    }
    *frames += 1;
    if *frames >= FACE_FRAMES {
        for entity in &scenes {
            commands.entity(entity).despawn();
        }
    }
}

/// The row of frames, up while practicing and gone when not.
pub(super) fn show_picker(mut commands: Commands, icons: Res<FaceIcons>, room: Res<CurrentRoom>, row: Query<Entity, With<PickerRow>>) {
    let practicing = room.0.as_ref().is_some_and(|r| r.practice);
    match (practicing, row.single()) {
        (true, Err(_)) => {
            commands
                .spawn((
                    PickerRow,
                    GameUi,
                    GlobalZIndex(4),
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(8.0),
                        left: px(0.0),
                        right: px(0.0),
                        justify_content: JustifyContent::Center,
                        column_gap: px(6.0),
                        ..default()
                    },
                ))
                .with_children(|row| {
                    for (class, icon) in &icons.0 {
                        row.spawn((
                            PickFrame(*class),
                            HudButton,
                            Interaction::default(),
                            ImageNode::new(icon.clone()),
                            Node { width: px(54.0), height: px(54.0), border: UiRect::all(px(2.0)), ..default() },
                            BorderColor::all(palette::ui::MUTED),
                            BackgroundColor(palette::ui::PANEL.with_alpha(0.75)),
                        ));
                    }
                });
        }
        (false, Ok(row)) => commands.entity(row).despawn(),
        _ => {}
    }
}

/// A frame clicked re-picks: the server swaps our fighter on the spot.
pub(super) fn pick_class(frames: Query<(Ref<Interaction>, &PickFrame)>, mut chosen: ResMut<ChosenClass>) {
    for (interaction, frame) in &frames {
        if clicked(interaction) && chosen.0 != Some(frame.0) {
            chosen.0 = Some(frame.0);
        }
    }
}

/// The frame we're playing (or just picked) holds a lit border.
pub(super) fn mark_current(me: Query<&ClassId, With<Me>>, chosen: Res<ChosenClass>, mut frames: Query<(&PickFrame, &mut BorderColor)>) {
    let current = chosen.0.or(me.single().ok().copied());
    for (frame, mut border) in &mut frames {
        let lit = current == Some(frame.0);
        border.set_if_neq(BorderColor::all(if lit { palette::GOLD } else { palette::ui::MUTED }));
    }
}
