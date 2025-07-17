mod app;
mod config;
mod dynamic;
mod input;
mod renderer;
mod ui;
mod virtual_keyboard;
mod backlight;
mod volume;
mod screenshot;
mod media;

use anyhow::Result;
use app::AppState;
use cairo::{Format, ImageSurface};

use crate::dynamic::DynamicManager;
use std::sync::{mpsc, Arc, Mutex, Condvar};
use std::thread;
use std::time::{Duration, Instant};
use backlight::Backlight;
use volume::Volume;
use crate::ui::Page;
use crate::media::MediaInfo;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use nix::unistd;
use nix::sys::stat;
use std::os::unix::fs::PermissionsExt;

const PIPE_PATH: &str = "/tmp/ndfr-media.pipe";

fn main() -> Result<()> {
    let pipe_path = Path::new(PIPE_PATH);
    if pipe_path.exists() {
        fs::remove_file(pipe_path)?;
    }
    unistd::mkfifo(pipe_path, stat::Mode::S_IRWXU)?;
    fs::set_permissions(pipe_path, fs::Permissions::from_mode(0o666))?;
    println!("[main] Created media pipe at {}", PIPE_PATH);

    let keyboard_features = input::find_keyboard_features()?;
    let has_physical_esc = keyboard_features.has_physical_esc;
    let keyboard_device = keyboard_features.device;

    let mut drm = renderer::DrmBackend::new()?;
    let (physical_width, physical_height) = drm.get_dimensions();
    let (logical_width, logical_height) = (physical_height, physical_width);

    let app_state = Arc::new((Mutex::new(AppState::new(logical_width, logical_height, has_physical_esc, &Vec::new())?), Condvar::new()));
    let backlight = Arc::new(Mutex::new(Backlight::new()?));
    let volume = Arc::new(Mutex::new(Volume::new()?));

    // shared state for the latest media info
    let latest_media_info = Arc::new(Mutex::new(Vec::<MediaInfo>::new()));

    let (tx, rx) = mpsc::channel();
    input::start_touch_handler(tx.clone())?;
    input::start_keyboard_handler(tx, keyboard_device)?;

    let mut uinput = virtual_keyboard::create_virtual_keyboard()?;

    let renderer_state = Arc::clone(&app_state);
    let render_thread_handle = thread::spawn(move || -> Result<()> {
        let (drm_w, drm_h) = drm.get_dimensions();
        let mut surface = ImageSurface::create(Format::ARgb32, drm_w, drm_h)?;
        let (lock, cvar) = &*renderer_state;

        const TARGET_FPS: u64 = 60;
        const FRAME_DURATION: Duration = Duration::from_millis(1000 / TARGET_FPS);

        loop {
            let frame_start = Instant::now();

            let (page_to_draw, gesture_to_draw, dynamic_content, anim_progress, is_still_animating) = {
                let mut state = lock.lock().unwrap();

                if !state.is_animating {
                    state = cvar.wait_while(state, |s| !s.needs_redraw).unwrap();
                }

                state.update_animations();

                let page = state.page.clone();
                let gesture = state.gesture.clone();
                let progress = state.get_animation_progress();
                let is_animating = state.is_animating;

                let dynamic_content = match &page {
                    Page::Default(layout) if Arc::ptr_eq(layout, &state.default_layout) => {
                        Some((state.dynamic_drawable.clone(), state.default_dynamic_area_bounds))
                    }
                    Page::MediaInfoShowing(_) | Page::MediaInfoHiding(_) => {
                        Some((state.dynamic_drawable.clone(), state.default_dynamic_area_bounds))
                    }
                    _ => None,
                };

                if !is_animating {
                    state.needs_redraw = false;
                }

                (page, gesture, dynamic_content, progress, is_animating)
            };

            renderer::draw_ui(
                &mut surface,
                &page_to_draw,
                &gesture_to_draw,
                dynamic_content.as_ref().map(|(d, r)| (d, r)),
                              anim_progress,
                              false,
            )?;
            drm.present(&mut surface)?;

            if is_still_animating {
                let elapsed = frame_start.elapsed();
                if elapsed < FRAME_DURATION {
                    thread::sleep(FRAME_DURATION - elapsed);
                }
                cvar.notify_one();
            }
        }
    });

    {
        let (lock, cvar) = &*app_state;
        let mut state = lock.lock().unwrap();
        state.brightness_value = backlight.lock().unwrap().get_brightness()?;
        state.volume_value = volume.lock().unwrap().get_volume()?;
        state.needs_redraw = true;
        cvar.notify_one();
    }

    // the only job of this media pipe listener thread is to update the shared `latest_media_info` state.
    let media_pipe_info = Arc::clone(&latest_media_info);
    thread::spawn(move || {
        println!("[media] Media pipe listener thread started.");
        // ensures that if the pipe is closed and re-created, the listener will re-attach.
        loop {
            if let Ok(file) = fs::File::open(PIPE_PATH) {
                let reader = BufReader::new(file);
                println!("[media] Opened media pipe for reading.");
                for line in reader.lines() {
                    let line = match line {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("[media] Error reading from pipe: {}. Breaking to reopen.", e);
                            break;
                        }
                    };

                    if line.is_empty() {
                        continue;
                    }

                    let mut info_lock = media_pipe_info.lock().unwrap();
                    match serde_json::from_str::<Vec<MediaInfo>>(&line) {
                        Ok(media_info) => {
                            *info_lock = media_info;
                        },
                        Err(e) => {
                            if !line.trim().is_empty() && line != "[]" {
                                eprintln!("[media] Failed to deserialize media info: '{}', line: '{}'", e, line);
                            }
                            *info_lock = Vec::new();
                        }
                    };
                }
            } else {
                thread::sleep(Duration::from_secs(1));
            }
        }
    });

    let dynamic_updater_info = Arc::clone(&latest_media_info);
    let dynamic_updater_state = Arc::clone(&app_state);
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(200));

            let (lock, cvar) = &*dynamic_updater_state;
            let mut state = lock.lock().unwrap();


            if let app::Gesture::ScrubberDrag { .. } = state.gesture {
                drop(state);
                continue;
            }

            let info_lock = dynamic_updater_info.lock().unwrap();

            let mut layout_changed = false;

            let new_drawable = if state.media_info_visible {
                if !info_lock.is_empty() {
                    DynamicManager::create_media_drawable(&info_lock, state.active_player_index, state.height)
                } else {
                    DynamicManager::create_clock_drawable()
                }
            } else {
                DynamicManager::create_clock_drawable()
            };

            if !state.media_button_visible && !info_lock.is_empty() {
                if let Ok((new_buttons, new_bounds)) = ui::create_default_layout(
                    state.width, state.height, state.has_physical_esc, &info_lock
                ) {
                    state.default_layout = Arc::new(new_buttons);
                    state.default_dynamic_area_bounds = new_bounds;
                    state.media_button_visible = true;
                    layout_changed = true;
                } else {
                    eprintln!("[main] Error: Failed to create layout with media button.");
                }
            } else if state.media_button_visible && info_lock.is_empty() {
                let (new_buttons, new_bounds) = ui::create_default_layout(
                    state.width, state.height, state.has_physical_esc, &Vec::new()
                ).unwrap();
                state.default_layout = Arc::new(new_buttons);
                state.default_dynamic_area_bounds = new_bounds;
                state.media_button_visible = false;
                layout_changed = true;
            }

            if layout_changed {
                if let Page::Default(_) = &state.page {
                    state.page = Page::Default(Arc::clone(&state.default_layout));
                }
            }

            if state.dynamic_drawable != new_drawable || layout_changed {
                state.dynamic_drawable = new_drawable;
                state.needs_redraw = true;
                cvar.notify_one();
            }
        }
    });


    let timeout_check_state = Arc::clone(&app_state);
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            let (lock, cvar) = &*timeout_check_state;
            let mut state = lock.lock().unwrap();
            if state.control_strip_expanded && !state.is_animating && state.last_input_time.elapsed() > Duration::from_secs(5) {
                println!("[app] No input for 5 seconds, closing control strip");
                state.control_strip_expanded = false;
                state.page = Page::ControlStripClosing(Arc::clone(&state.expanded_layout));
                state.animation_start = Instant::now();
                state.is_animating = true;
                state.needs_redraw = true;
                state.ignore_input = true;
                cvar.notify_one();
            }
        }
    });

    let brightness_writer_state = Arc::clone(&app_state);
    let brightness_writer_backlight = Arc::clone(&backlight);
    thread::spawn(move || -> Result<()> {
        let mut last_written_brightness = -1.0;
        loop {
            thread::sleep(Duration::from_millis(100));
            let state = brightness_writer_state.0.lock().unwrap();
            let new_brightness = state.brightness_value;
            drop(state);

            if (new_brightness - last_written_brightness).abs() > 0.01 {
                brightness_writer_backlight.lock().unwrap().set_brightness(new_brightness)?;
                last_written_brightness = new_brightness;
            }
        }
    });

    let brightness_reader_state = Arc::clone(&app_state);
    let brightness_reader_backlight = Arc::clone(&backlight);
    thread::spawn(move || -> Result<()> {
        loop {
            thread::sleep(Duration::from_secs(1));
            if let Ok(current_brightness) = brightness_reader_backlight.lock().unwrap().get_brightness() {
                let (lock, cvar) = &*brightness_reader_state;
                let mut state = lock.lock().unwrap();
                if (current_brightness - state.brightness_value).abs() > 0.01 {
                    state.brightness_value = current_brightness;
                    state.needs_redraw = true;
                    cvar.notify_one();
                }
            }
        }
    });

    let volume_writer_state = Arc::clone(&app_state);
    let volume_writer_control = Arc::clone(&volume);
    thread::spawn(move || -> Result<()> {
        let mut last_written_volume = -1.0;
        loop {
            thread::sleep(Duration::from_millis(50));
            let (lock, _cvar) = &*volume_writer_state;
            let mut state = lock.lock().unwrap();

            if matches!(state.page, Page::VolumeSlider(_)) {
                let new_volume = state.volume_value;

                if (new_volume - last_written_volume).abs() > 0.01 && state.last_volume_update.elapsed() > Duration::from_millis(100) {
                    volume_writer_control.lock().unwrap().set_volume(new_volume)?;
                    last_written_volume = new_volume;
                    state.last_volume_update = Instant::now();
                }
            }
        }
    });

    let volume_reader_state = Arc::clone(&app_state);
    let volume_reader_control = Arc::clone(&volume);
    thread::spawn(move || -> Result<()> {
        loop {
            thread::sleep(Duration::from_secs(1));
            if let Ok(current_volume) = volume_reader_control.lock().unwrap().get_volume() {
                let (lock, cvar) = &*volume_reader_state;
                let mut state = lock.lock().unwrap();
                if (current_volume - state.volume_value).abs() > 0.01 {
                    state.volume_value = current_volume;
                    state.needs_redraw = true;
                    cvar.notify_one();
                }
            }
        }
    });

    let event_handler_info = Arc::clone(&latest_media_info);
    for event in rx {
        let (lock, cvar) = &*app_state;
        let mut state = lock.lock().unwrap();
        state.handle_event(event, &mut uinput, &event_handler_info)?;
        if state.needs_redraw {
            cvar.notify_one();
        }
    }

    render_thread_handle.join().unwrap()?;
    Ok(())
}
