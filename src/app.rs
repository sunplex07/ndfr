use crate::ui::{Page, Button, create_default_layout, create_fn_layout, create_brightness_slider_layout, create_volume_slider_layout, create_expanded_layout};
use crate::input::{InputEvent, TouchEvent};
use crate::virtual_keyboard;
use crate::screenshot;
use crate::dynamic::{DynamicDrawable, DynamicManager, Rect};
use crate::media::MediaInfo;
use anyhow::Result;
use evdev::Key as EvdevKey;
use input_linux::Key as UinputKey;
use input_linux::uinput::UInputHandle;
use std::env;
use std::fs::File;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
pub enum Gesture {
    Idle,
    ButtonDown { button_index: usize },
    SliderDrag,
    ScrubberDrag { player_id: String },
}

pub struct AppState {
    pub page: Page,
    pub brightness_value: f64,
    pub volume_value: f64,
    pub gesture: Gesture,
    pub animation_start: Instant,
    pub needs_redraw: bool,
    pub is_animating: bool,
    pub last_input_time: Instant,
    pub last_volume_update: Instant,
    pub default_layout: Arc<Vec<Button>>,
    pub fn_layout: Arc<Vec<Button>>,
    pub expanded_layout: Arc<Vec<Button>>,
    pub control_strip_expanded: bool,
    pub ignore_input: bool,
    pub width: i32,
    pub height: i32,
    pub has_physical_esc: bool,
    is_shift_pressed: bool,
    is_super_pressed: bool,
    pub default_dynamic_area_bounds: Rect,
    pub dynamic_drawable: DynamicDrawable,
    pub media_button_visible: bool,
    pub media_info_visible: bool,
    pub active_player_index: usize,
}

impl AppState {
    pub fn new(width: i32, height: i32, has_physical_esc: bool, media_info: &Vec<MediaInfo>) -> Result<Self> {
        println!("[ndfr] Initializing new state");
        let (default_buttons, default_dynamic_area_bounds) = create_default_layout(width, height, has_physical_esc, media_info)?;
        let default_layout = Arc::new(default_buttons);
        let fn_layout = Arc::new(create_fn_layout(width, height)?);
        let expanded_layout = Arc::new(create_expanded_layout(width, height)?);
        Ok(AppState {
            page: Page::Default(Arc::clone(&default_layout)),
           brightness_value: 0.5,
           volume_value: 0.5,
           gesture: Gesture::Idle,
           animation_start: Instant::now(),
           needs_redraw: true,
           is_animating: false,
           last_input_time: Instant::now(),
           last_volume_update: Instant::now(),
           default_layout,
           fn_layout,
           expanded_layout,
           control_strip_expanded: false,
           ignore_input: false,
           width,
           height,
           has_physical_esc,
           is_shift_pressed: false,
           is_super_pressed: false,
           default_dynamic_area_bounds,
           dynamic_drawable: DynamicManager::create_clock_drawable(),
            media_button_visible: !media_info.is_empty(),
            media_info_visible: false,
            active_player_index: 0,
        })
    }

    pub fn handle_event(&mut self, event: InputEvent, uinput: &mut UInputHandle<File>, latest_media_info: &Arc<Mutex<Vec<MediaInfo>>>) -> Result<()> {
        if !self.ignore_input {
            self.last_input_time = Instant::now();
        }
        match event {
            InputEvent::Touch(touch_event) => self.handle_touch_event(touch_event, uinput, latest_media_info)?,
            InputEvent::FnKeyPressed => self.handle_fn_key(true),
            InputEvent::FnKeyReleased => self.handle_fn_key(false),
            InputEvent::KeyPressed(code) => self.handle_key_press(code)?,
            InputEvent::KeyReleased(code) => self.handle_key_release(code),
        }
        Ok(())
    }

    fn handle_fn_key(&mut self, pressed: bool) {
        if self.is_animating || self.ignore_input {
            return;
        }

        self.gesture = Gesture::Idle;
        if pressed {
            self.page = Page::FnKeys(Arc::clone(&self.fn_layout));
        } else {
            if self.control_strip_expanded {
                self.page = Page::Default(Arc::clone(&self.expanded_layout));
            } else {
                self.page = Page::Default(Arc::clone(&self.default_layout));
            }
        }
        self.needs_redraw = true;
    }

    fn handle_touch_event(&mut self, event: TouchEvent, uinput: &mut UInputHandle<File>, latest_media_info: &Arc<Mutex<Vec<MediaInfo>>>) -> Result<()> {
        if self.is_animating || self.ignore_input {
            return Ok(());
        }

        match event {
            TouchEvent::Down(x_raw) => {
                let x_down = x_raw / 32767.0 * self.width as f64;

                if self.media_info_visible && matches!(&self.page, Page::Default(_)) && !self.control_strip_expanded {
                    if let DynamicDrawable::Media { ref primary_info, ref secondary_info, .. } = self.dynamic_drawable {
                        let bounds = self.default_dynamic_area_bounds;
                        let icon_size = self.height as f64 * 0.7;
                        let mut current_x = bounds.x;

                        // Check for tap on primary icon
                        let primary_icon_width = if !primary_info.icon_name.is_empty() { icon_size + 20.0 } else { 0.0 };
                        if x_down >= current_x && x_down <= current_x + primary_icon_width {
                            let info_lock = latest_media_info.lock().unwrap();
                            if info_lock.len() > 1 {
                                self.active_player_index = (self.active_player_index + 1) % info_lock.len();
                                self.needs_redraw = true;
                            }
                            return Ok(());
                        }
                        current_x += primary_icon_width;

                        if secondary_info.is_some() {
                            let sec_icon_width = icon_size + 20.0;
                            if x_down >= current_x && x_down <= current_x + sec_icon_width {
                                return Ok(());
                            }
                        }

                        if let Some(scrubber_bounds) = self.dynamic_drawable.scrubber_bounds(&bounds) {
                            if x_down >= scrubber_bounds.x && x_down <= scrubber_bounds.x + scrubber_bounds.width {
                                println!("[touch] Scrubber tapped. Starting drag.");
                                self.gesture = Gesture::ScrubberDrag { player_id: primary_info.player_id.clone() };
                                self.needs_redraw = true;
                                return Ok(());
                            }
                        }
                    }
                }

                match &mut self.page {
                    Page::BrightnessSlider(slider) => {
                        if slider.is_hit(x_down) {
                            slider.update_value(x_down);
                            self.brightness_value = slider.value;
                            self.gesture = Gesture::SliderDrag;
                            self.needs_redraw = true;
                        } else {
                            self.page = Page::BrightnessSliderClosing(slider.clone());
                            self.animation_start = Instant::now();
                            self.is_animating = true;
                            self.gesture = Gesture::Idle;
                            self.needs_redraw = true;
                        }
                    }
                    Page::VolumeSlider(slider) => {
                        if slider.is_hit(x_down) {
                            slider.update_value(x_down);
                            self.volume_value = slider.value;
                            self.gesture = Gesture::SliderDrag;
                            self.needs_redraw = true;
                        } else {
                            self.page = Page::VolumeSliderClosing(slider.clone());
                            self.animation_start = Instant::now();
                            self.is_animating = true;
                            self.gesture = Gesture::Idle;
                            self.needs_redraw = true;
                        }
                    }
                    Page::Default(buttons) | Page::FnKeys(buttons) | Page::ControlStripExpanding(buttons) | Page::ControlStripClosing(buttons) => {
                        if let Some(hit_index) = buttons.iter().position(|b| b.is_hit(x_down)) {
                            self.gesture = Gesture::ButtonDown { button_index: hit_index };
                            self.needs_redraw = true;
                        }
                    }
                    _ => {}
                }
            }
            TouchEvent::Motion(x_raw) => {
                let x_motion = x_raw / 32767.0 * self.width as f64;
                match self.gesture {
                    Gesture::SliderDrag => {
                        if let Page::BrightnessSlider(slider) = &mut self.page {
                            slider.update_value(x_motion);
                            self.brightness_value = slider.value;
                            self.needs_redraw = true;
                        }
                        if let Page::VolumeSlider(slider) = &mut self.page {
                            slider.update_value(x_motion);
                            self.volume_value = slider.value;
                            self.needs_redraw = true;
                        }
                    }
                    Gesture::ScrubberDrag { .. } => {
                        if self.control_strip_expanded { return Ok(()); }
                        
                        if let Some(scrubber_bounds) = self.dynamic_drawable.scrubber_bounds(&self.default_dynamic_area_bounds) {
                            if let DynamicDrawable::Media { ref mut primary_info, .. } = self.dynamic_drawable {
                                if x_motion >= scrubber_bounds.x && x_motion <= scrubber_bounds.x + scrubber_bounds.width {
                                    let progress = ((x_motion - scrubber_bounds.x) / scrubber_bounds.width).max(0.0).min(1.0);
                                    let new_pos_usecs = (progress * primary_info.duration_s() * 1_000_000.0) as i64;
                                    primary_info.set_position(new_pos_usecs);
                                    self.needs_redraw = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            // NOTE: Cannot be loaded at boot because of this.
            TouchEvent::Up => {
                if let Gesture::ScrubberDrag { .. } = self.gesture {
                    if !self.control_strip_expanded {
                        if let DynamicDrawable::Media { ref primary_info, .. } = self.dynamic_drawable {
                            if let (Ok(sudo_user), Ok(sudo_uid)) = (env::var("SUDO_USER"), env::var("SUDO_UID")) {
                                Command::new("sudo")
                                    .arg("-u")
                                    .arg(sudo_user)
                                    .arg("env")
                                    .arg(format!("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{}/bus", sudo_uid))
                                    .arg(format!("XDG_RUNTIME_DIR=/run/user/{}", sudo_uid))
                                    .arg("/usr/bin/ndfr-media-helper")
                                    .arg("set-position")
                                    .arg(&primary_info.player_id)
                                    .arg(primary_info.position_usecs().to_string())
                                    .spawn()?;
                            } else {
                                Command::new("/usr/bin/ndfr-media-helper")
                                    .arg("set-position")
                                    .arg(&primary_info.player_id)
                                    .arg(primary_info.position_usecs().to_string())
                                    .spawn()?;
                            }
                        }
                    }
                } else if let Gesture::ButtonDown { button_index } = self.gesture {
                    let mut action_key = None;

                    let buttons = match &self.page {
                        Page::Default(buttons) | Page::FnKeys(buttons) | Page::ControlStripExpanding(buttons) | Page::ControlStripClosing(buttons) => Some(buttons),
                        _ => None,
                    };

                    if let Some(buttons) = buttons {
                        action_key = Some(buttons[button_index].action);
                    }

                    if let Some(action) = action_key {
                        if self.control_strip_expanded {
                            if action == UinputKey::Close || action == UinputKey::Stop {
                                self.control_strip_expanded = false;
                                self.page = Page::ControlStripClosing(Arc::clone(&self.expanded_layout));
                                self.animation_start = Instant::now();
                                self.is_animating = true;
                                self.ignore_input = true;
                            } else {
                                virtual_keyboard::toggle_key(uinput, action, true)?;
                                virtual_keyboard::toggle_key(uinput, action, false)?;
                            }
                        } else {
                            match action {
                                UinputKey::Unknown => {
                                    self.control_strip_expanded = true;
                                    self.page = Page::ControlStripExpanding(Arc::clone(&self.expanded_layout));
                                    self.animation_start = Instant::now();
                                    self.is_animating = true;
                                    self.ignore_input = true;
                                }
                                UinputKey::BrightnessDown | UinputKey::BrightnessUp => {
                                    self.page = Page::BrightnessSlider(create_brightness_slider_layout(self.width, self.height, self.brightness_value)?);
                                    self.animation_start = Instant::now();
                                    self.is_animating = true;
                                }
                                UinputKey::VolumeUp | UinputKey::VolumeDown => {
                                    self.page = Page::VolumeSlider(create_volume_slider_layout(self.width, self.height, self.volume_value)?);
                                    self.animation_start = Instant::now();
                                    self.is_animating = true;
                                }
                                UinputKey::Stop => {
                                    self.media_info_visible = !self.media_info_visible;
                                    self.animation_start = Instant::now();
                                    self.is_animating = true;
                                    if self.media_info_visible {
                                        let info_lock = latest_media_info.lock().unwrap();
                                        if !info_lock.is_empty() {
                                            self.dynamic_drawable = DynamicManager::create_media_drawable(&info_lock, self.active_player_index, self.height);
                                        }
                                        self.page = Page::MediaInfoShowing(Arc::clone(&self.default_layout));
                                    } else {
                                        self.page = Page::MediaInfoHiding(Arc::clone(&self.default_layout));
                                    }
                                    self.needs_redraw = true;
                                }
                                _ => {
                                    virtual_keyboard::toggle_key(uinput, action, true)?;
                                    virtual_keyboard::toggle_key(uinput, action, false)?;
                                }
                            }
                        }
                    }
                } else if let Gesture::SliderDrag = self.gesture {
                    if let Page::VolumeSlider(slider) = &mut self.page {
                        self.volume_value = slider.value;
                        self.last_volume_update = Instant::now();
                        self.needs_redraw = true;
                    }
                }

                self.gesture = Gesture::Idle;
                self.needs_redraw = true;
            }
        }
        Ok(())
    }

    fn handle_key_press(&mut self, code: u16) -> Result<()> {
        let key = EvdevKey(code);
        match key {
            EvdevKey::KEY_LEFTSHIFT | EvdevKey::KEY_RIGHTSHIFT => self.is_shift_pressed = true,
            EvdevKey::KEY_LEFTMETA | EvdevKey::KEY_RIGHTMETA => self.is_super_pressed = true,
            EvdevKey::KEY_6 => {
                if self.is_super_pressed && self.is_shift_pressed {
                    println!("[app] Screenshot shortcut detected!");
                    if let Err(e) = screenshot::take_screenshot(self) {
                        eprintln!("[screenshot] Failed to take screenshot: {}", e);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_key_release(&mut self, code: u16) {
        let key = EvdevKey(code);
        match key {
            EvdevKey::KEY_LEFTSHIFT | EvdevKey::KEY_RIGHTSHIFT => self.is_shift_pressed = false,
            EvdevKey::KEY_LEFTMETA | EvdevKey::KEY_RIGHTMETA => self.is_super_pressed = false,
            _ => {}
        }
    }

    pub fn get_animation_progress(&self) -> f64 {
        if !self.is_animating {
            return 1.0;
        }

        let elapsed = self.animation_start.elapsed().as_millis() as f64;
        let duration = 350.0;
        let t = (elapsed / duration).min(1.0);

        let ease_in_out_quad = |t: f64| {
            if t < 0.5 { 2.0 * t * t } else { -1.0 + (4.0 - 2.0 * t) * t }
        };

        match &self.page {
            Page::BrightnessSlider(_) | Page::VolumeSlider(_) | Page::ControlStripExpanding(_) | Page::MediaInfoShowing(_) => ease_in_out_quad(t),
            Page::BrightnessSliderClosing(_) | Page::VolumeSliderClosing(_) | Page::ControlStripClosing(_) | Page::MediaInfoHiding(_) => 1.0 - ease_in_out_quad(t),
            _ => 1.0,
        }
    }

    pub fn update_animations(&mut self) {
        if !self.is_animating {
            return;
        }

        let progress = self.get_animation_progress();

        let animation_finished = match self.page {
            Page::BrightnessSlider(_) | Page::VolumeSlider(_) | Page::ControlStripExpanding(_) | Page::MediaInfoShowing(_) => progress >= 1.0,
            Page::BrightnessSliderClosing(_) | Page::VolumeSliderClosing(_) | Page::ControlStripClosing(_) | Page::MediaInfoHiding(_) => progress <= 0.0,
            _ => true,
        };

        if animation_finished {
            self.is_animating = false;

            match &self.page {
                Page::BrightnessSliderClosing(_) | Page::VolumeSliderClosing(_) => {
                    self.page = Page::Default(Arc::clone(&self.default_layout));
                }
                Page::ControlStripClosing(_) => {
                    self.page = Page::Default(Arc::clone(&self.default_layout));
                    self.ignore_input = false;
                }
                Page::ControlStripExpanding(_) => {
                    self.page = Page::Default(Arc::clone(&self.expanded_layout));
                    self.ignore_input = false;
                }
                Page::MediaInfoShowing(_) => {
                    self.page = Page::Default(Arc::clone(&self.default_layout));
                }
                Page::MediaInfoHiding(_) => {
                    self.page = Page::Default(Arc::clone(&self.default_layout));
                    self.dynamic_drawable = DynamicManager::create_clock_drawable();
                }
                _ => {}
            }
        }
        self.needs_redraw = self.is_animating;
    }
}
