mod app;
mod config;
mod input;
mod renderer;
mod ui;
mod virtual_keyboard;
mod backlight;
mod volume;

use anyhow::Result;
use app::AppState;
use cairo::{Format, ImageSurface};
use std::sync::{mpsc, Arc, Mutex, Condvar};
use std::thread;
use std::time::{Duration, Instant};
use backlight::Backlight;
use volume::Volume;
use crate::ui::Page;
use std::env;
use std::fs::File;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.contains(&"--screenshot".to_string()) {
        println!("[screenshot]");
        let default_layout = ui::create_default_layout(2170, 60)?;
        let page = Page::Default(default_layout);
        let (width, height) = (2170, 60);
        let mut surface = ImageSurface::create(Format::ARgb32, width, height)?;
        renderer::draw_ui(&mut surface, &page, 1.0, true)?;
        let mut file = File::create("screenshot.png")?;
        surface.write_to_png(&mut file)?;
        return Ok(());
    }

    let app_state = Arc::new((Mutex::new(AppState::new()?), Condvar::new()));
    let backlight = Arc::new(Mutex::new(Backlight::new()?));
    let volume = Arc::new(Mutex::new(Volume::new()?));

    let (tx, rx) = mpsc::channel();
    input::start_touch_handler(tx.clone())?;
    input::start_keyboard_handler(tx)?;

    let mut uinput = virtual_keyboard::create_virtual_keyboard()?;

    let renderer_state = Arc::clone(&app_state);
    let render_thread_handle = thread::spawn(move || -> Result<()> {
        let mut drm = renderer::DrmBackend::new()?;
        let (drm_w, drm_h) = drm.get_dimensions();
        let mut surface = ImageSurface::create(Format::ARgb32, drm_w, drm_h)?;
        let (lock, cvar) = &*renderer_state;

        loop {
            let mut state = lock.lock().unwrap();
            state = cvar.wait_while(state, |s| !s.needs_redraw).unwrap();

            let elapsed = state.animation_start.elapsed().as_millis() as f64;
            let duration = 150.0;
            let t = (elapsed / duration).min(1.0);

            let ease_in_out_quad = |t: f64| {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            };

            let animation_progress = match &state.page {
                Page::BrightnessSlider(_) | Page::VolumeSlider(_) | Page::ControlStripExpanding(_) => ease_in_out_quad(t),
                Page::BrightnessSliderClosing(_) | Page::VolumeSliderClosing(_) | Page::ControlStripClosing(_) => 1.0 - ease_in_out_quad(t),
                _ => 1.0,
            };

            if let Page::BrightnessSliderClosing(_) = &state.page {
                if animation_progress == 0.0 {
                    state.page = Page::Default(state.default_layout.clone());
                }
            }

            if let Page::VolumeSliderClosing(_) = &state.page {
                if animation_progress == 0.0 {
                    state.page = Page::Default(state.default_layout.clone());
                }
            }

            if let Page::ControlStripClosing(_) = &state.page {
                if animation_progress == 0.0 {
                    state.page = Page::Default(state.default_layout.clone());
                    state.ignore_input = false;
                }
            }

            if let Page::ControlStripExpanding(buttons) = &state.page {
                if animation_progress == 1.0 {
                    state.page = Page::Default(buttons.clone());
                    state.ignore_input = false;
                }
            }

            renderer::draw_ui(&mut surface, &state.page, animation_progress, false)?;
            drm.present(&mut surface)?;

            let was_animating = state.is_animating;

            let is_closing = matches!(&state.page, Page::BrightnessSliderClosing(_) | Page::VolumeSliderClosing(_) | Page::ControlStripClosing(_));
            let is_opening = matches!(&state.page, Page::BrightnessSlider(_) | Page::VolumeSlider(_) | Page::ControlStripExpanding(_));
            let animation_in_progress = (is_opening && animation_progress < 1.0) || (is_closing && animation_progress > 0.0);

            if animation_in_progress {
                state.needs_redraw = true;
                drop(state);
                thread::sleep(Duration::from_millis(16));
                cvar.notify_one();
            } else {
                state.is_animating = false;
                if was_animating {
                    state.needs_redraw = true;
                } else {
                    state.needs_redraw = false;
                }
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

    let timeout_check_state = Arc::clone(&app_state);
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            let (lock, cvar) = &*timeout_check_state;
            let mut state = lock.lock().unwrap();
            if state.control_strip_expanded && !state.is_animating && state.last_input_time.elapsed() > Duration::from_secs(5) {
                println!("[app] No input for 5 seconds, closing control strip");
                state.control_strip_expanded = false;
                state.page = Page::ControlStripClosing(state.expanded_layout.clone());
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

    for event in rx {
        let (lock, cvar) = &*app_state;
        let mut state = lock.lock().unwrap();
        state.handle_event(event, &mut uinput)?;
        if state.needs_redraw {
            cvar.notify_one();
        }
    }

    render_thread_handle.join().unwrap()?;
    Ok(())
}





