//! Keep the client ticking while its browser tab is hidden.
//!
//! Browsers stop `requestAnimationFrame` in hidden tabs. Lightyear's `WebKeepalivePlugin`
//! (part of `ClientPlugins` on wasm) works around that by waking Bevy's event loop from a Web
//! Worker, but Bevy 0.19's winit runner ignores those wake-ups unless a frame was drawn since the
//! last update or every window is marked invisible. So while the tab is hidden we mark the window
//! invisible: winit ignores `visible` on the web, and only the runner's update gate reads it.
//! Cameras are switched off while hidden, so the background tab only simulates and networks.
//!
//! Without this, switching tabs froze the client and the server dropped it after 3 seconds.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

pub struct HiddenTabPlugin;

impl Plugin for HiddenTabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, listen_for_visibility_changes);
    }
}

fn listen_for_visibility_changes(world: &mut World) {
    let document = web_sys::window().and_then(|w| w.document()).expect("no document");
    // Same approach as lightyear's keepalive: the World outlives the page, and JS callbacks never
    // run while `app.update()` is borrowing it, because the browser is single-threaded.
    let world_ptr = world as *mut World;
    let on_change = Closure::<dyn FnMut()>::new({
        let document = document.clone();
        move || {
            let visible = !document.hidden();
            let world = unsafe { &mut *world_ptr };
            let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
            if let Ok(mut window) = windows.single_mut(world) {
                window.visible = visible;
            }
            // Nobody sees a hidden tab: keep simulating and networking, but skip rendering. Tabs
            // of the same site can share a renderer thread, so this also keeps other tabs smooth.
            let mut cameras = world.query::<&mut Camera>();
            for mut camera in cameras.iter_mut(world) {
                camera.is_active = visible;
            }
        }
    });
    document
        .add_event_listener_with_callback("visibilitychange", on_change.as_ref().unchecked_ref())
        .expect("failed to listen for visibilitychange");
    on_change.forget();
}
