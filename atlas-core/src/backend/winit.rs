use std::sync::Arc;
use std::time::Duration;

use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, ButtonState, InputEvent, KeyboardKeyEvent,
            PointerButtonEvent,
        },
        renderer::{
            element::solid::SolidColorRenderElement,
            gles::GlesRenderer,
            Color32F,
        },
        winit::{self, WinitEvent},
    },
    desktop::{Window, space::render_output},
    input::{
        keyboard::FilterResult,
        pointer::{ButtonEvent, MotionEvent, PointerHandle},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{EventLoop, Interest, Mode as LoopMode, PostAction, generic::Generic},
        wayland_server::Display,
        winit::event_loop::pump_events::PumpStatus,
    },
    utils::{Logical, Physical, Point, Transform},
    wayland::{
        socket::ListeningSocketSource,
    },
};
use tracing::{error, info, warn};

use atlas_space::{GlobalSpace, Size as GsSize, Point as GsPoint, Viewport};

use crate::state::{AtlasState, ClientState, GrabState, GrabKind};

const PAN_SPEED: f64 = 50.0;
const MOD_KEY_EVDEV: i32 = 125;
const KEY_ENTER: i32 = 28;
const KEY_Q: i32 = 16;
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const MIN_WIN_SIZE: f64 = 100.0;

/// ── Spatial helpers ──────────────────────────────────────────────

fn sync_space_with_viewport(
    state: &mut AtlasState,
    screen_size: smithay::utils::Size<i32, smithay::utils::Physical>,
) {
    let gs_size = GsSize::new(screen_size.w as f64, screen_size.h as f64);
    let visible = state
        .global_space
        .windows_visible_in(&state.viewport, gs_size);

    let mut mapped: Vec<u64> = Vec::with_capacity(visible.len());

    for (gid, screen_pos, _) in &visible {
        let sp = smithay::utils::Point::from((
            screen_pos.x as i32,
            screen_pos.y as i32,
        ));
        if let Some(window) = state.windows.get(gid) {
            if state.space.element_geometry(window).is_some() {
                state.space.relocate_element(window, sp);
            } else {
                state.space.map_element(window.clone(), sp, false);
            }
            mapped.push(*gid);
        }
    }

    let known: Vec<u64> = state.windows.keys().copied().collect();
    for gid in &known {
        if !mapped.contains(gid) {
            if let Some(window) = state.windows.get(gid) {
                if state.space.element_geometry(window).is_some() {
                    state.space.unmap_elem(window);
                }
            }
        }
    }
}

fn screen_to_canvas(state: &AtlasState, phys: Point<f64, Physical>) -> GsPoint {
    state
        .global_space
        .screen_to_canvas(GsPoint::new(phys.x, phys.y), &state.viewport)
}

fn surface_from_window(window: &Window) -> Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface> {
    window.toplevel().map(|t| t.wl_surface().clone())
}

fn find_gid(state: &AtlasState, window: &Window) -> Option<u64> {
    state.windows.iter().find_map(|(gid, w)| {
        if std::ptr::eq(w as *const Window, window as *const Window) {
            Some(*gid)
        } else {
            None
        }
    })
}

/// ── Keyboard ─────────────────────────────────────────────────────

/// Spawn a terminal emulator process.
fn spawn_terminal() {
    // Try a few common terminal emulators
    for cmd in &["fish", "gnome-terminal", "alacritty", "kitty", "foot", "weston-terminal"] {
        if std::process::Command::new("which")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            let _ = std::process::Command::new(cmd)
                .spawn()
                .map(|_| info!("Spawned terminal: {}", cmd));
            return;
        }
    }
    // As last resort try the WAYLAND_DISPLAY env variable
    let _ = std::process::Command::new("xterm")
        .env("WAYLAND_DISPLAY", std::env::var("WAYLAND_DISPLAY").unwrap_or_default())
        .spawn()
        .map(|_| info!("Spawned xterm"));
}

fn handle_keyboard_event(
    state: &mut AtlasState,
    event: &impl KeyboardKeyEvent<smithay::backend::winit::WinitInput>,
    keyboard: &smithay::input::keyboard::KeyboardHandle<AtlasState>,
) {
    let pressed = event.state() == smithay::backend::input::KeyState::Pressed;
    let evdev = event.key_code().raw() as i32 - 8;

    if evdev == MOD_KEY_EVDEV {
        state.mod_pressed = pressed;
    }

    // ── Keybinds (only on press) ──────────────────────────────────
    if pressed && state.mod_pressed {
        match evdev {
            KEY_ENTER => {
                spawn_terminal();
                return;
            }
            KEY_Q => {
                if let Some(gid) = state.focused_gid {
                    if let Some(window) = state.windows.get(&gid) {
                        if let Some(toplevel) = window.toplevel() {
                            toplevel.send_close();
                            info!("Sent close to window {}", gid);
                        }
                    }
                }
                return;
            }
            _ => {}
        }
    }

    // ── Camera pan (plain arrow keys, no Mod) ─────────────────────
    if pressed && !state.mod_pressed {
        match evdev {
            103 => state.viewport.y -= PAN_SPEED / state.viewport.zoom,
            108 => state.viewport.y += PAN_SPEED / state.viewport.zoom,
            105 => state.viewport.x -= PAN_SPEED / state.viewport.zoom,
            106 => state.viewport.x += PAN_SPEED / state.viewport.zoom,
            _ => {}
        }
    }

    // ── Mod+Arrow Nudge (move focused window) ─────────────────────
    if pressed && state.mod_pressed {
        if let Some(focused_gid) = state.focused_gid {
            let step = 20.0;
            let pos = state.global_space.window_position(focused_gid);
            if let Some(p) = pos {
                let new_pos = match evdev {
                    103 => GsPoint::new(p.x, p.y - step),
                    108 => GsPoint::new(p.x, p.y + step),
                    105 => GsPoint::new(p.x - step, p.y),
                    106 => GsPoint::new(p.x + step, p.y),
                    _ => p,
                };
                state.global_space.move_window(focused_gid, new_pos);
            }
        }
    }

    keyboard.input::<(), _>(
        state,
        event.key_code(),
        event.state(),
        0.into(),
        0,
        |_, _, _| FilterResult::Forward,
    );
}

/// ── Pointer motion ───────────────────────────────────────────────

fn handle_motion_event(
    state: &mut AtlasState,
    pointer: &PointerHandle<AtlasState>,
    phys: Point<f64, Physical>,
    logical: Point<f64, Logical>,
) {
    state.pointer_location = phys;

    // ── Active grab (move or resize) ─────────────────────────────
    let grab_update = state.grab.as_ref().map(|g| {
        let current_canvas = screen_to_canvas(state, phys);
        let delta_x = current_canvas.x - g.grab_anchor.x;
        let delta_y = current_canvas.y - g.grab_anchor.y;

        match g.kind {
            GrabKind::Move => (
                g.window_id,
                g.initial_window_pos.x + delta_x,
                g.initial_window_pos.y + delta_y,
                None, // no size change
            ),
            GrabKind::Resize => {
                let new_w = (g.initial_window_size.width + delta_x).max(MIN_WIN_SIZE);
                let new_h = (g.initial_window_size.height + delta_y).max(MIN_WIN_SIZE);
                (g.window_id, g.initial_window_pos.x, g.initial_window_pos.y, Some((new_w, new_h)))
            }
        }
    });

    if let Some((gid, nx, ny, resize_opt)) = grab_update {
        state.global_space.move_window(gid, GsPoint::new(nx, ny));

        if let Some((nw, nh)) = resize_opt {
            let ns = GsSize::new(nw, nh);
            state.global_space.resize_window(gid, ns);
            // Send configure to the client so it reallocates buffers
            if let Some(window) = state.windows.get(&gid) {
                if let Some(toplevel) = window.toplevel() {
                    toplevel.with_pending_state(|state| {
                        state.size = Some(smithay::utils::Size::from((nw as i32, nh as i32)));
                    });
                    toplevel.send_configure();
                }
            }
        }
    }

    // ── Focus surface (for pointer events) ────────────────────────
    let surface = state
        .space
        .element_under(logical)
        .and_then(|(w, _)| surface_from_window(w));

    let focus = surface.map(|s| (s, logical));

    state.serial_counter += 1;
    let serial = state.serial_counter;

    pointer.motion(
        state,
        focus,
        &MotionEvent {
            location: logical,
            serial: serial.into(),
            time: 0,
        },
    );
    pointer.frame(state);
}

/// ── Pointer button ───────────────────────────────────────────────

fn handle_button_event(
    state: &mut AtlasState,
    pointer: &PointerHandle<AtlasState>,
    keyboard: &smithay::input::keyboard::KeyboardHandle<AtlasState>,
    is_press: bool,
    code: u32,
    btn_state: ButtonState,
    serial: u32,
) {
    let is_left = code == BTN_LEFT;
    let is_right = code == BTN_RIGHT;

    // ── Press ─────────────────────────────────────────────────────
    if is_press && (is_left || is_right) && state.mod_pressed {
        let logical = Point::<f64, Logical>::from((
            state.pointer_location.x,
            state.pointer_location.y,
        ));

        let hit = state.space.element_under(logical).map(|(w, _)| {
            let surface = surface_from_window(w);
            let gid = find_gid(state, w);
            (gid, surface)
        });

        if let Some((window_id, _)) = hit {
            if let Some(gid) = window_id {
                let canvas = screen_to_canvas(state, state.pointer_location);
                let win_pos = state
                    .global_space
                    .window_position(gid)
                    .unwrap_or(GsPoint::new(0.0, 0.0));
                let win_size = state
                    .global_space
                    .window_entry(gid)
                    .map(|e| e.size)
                    .unwrap_or(GsSize::new(800.0, 600.0));

                state.grab = Some(GrabState {
                    kind: if is_left { GrabKind::Move } else { GrabKind::Resize },
                    window_id: gid,
                    initial_window_pos: win_pos,
                    grab_anchor: canvas,
                    initial_window_size: win_size,
                });
            }
        }
    }

    // ── Release ───────────────────────────────────────────────────
    if !is_press && (is_left || is_right) {
        let grab_end = state.grab.as_ref().map(|g| {
            let current_canvas = screen_to_canvas(state, state.pointer_location);
            let delta_x = current_canvas.x - g.grab_anchor.x;
            let delta_y = current_canvas.y - g.grab_anchor.y;

            match g.kind {
                GrabKind::Move => (
                    g.window_id,
                    g.initial_window_pos.x + delta_x,
                    g.initial_window_pos.y + delta_y,
                    None,
                ),
                GrabKind::Resize => {
                    let new_w = (g.initial_window_size.width + delta_x).max(MIN_WIN_SIZE);
                    let new_h = (g.initial_window_size.height + delta_y).max(MIN_WIN_SIZE);
                    (
                        g.window_id,
                        g.initial_window_pos.x,
                        g.initial_window_pos.y,
                        Some((new_w, new_h)),
                    )
                }
            }
        });

        if let Some((gid, nx, ny, resize_opt)) = grab_end {
            state.global_space.move_window(gid, GsPoint::new(nx, ny));
            if let Some((nw, nh)) = resize_opt {
                let ns = GsSize::new(nw, nh);
                state.global_space.resize_window(gid, ns);
                if let Some(window) = state.windows.get(&gid) {
                    if let Some(toplevel) = window.toplevel() {
                        toplevel.with_pending_state(|state| {
                            state.size = Some(smithay::utils::Size::from((nw as i32, nh as i32)));
                        });
                        toplevel.send_configure();
                    }
                }
            }
        }
        state.grab = None;
    }

    // ── Click-to-focus on plain left click (no Mod) ───────────────
    if is_press && is_left && !state.mod_pressed {
        let logical = Point::<f64, Logical>::from((
            state.pointer_location.x,
            state.pointer_location.y,
        ));

        let hit = state.space.element_under(logical).map(|(w, _)| {
            let surface = surface_from_window(w);
            let gid = find_gid(state, w);
            (gid, surface)
        });

        if let Some((window_id, surface_opt)) = hit {
            state.focused_gid = window_id;
            if let Some(surface) = surface_opt {
                keyboard.set_focus(state, Some(surface), 0.into());
            }
            if let Some(gid) = window_id {
                if let Some(w) = state.windows.get(&gid) {
                    state.space.raise_element(w, true);
                }
                state.global_space.raise_window(gid);
            }
        }
    }

    // ── Forward to client ─────────────────────────────────────────
    let button_event = ButtonEvent {
        serial: serial.into(),
        time: 0,
        button: code,
        state: btn_state,
    };
    pointer.button(state, &button_event);
    pointer.frame(state);
}

/// ── Main loop ────────────────────────────────────────────────────

pub fn run_winit() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<AtlasState> = EventLoop::try_new()?;
    let display: Display<AtlasState> = Display::new()?;
    let dh = display.handle();

    let compositor_state = smithay::wayland::compositor::CompositorState::new::<AtlasState>(&dh);
    let shm_state = smithay::wayland::shm::ShmState::new::<AtlasState>(&dh, vec![]);
    let mut seat_state = smithay::input::SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "atlas");
    let data_device_state =
        smithay::wayland::selection::data_device::DataDeviceState::new::<AtlasState>(&dh);

    let (mut backend, mut winit) = winit::init::<GlesRenderer>()?;

    let size = backend.window_size();
    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Atlas".into(),
            model: "Winit".into(),
            serial_number: "Unknown".into(),
        },
    );
    let mode = Mode { size, refresh: 60_000 };
    output.create_global::<AtlasState>(&dh);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    let damage_tracker =
        smithay::backend::renderer::damage::OutputDamageTracker::from_output(&output);

    let xdg_shell_state = smithay::wayland::shell::xdg::XdgShellState::new::<AtlasState>(&dh);

    let socket_source = ListeningSocketSource::new_auto()?;
    let socket_name = socket_source.socket_name().to_string_lossy().into_owned();
    info!(name = socket_name, "Listening on wayland socket");

    event_loop.handle().insert_source(
        socket_source,
        |client_stream, _, data: &mut AtlasState| {
            if let Err(err) = data
                .display_handle
                .insert_client(client_stream, Arc::new(ClientState::default()))
            {
                warn!("Error adding wayland client: {}", err);
            }
        },
    )?;

    event_loop.handle().insert_source(
        Generic::new(display, Interest::READ, LoopMode::Level),
        |_, display, data| {
            unsafe {
                display.get_mut().dispatch_clients(data).unwrap();
            }
            Ok(PostAction::Continue)
        },
    )?;

    let pointer: PointerHandle<AtlasState> = seat.add_pointer();

    let mut space = smithay::desktop::Space::default();
    space.map_output(&output, (0, 0));

    let global_space = GlobalSpace::new();
    let viewport = Viewport::new("winit");

    let mut state = AtlasState {
        display_handle: dh.clone(),
        compositor_state,
        xdg_shell_state,
        shm_state,
        seat_state,
        data_device_state,
        seat,
        output,
        socket_name,
        space,
        damage_tracker,
        global_space,
        viewport,
        windows: std::collections::HashMap::new(),
        running: true,
        grab: None,
        pointer_location: Point::from((0.0f64, 0.0f64)),
        mod_pressed: false,
        serial_counter: 0,
        focused_gid: None,
        cursor_status: smithay::input::pointer::CursorImageStatus::default_named(),
    };

    info!("Initialization completed, starting the main loop.");

    let keyboard = state
        .seat
        .add_keyboard(smithay::input::keyboard::XkbConfig::default(), 200, 200)
        .map_err(|e| format!("Failed to initialize keyboard: {}", e))?;

    let start_time = std::time::Instant::now();
    let mut full_redraw: u8 = 4;

    while state.running {
        let status = winit.dispatch_new_events(|event| match event {
            WinitEvent::Resized { size, .. } => {
                let mode = Mode { size, refresh: 60_000 };
                state.output.change_current_state(Some(mode), None, None, None);
                state.output.set_preferred(mode);
            }
            WinitEvent::Input(event) => match event {
                InputEvent::Keyboard { event } => {
                    handle_keyboard_event(&mut state, &event, &keyboard);
                }

                InputEvent::PointerMotionAbsolute { event } => {
                    let phys = Point::<f64, Physical>::from((event.x(), event.y()));
                    let logical = Point::<f64, Logical>::from((phys.x, phys.y));
                    handle_motion_event(&mut state, &pointer, phys, logical);
                }

                InputEvent::PointerButton { event } => {
                    state.serial_counter += 1;
                    let serial = state.serial_counter;
                    handle_button_event(
                        &mut state,
                        &pointer,
                        &keyboard,
                        event.state() == ButtonState::Pressed,
                        event.button_code(),
                        event.state(),
                        serial,
                    );
                }

                _ => {}
            },
            _ => (),
        });

        match status {
            PumpStatus::Continue => (),
            PumpStatus::Exit(_) => {
                state.running = false;
                break;
            }
        }

        // ── Spatial sync ──────────────────────────────────────────
        let screen_size = backend.window_size();
        sync_space_with_viewport(&mut state, screen_size);
        state.space.refresh();

        let age = if full_redraw > 0 {
            full_redraw -= 1;
            0
        } else {
            backend.buffer_age().unwrap_or(0)
        };
        let clear_color = Color32F::new(0.1, 0.0, 0.0, 1.0);

        // ── Render ────────────────────────────────────────────────
        let (damage_to_submit, frame_time) = {
            let (renderer, mut framebuffer) = match backend.bind() {
                Ok(ret) => ret,
                Err(err) => {
                    error!("Failed to bind renderer: {}", err);
                    break;
                }
            };

            let custom_elements: &[SolidColorRenderElement] = &[];

            let result = render_output(
                &state.output,
                renderer,
                &mut framebuffer,
                1.0,
                age,
                std::slice::from_ref(&state.space),
                custom_elements,
                &mut state.damage_tracker,
                clear_color,
            );

            let frame_time = start_time.elapsed();
            match result {
                Ok(render_output_result) => (render_output_result.damage.cloned(), frame_time),
                Err(err) => {
                    warn!("Rendering error: {:?}", err);
                    (None, frame_time)
                }
            }
        };

        if let Some(ref damage) = damage_to_submit {
            if !damage.is_empty() {
                if let Err(err) = backend.submit(Some(damage)) {
                    warn!("Failed to submit buffer: {}", err);
                }
            }
        }

        let output_for_frames = state.output.clone();
        for window in state.space.elements() {
            if state.space.outputs_for_element(window).contains(&output_for_frames) {
                window.send_frame(
                    &output_for_frames,
                    frame_time,
                    None,
                    |_, _| Some(output_for_frames.clone()),
                );
            }
        }

        let result = event_loop.dispatch(Some(Duration::from_millis(1)), &mut state);
        if result.is_err() {
            error!("Event loop dispatch error");
            state.running = false;
            break;
        }
        if let Err(err) = state.display_handle.flush_clients() {
            warn!("Failed to flush clients: {:?}", err);
        }
    }

    Ok(())
}
