use anyhow::{Result, anyhow};
use cairo::Context;
use input_linux::Key;
use crate::config::{Layout, ButtonRenderMode};
use crate::dynamic::Rect;
use crate::media::MediaInfo;
use std::fs::File;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use usvg::Tree;
use tiny_skia::{Pixmap, Transform};

fn find_resource_path(file_name: &str) -> Result<PathBuf> {
    let mut exe_path = env::current_exe()?;
    exe_path.pop();
    let mut resource_path = exe_path.clone();
    resource_path.push(file_name);
    if resource_path.exists() {
        return Ok(resource_path);
    }

    let mut cwd_path = env::current_dir()?;
    cwd_path.push(file_name);
    if cwd_path.exists() {
        return Ok(cwd_path);
    }

    Err(anyhow!("Could not find resource file: {}", file_name))
}


pub const BUTTON_COLOR_INACTIVE: f64 = 0.200;
pub const BUTTON_COLOR_ACTIVE: f64 = 0.400;

#[derive(Clone, Debug)]
pub enum ButtonContent {
    Text(String),
    Icon(Arc<Tree>),
}

#[derive(Clone, Debug)]
pub enum RoundedCorners {
    All,
    Left,
    Right,
    None,
}

#[derive(Clone, Debug)]
pub struct Button {
    pub content: ButtonContent,
    pub action: Key,
    pub x: f64,
    pub width: f64,
    pub rounded_corners: RoundedCorners,
    pub render_mode: ButtonRenderMode,
}

pub const BUTTON_RADIUS: f64 = 8.0;

impl Button {
    pub fn draw(&self, c: &Context, height: f64, is_active: bool) -> Result<()> {
        let color = if is_active { BUTTON_COLOR_ACTIVE } else { BUTTON_COLOR_INACTIVE };
        c.set_source_rgb(color, color, color);

        c.new_path();
        match self.rounded_corners {
            RoundedCorners::All => {
                c.arc(self.x + BUTTON_RADIUS, BUTTON_RADIUS, BUTTON_RADIUS, 180.0f64.to_radians(), 270.0f64.to_radians());
                c.arc(self.x + self.width - BUTTON_RADIUS, BUTTON_RADIUS, BUTTON_RADIUS, -90.0f64.to_radians(), 0.0f64.to_radians());
                c.arc(self.x + self.width - BUTTON_RADIUS, height - BUTTON_RADIUS, BUTTON_RADIUS, 0.0f64.to_radians(), 90.0f64.to_radians());
                c.arc(self.x + BUTTON_RADIUS, height - BUTTON_RADIUS, BUTTON_RADIUS, 90.0f64.to_radians(), 180.0f64.to_radians());
            }
            RoundedCorners::Left => {
                c.arc(self.x + BUTTON_RADIUS, BUTTON_RADIUS, BUTTON_RADIUS, 180.0f64.to_radians(), 270.0f64.to_radians());
                c.line_to(self.x + self.width, 0.0);
                c.line_to(self.x + self.width, height);
                c.arc(self.x + BUTTON_RADIUS, height - BUTTON_RADIUS, BUTTON_RADIUS, 90.0f64.to_radians(), 180.0f64.to_radians());
            }
            RoundedCorners::Right => {
                c.move_to(self.x, 0.0);
                c.arc(self.x + self.width - BUTTON_RADIUS, BUTTON_RADIUS, BUTTON_RADIUS, -90.0f64.to_radians(), 0.0f64.to_radians());
                c.arc(self.x + self.width - BUTTON_RADIUS, height - BUTTON_RADIUS, BUTTON_RADIUS, 0.0f64.to_radians(), 90.0f64.to_radians());
                c.line_to(self.x, height);
            }
            RoundedCorners::None => {
                c.rectangle(self.x, 0.0, self.width, height);
            }
        }
        c.close_path();
        c.fill()?;

        c.set_source_rgb(1.0, 1.0, 1.0);
        c.set_font_size(24.0);
        match &self.content {
            ButtonContent::Text(text) => {
                let extents = c.text_extents(text)?;
                let text_x = self.x + (self.width / 2.0) - (extents.width() / 2.0);
                let mut text_y = (height / 2.0) + (extents.height() / 2.0);
                if text == "Apps" {
                    text_y -= 4.0;
                }
                c.move_to(text_x, text_y);
                c.show_text(text)?;
            }
            ButtonContent::Icon(tree) => {
                let icon_size = height * 0.6;
                let icon_x = self.x + (self.width - icon_size) / 2.0;
                let icon_y = (height - icon_size) / 2.0;

                let mut pixmap = Pixmap::new(icon_size as u32, icon_size as u32).unwrap();
                let transform = Transform::from_scale(
                    icon_size as f32 / tree.size().width(),
                                                      icon_size as f32 / tree.size().height(),
                );
                resvg::render(tree, transform, &mut pixmap.as_mut());

                let mut data = pixmap.data().to_vec();
                for chunk in data.chunks_mut(4) {
                    chunk.swap(0, 2); // BGRA -> ARGB
                }

                let surface = cairo::ImageSurface::create_for_data(
                    data.into_boxed_slice(),
                                                                   cairo::Format::ARgb32,
                                                                   icon_size as i32,
                                                                   icon_size as i32,
                                                                   cairo::Format::ARgb32.stride_for_width(icon_size as u32).unwrap(),
                )?;

                match self.render_mode {
                    ButtonRenderMode::Mask => {
                        c.mask_surface(&surface, icon_x, icon_y)?;
                    }
                    ButtonRenderMode::Color => {
                        c.set_source_surface(&surface, icon_x, icon_y)?;
                        c.paint()?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn is_hit(&self, x: f64) -> bool {
        x >= self.x && x <= self.x + self.width
    }
}

#[derive(Clone, Debug)]
pub enum SliderKind {
    Brightness,
    Volume,
}

#[derive(Clone, Debug)]
pub struct Slider {
    pub x: f64,
    pub width: f64,
    pub value: f64,
    pub kind: SliderKind,
}

impl Slider {
    pub fn draw(&self, c: &Context, height: f64, animation_progress: f64) -> Result<()> {
        let animated_width = self.width * animation_progress;
        let animated_x = self.x + (self.width - animated_width) / 2.0;

        // bg
        c.set_source_rgb(0.1, 0.1, 0.1);
        c.new_path();
        c.arc(animated_x + BUTTON_RADIUS, BUTTON_RADIUS, BUTTON_RADIUS, 180.0f64.to_radians(), 270.0f64.to_radians());
        c.arc(animated_x + animated_width - BUTTON_RADIUS, BUTTON_RADIUS, BUTTON_RADIUS, -90.0f64.to_radians(), 0.0f64.to_radians());
        c.arc(animated_x + animated_width - BUTTON_RADIUS, height - BUTTON_RADIUS, BUTTON_RADIUS, 0.0f64.to_radians(), 90.0f64.to_radians());
        c.arc(animated_x + BUTTON_RADIUS, height - BUTTON_RADIUS, BUTTON_RADIUS, 90.0f64.to_radians(), 180.0f64.to_radians());
        c.close_path();
        c.fill_preserve()?;
        c.set_source_rgb(0.5, 0.5, 0.5);
        c.set_line_width(1.0);
        c.stroke()?;

        let alpha = (animation_progress - 0.5) * 2.0;
        if alpha > 0.0 {
            let alpha = alpha.max(0.0).min(1.0);

            // active slider
            let active_width = animated_width * self.value;
            c.set_source_rgba(0.8, 0.8, 0.8, alpha);
            c.new_path();
            c.arc(animated_x + BUTTON_RADIUS, BUTTON_RADIUS, BUTTON_RADIUS, 180.0f64.to_radians(), 270.0f64.to_radians());
            c.arc(animated_x + active_width - BUTTON_RADIUS, BUTTON_RADIUS, BUTTON_RADIUS, -90.0f64.to_radians(), 0.0f64.to_radians());
            c.arc(animated_x + active_width - BUTTON_RADIUS, height - BUTTON_RADIUS, BUTTON_RADIUS, 0.0f64.to_radians(), 90.0f64.to_radians());
            c.arc(animated_x + BUTTON_RADIUS, height - BUTTON_RADIUS, BUTTON_RADIUS, 90.0f64.to_radians(), 180.0f64.to_radians());
            c.close_path();
            c.fill()?;

            // handle
            let handle_x = animated_x + active_width;
            c.set_source_rgba(1.0, 1.0, 1.0, alpha);
            c.new_path();
            c.arc(handle_x, height / 2.0, BUTTON_RADIUS * 1.2, 0.0, 2.0 * std::f64::consts::PI);
            c.fill()?;

            let mut exe_path = env::current_exe()?;
            exe_path.pop();

            let (icon_left_name, icon_right_name) = match self.kind {
                SliderKind::Brightness => ("brightness-low.svg", "brightness-high.svg"),
                SliderKind::Volume => ("volume-low.svg", "volume-high.svg"),
            };

            for (icon_name, side) in [(icon_left_name, -1), (icon_right_name, 1)] {
                let icon_path = find_resource_path(&format!("icons/{}", icon_name))?;
                let svg_data = std::fs::read(icon_path)?;
                let tree = Tree::from_data(&svg_data, &usvg::Options::default())?;

                let icon_size = height * 0.6;
                let icon_y = (height - icon_size) / 2.0;

                let mut pixmap = Pixmap::new(icon_size as u32, icon_size as u32).unwrap();
                let transform = Transform::from_scale(
                    icon_size as f32 / tree.size().width(),
                                                      icon_size as f32 / tree.size().height(),
                );
                resvg::render(&tree, transform, &mut pixmap.as_mut());

                let surface = cairo::ImageSurface::create_for_data(
                    pixmap.data().to_vec().into_boxed_slice(),
                                                                   cairo::Format::ARgb32,
                                                                   icon_size as i32,
                                                                   icon_size as i32,
                                                                   cairo::Format::ARgb32.stride_for_width(icon_size as u32).unwrap(),
                )?;

                let icon_x = if side == -1 {
                    animated_x + 10.0
                } else {
                    animated_x + animated_width - icon_size - 10.0
                };

                c.set_source_rgba(1.0, 1.0, 1.0, alpha);
                c.mask_surface(&surface, icon_x, icon_y)?;
            }
        }

        Ok(())
    }

    pub fn is_hit(&self, x: f64) -> bool {
        const HIT_MARGIN: f64 = 10.0;
        x >= self.x - HIT_MARGIN && x <= self.x + self.width + HIT_MARGIN
    }

    pub fn update_value(&mut self, x: f64) {
        self.value = ((x - self.x) / self.width).max(0.0).min(1.0);
    }
}

#[derive(Clone, Debug)]
pub enum Page {
    Default(Arc<Vec<Button>>),
    FnKeys(Arc<Vec<Button>>),
    BrightnessSlider(Slider),
    BrightnessSliderClosing(Slider),
    VolumeSlider(Slider),
    VolumeSliderClosing(Slider),
    ControlStripExpanding(Arc<Vec<Button>>),
    ControlStripClosing(Arc<Vec<Button>>),
    MediaInfoShowing(Arc<Vec<Button>>),
    MediaInfoHiding(Arc<Vec<Button>>),
}

pub fn create_default_layout(width: i32, height: i32, has_physical_esc: bool, media_info: &Vec<MediaInfo>) -> Result<(Vec<Button>, Rect)> {
    let layout_path = find_resource_path("layout.yml")?;
    let f = File::open(layout_path)?;
    let layout: Layout = serde_yaml::from_reader(f)?;

    let mut buttons = Vec::new();
    let mut current_x = 0.0;

    let left_buttons_config: Vec<_> = layout.left.buttons.into_iter().filter(|b| {
        !(string_to_key(&b.action) == Key::Esc && has_physical_esc)
    }).collect();

    for button_config in left_buttons_config {
        let content = if let Some(icon_name) = &button_config.icon {
            let icon_path = find_resource_path(&format!("icons/{}", icon_name))?;
            let svg_data = std::fs::read(icon_path)?;
            let tree = Tree::from_data(&svg_data, &usvg::Options::default())?;
            ButtonContent::Icon(Arc::new(tree))
        } else {
            ButtonContent::Text(button_config.text.clone().unwrap_or_default())
        };

        buttons.push(Button {
            content,
            action: string_to_key(&button_config.action),
                     x: current_x,
                     width: button_config.width,
                     rounded_corners: RoundedCorners::All,
            render_mode: button_config.render_mode.clone(),
        });
        current_x += button_config.width + layout.left.spacing;
    }

    let left_buttons_end_x = if current_x > 0.0 { current_x - layout.left.spacing } else { 0.0 };

    let mut right_buttons_config: Vec<_> = layout.right.buttons.into_iter().filter(|b| {
        !(string_to_key(&b.action) == Key::Esc && has_physical_esc)
    }).collect();

    if !media_info.is_empty() {
        let media = &media_info[0];
        if !media.icon_name.is_empty() {
            right_buttons_config.insert(1, crate::config::ButtonConfig {
                text: None,
                icon: Some(media.icon_name.clone()),
                action: "KEY_TOGGLE_MEDIA".to_string(),
                width: 80.0,
                render_mode: ButtonRenderMode::Color,
            });
        }
    }

    let right_buttons_width: f64 = right_buttons_config.iter().map(|b| b.width).sum::<f64>()
    + layout.right.spacing * (right_buttons_config.len().saturating_sub(1)) as f64;
    let right_buttons_start_x = width as f64 - right_buttons_width;
    current_x = right_buttons_start_x;

    let num_right_buttons = right_buttons_config.len();
    for (i, button_config) in right_buttons_config.iter().enumerate() {
        let content = if let Some(icon_name) = &button_config.icon {
            let icon_path = crate::dynamic::find_icon_path(icon_name)
                .or_else(|| find_resource_path(&format!("icons/{}", icon_name)).ok())
                .or_else(|| find_resource_path("icons/media.svg").ok())
                .ok_or_else(|| anyhow!("Could not find icon for {} or fallback media.svg", icon_name))?;
            let svg_data = std::fs::read(icon_path)?;
            let tree = Tree::from_data(&svg_data, &usvg::Options::default())?;
            ButtonContent::Icon(Arc::new(tree))
        } else {
            ButtonContent::Text(button_config.text.clone().unwrap_or_default())
        };

        let rounded_corners = if num_right_buttons == 1 {
            RoundedCorners::All
        } else if i == 0 {
            RoundedCorners::Left
        } else if i == num_right_buttons - 1 {
            RoundedCorners::Right
        } else {
            RoundedCorners::None
        };
        buttons.push(Button {
            content,
            action: string_to_key(&button_config.action),
                     x: current_x,
                     width: button_config.width,
                     rounded_corners,
            render_mode: button_config.render_mode.clone(),
        });
        current_x += button_config.width + layout.right.spacing;
    }

    let dynamic_bounds = Rect {
        x: left_buttons_end_x,
        y: 0.0,
        width: right_buttons_start_x - left_buttons_end_x - 10.0,
        height: height as f64,
    };

    Ok((buttons, dynamic_bounds))
}

pub fn create_fn_layout(width: i32, _height: i32) -> Result<Vec<Button>> {
    let mut buttons = Vec::new();
    let num_buttons = 12;
    let spacing = 10.0;
    let button_width = (width as f64 - (spacing * (num_buttons - 1) as f64)) / num_buttons as f64;

    for i in 0..num_buttons {
        let key_name = format!("F{}", i + 1);
        buttons.push(Button {
            content: ButtonContent::Text(key_name.clone()),
                     action: string_to_key(&format!("KEY_{}", key_name)),
                     x: i as f64 * (button_width + spacing),
                     width: button_width,
                     rounded_corners: RoundedCorners::All,
            render_mode: ButtonRenderMode::Mask,
        });
    }

    Ok(buttons)
}

pub fn create_brightness_slider_layout(width: i32, _height: i32, value: f64) -> Result<Slider> {
    let slider_width = width as f64 * 0.5;
    let slider_x = (width as f64 - slider_width) / 2.0;
    Ok(Slider {
        x: slider_x,
       width: slider_width,
       value,
       kind: SliderKind::Brightness,
    })
}

pub fn create_volume_slider_layout(width: i32, _height: i32, value: f64) -> Result<Slider> {
    let slider_width = width as f64 * 0.5;
    let slider_x = (width as f64 - slider_width) / 2.0;
    Ok(Slider {
        x: slider_x,
       width: slider_width,
       value,
       kind: SliderKind::Volume,
    })
}

pub fn create_expanded_layout(width: i32, _height: i32) -> Result<Vec<Button>> {
    let mut buttons = Vec::new();
    let mut current_x = 0.0;
    let spacing = 2.0;
    let group_spacing = 15.0;

    let button_definitions = vec![
        ("close.svg", Key::Close, RoundedCorners::All, true),
        ("brightness-down.svg", Key::BrightnessDown, RoundedCorners::Left, false),
        ("brightness-up.svg", Key::BrightnessUp, RoundedCorners::Right, true),
        ("mission-control.svg", Key::F13, RoundedCorners::All, true),
        ("previous.svg", Key::PreviousSong, RoundedCorners::Left, false),
        ("play-pause.svg", Key::PlayPause, RoundedCorners::None, false),
        ("next.svg", Key::NextSong, RoundedCorners::Right, true),
        ("mute.svg", Key::Mute, RoundedCorners::Left, false),
        ("volume-down.svg", Key::VolumeDown, RoundedCorners::None, false),
        ("volume-up.svg", Key::VolumeUp, RoundedCorners::Right, true),
    ];

    let total_buttons = button_definitions.len();
    let num_groups = button_definitions.iter().filter(|b| b.3).count();
    let total_spacing = (total_buttons as f64 - num_groups as f64) * spacing + (num_groups as f64 - 1.0) * group_spacing;
    let button_width = (width as f64 - total_spacing) / total_buttons as f64;

    for (icon_name, action, rounded_corners, is_group_ender) in button_definitions {
        let icon_path = find_resource_path(&format!("icons/{}", icon_name))?;
        let svg_data = std::fs::read(icon_path)?;
        let tree = Tree::from_data(&svg_data, &usvg::Options::default())?;

        buttons.push(Button {
            content: ButtonContent::Icon(Arc::new(tree)),
                     action,
                     x: current_x,
                     width: button_width,
                     rounded_corners,
            render_mode: ButtonRenderMode::Mask,
        });

        current_x += button_width;
        if is_group_ender {
            current_x += group_spacing;
        } else {
            current_x += spacing;
        }
    }

    Ok(buttons)
}

fn string_to_key(s: &str) -> Key {
    match s {
        "KEY_ESC" => Key::Esc,
        "KEY_BRIGHTNESSDOWN" => Key::BrightnessDown,
        "KEY_BRIGHTNESSUP" => Key::BrightnessUp,
        "KEY_VOLUMEUP" => Key::VolumeUp,
        "KEY_VOLUMEDOWN" => Key::VolumeDown,
        "KEY_MUTE" => Key::Mute,
        "KEY_F1" => Key::F1,
        "KEY_F2" => Key::F2,
        "KEY_F3" => Key::F3,
        "KEY_F4" => Key::F4,
        "KEY_F5" => Key::F5,
        "KEY_F6" => Key::F6,
        "KEY_F7" => Key::F7,
        "KEY_F8" => Key::F8,
        "KEY_F9" => Key::F9,
        "KEY_F10" => Key::F10,
        "KEY_F11" => Key::F11,
        "KEY_F12" => Key::F12,
        "KEY_F13" => Key::F13,
        "KEY_PREVIOUSSONG" => Key::PreviousSong,
        "KEY_PLAYPAUSE" => Key::PlayPause,
        "KEY_NEXTSONG" => Key::NextSong,
        "KEY_TOGGLE_MEDIA" => Key::Stop,
        _ => Key::Unknown,
    }
}
