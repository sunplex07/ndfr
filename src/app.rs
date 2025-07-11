/**
* Author: sunplex07
* (ndfr) is a TouchBar Daemon with a focus on bringing a native macOS-like experience.
*/
use crate::ui::{Page, Button, create_default_layout, create_fn_layout, create_brightness_slider_layout, create_volume_slider_layout, create_expanded_layout};
use crate::input::{InputEvent, TouchEvent};
use crate::virtual_keyboard;
use anyhow::Result;
use input_linux::Key;
use input_linux::uinput::UInputHandle;
use std::fs::File;
use std::time::Instant;

#[derive(Debug)]
pub enum Gesture {
    Idle,
    ButtonDown { button_index: usize },
    SliderDrag,
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
    pub default_layout: Vec<Button>,
    pub fn_layout: Vec<Button>,
    pub expanded_layout: Vec<Button>,
    pub control_strip_expanded: bool,
    pub ignore_input: bool,
}

impl AppState {
    pub fn new() -> Result<Self> {
        println!("[ndfr] Initializing new state");
        let default_layout = create_default_layout(2170, 60)?;
        let fn_layout = create_fn_layout(2170, 60)?;
        let expanded_layout = create_expanded_layout(2170, 60)?;
        Ok(AppState {
            page: Page::Default(default_layout.clone()),
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
        })
    }

    pub fn handle_event(&mut self, event: InputEvent, uinput: &mut UInputHandle<File>) -> Result<()> {
        if !self.ignore_input {
            self.last_input_time = Instant::now();
        }
        match event {
            InputEvent::Touch(touch_event) => self.handle_touch_event(touch_event, uinput)?,
            InputEvent::FnKeyPressed => self.handle_fn_key(true),
            InputEvent::FnKeyReleased => self.handle_fn_key(false),
            InputEvent::Keyboard(key) => {
				
            }
        }
        Ok(())
    }

    fn handle_fn_key(&mut self, pressed: bool) {
        self.gesture = Gesture::Idle;
        if pressed {
            self.page = Page::FnKeys(self.fn_layout.clone());
        } else {
            self.page = Page::Default(self.default_layout.clone());
        }
        self.needs_redraw = true;
    }

    fn handle_touch_event(&mut self, event: TouchEvent, uinput: &mut UInputHandle<File>) -> Result<()> {
        if self.is_animating || self.ignore_input {
            return Ok(());
        }

        match event {
            TouchEvent::Down(x_raw) => {
                let x_down = x_raw / 32767.0 * 2170.0;

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
                            buttons[hit_index].active = true;
                            self.gesture = Gesture::ButtonDown { button_index: hit_index };
                            self.needs_redraw = true;
                        }
                    }
                    _ => {}
                }
            }
            TouchEvent::Motion(x_raw) => {
                let x_motion = x_raw / 32767.0 * 2170.0;
                if let Gesture::SliderDrag = self.gesture {
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
            }
            TouchEvent::Up => {

                if let Gesture::ButtonDown { button_index } = self.gesture {
                    let mut action_key = None;

                    let buttons = match &mut self.page {
                        Page::Default(buttons) | Page::FnKeys(buttons) | Page::ControlStripExpanding(buttons) | Page::ControlStripClosing(buttons) => Some(buttons),
                        _ => None,
                    };

                    if let Some(buttons) = buttons {
                        buttons[button_index].active = false;
                        action_key = Some(buttons[button_index].action);
                    }

                    if let Some(action) = action_key {
                        if self.control_strip_expanded {
                            if action == Key::Close || action == Key::Stop {
                                self.control_strip_expanded = false;
                                self.page = Page::ControlStripClosing(self.expanded_layout.clone());
                                self.animation_start = Instant::now();
                                self.is_animating = true;
                                self.ignore_input = true;
                            } else {
                                virtual_keyboard::toggle_key(uinput, action, true)?;
                                virtual_keyboard::toggle_key(uinput, action, false)?;
                            }
                        } else {
                            match action {
                                Key::Unknown => { 
                                    self.control_strip_expanded = true;
                                    self.page = Page::ControlStripExpanding(self.expanded_layout.clone());
                                    self.animation_start = Instant::now();
                                    self.is_animating = true;
                                    self.ignore_input = true;
                                }
                                Key::BrightnessDown | Key::BrightnessUp => {
                                    self.page = Page::BrightnessSlider(create_brightness_slider_layout(2170, 60, self.brightness_value)?);
                                    self.animation_start = Instant::now();
                                    self.is_animating = true;
                                }
                                Key::VolumeUp | Key::VolumeDown => {
                                    self.page = Page::VolumeSlider(create_volume_slider_layout(2170, 60, self.volume_value)?);
                                    self.animation_start = Instant::now();
                                    self.is_animating = true;
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
                } else {
                    self.gesture = Gesture::Idle;
                }
                self.needs_redraw = true;
            }
        }
        Ok(())
    }
}
