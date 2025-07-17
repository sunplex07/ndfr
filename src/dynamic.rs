use anyhow::Result;
use cairo::{Context, Format, ImageSurface, SurfacePattern};
use crate::media::MediaInfo;
use std::path::PathBuf;
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};
use usvg::Tree;

#[derive(Copy, Clone, Debug)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug)]
pub enum DynamicDrawable {
    Clock(String),
    Media {
        primary_info: MediaInfo,
        secondary_info: Option<MediaInfo>,
        primary_icon_pixmap: Option<Arc<Pixmap>>,
        secondary_icon_pixmap: Option<Arc<Pixmap>>,
        scrubber_texture_data: Option<Arc<Vec<u8>>>,
    },
}

impl PartialEq for DynamicDrawable {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Clock(l0), Self::Clock(r0)) => l0 == r0,
            (Self::Media { primary_info: l_info, secondary_info: l_s_info, .. }, Self::Media { primary_info: r_info, secondary_info: r_s_info, .. }) => {
                l_info == r_info && l_s_info == r_s_info
            }
            _ => false,
        }
    }
}

pub fn find_icon_path(icon_name: &str) -> Option<PathBuf> {
    if icon_name.is_empty() {
        return None;
    }
    let search_paths = [
        "/usr/share/icons/hicolor/scalable/apps/",
        "/usr/share/icons/hicolor/48x48/apps/",
        "/usr/share/pixmaps/",
    ];
    for base_path in search_paths {
        let mut path = PathBuf::from(base_path);
        path.push(format!("{}.svg", icon_name));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

impl DynamicDrawable {
    pub fn scrubber_bounds(&self, bounds: &Rect) -> Option<Rect> {
        match self {
            DynamicDrawable::Media { primary_info, secondary_info, .. } => {
                let icon_size = bounds.height * 0.7;
                let primary_icon_width = if primary_info.icon_name.is_empty() { 0.0 } else { icon_size + 20.0 };
                let secondary_icon_width = if secondary_info.is_some() { icon_size + 20.0 } else { 0.0 };

                let scrubber_x = bounds.x + primary_icon_width + secondary_icon_width;
                let scrubber_width = bounds.width - primary_icon_width - secondary_icon_width - 10.0;

                if scrubber_width <= 0.0 {
                    return None;
                }

                Some(Rect {
                    x: scrubber_x,
                    y: bounds.y + 5.0,
                    width: scrubber_width,
                    height: bounds.height - 10.0,
                })
            }
            _ => None,
        }
    }

    pub fn draw(&self, c: &Context, bounds: &Rect, is_dragging: bool) -> Result<()> {
        match self {
            DynamicDrawable::Clock(time_str) => {
                c.set_source_rgb(0.8, 0.8, 0.8);
                c.set_font_size(24.0);
                let extents = c.text_extents(time_str)?;
                let text_x = bounds.x + (bounds.width / 2.0) - (extents.width() / 2.0);
                let text_y = (bounds.height / 2.0) + (extents.height() / 2.0);
                c.move_to(text_x, text_y);
                c.show_text(time_str)?;
            }
            DynamicDrawable::Media { primary_info, .. } => {
                let button_color = 0.2;
                let radius = 8.0;

                // Draw a single, connected background for the whole component
                c.new_path();
                c.arc(bounds.x + radius, bounds.y + radius, radius, 180.0f64.to_radians(), 270.0f64.to_radians());
                c.arc(bounds.x + bounds.width - radius, bounds.y + radius, radius, -90.0f64.to_radians(), 0.0f64.to_radians());
                c.arc(bounds.x + bounds.width - radius, bounds.y + bounds.height - radius, radius, 0.0f64.to_radians(), 90.0f64.to_radians());
                c.arc(bounds.x + radius, bounds.y + bounds.height - radius, radius, 90.0f64.to_radians(), 180.0f64.to_radians());
                c.close_path();
                c.set_source_rgb(button_color, button_color, button_color);
                c.fill()?;

                self.draw_media_contents(c, bounds, is_dragging, primary_info)?;
            }
        }
        Ok(())
    }

    fn draw_media_contents(&self, c: &Context, bounds: &Rect, is_dragging: bool, primary_info: &MediaInfo) -> Result<()> {
        if let DynamicDrawable::Media { primary_icon_pixmap, secondary_icon_pixmap, scrubber_texture_data, .. } = self {
            let icon_size = bounds.height * 0.7;
            let radius = 8.0;
            let mut current_x = bounds.x + 10.0;

            if let Some(pixmap) = primary_icon_pixmap {
                let icon_y = (bounds.height - icon_size) / 2.0;
                let mut data = pixmap.data().to_vec();
                for chunk in data.chunks_mut(4) { chunk.swap(0, 2); }
                let surface = ImageSurface::create_for_data(
                    data.into_boxed_slice(), Format::ARgb32, pixmap.width() as i32, pixmap.height() as i32,
                    Format::ARgb32.stride_for_width(pixmap.width()).unwrap(),
                )?;
                c.set_source_surface(&surface, current_x, icon_y)?;
                c.paint()?;
                current_x += icon_size + 10.0;
            }

            if let Some(pixmap) = secondary_icon_pixmap {
                let icon_y = (bounds.height - icon_size) / 2.0;
                let mut data = pixmap.data().to_vec();
                for chunk in data.chunks_mut(4) { chunk.swap(0, 2); }
                let surface = ImageSurface::create_for_data(
                    data.into_boxed_slice(), Format::ARgb32, pixmap.width() as i32, pixmap.height() as i32,
                    Format::ARgb32.stride_for_width(pixmap.width()).unwrap(),
                )?;
                c.set_source_surface(&surface, current_x, icon_y)?;
                c.paint()?;
            }

            if let Some(scrubber_bounds) = self.scrubber_bounds(bounds) {
                c.set_source_rgb(0.0, 0.0, 0.0);
                c.new_path();
                c.arc(scrubber_bounds.x + radius, scrubber_bounds.y + radius, radius, 180.0f64.to_radians(), 270.0f64.to_radians());
                c.arc(scrubber_bounds.x + scrubber_bounds.width - radius, scrubber_bounds.y + radius, radius, -90.0f64.to_radians(), 0.0f64.to_radians());
                c.arc(scrubber_bounds.x + scrubber_bounds.width - radius, scrubber_bounds.y + scrubber_bounds.height - radius, radius, 0.0f64.to_radians(), 90.0f64.to_radians());
                c.arc(scrubber_bounds.x + radius, scrubber_bounds.y + scrubber_bounds.height - radius, radius, 90.0f64.to_radians(), 180.0f64.to_radians());
                c.close_path();
                c.fill()?;

                if let Some(texture_data) = scrubber_texture_data {
                    let texture_surface = ImageSurface::create_for_data(
                        texture_data.to_vec().into_boxed_slice(), Format::ARgb32, 3, bounds.height as i32,
                        Format::ARgb32.stride_for_width(3).unwrap(),
                    )?;
                    let pattern = SurfacePattern::create(&texture_surface);
                    pattern.set_extend(cairo::Extend::Repeat);
                    c.save()?;
                    c.new_path();
                    c.arc(scrubber_bounds.x + radius, scrubber_bounds.y + radius, radius, 180.0f64.to_radians(), 270.0f64.to_radians());
                    c.arc(scrubber_bounds.x + scrubber_bounds.width - radius, scrubber_bounds.y + radius, radius, -90.0f64.to_radians(), 0.0f64.to_radians());
                    c.arc(scrubber_bounds.x + scrubber_bounds.width - radius, scrubber_bounds.y + scrubber_bounds.height - radius, radius, 0.0f64.to_radians(), 90.0f64.to_radians());
                    c.arc(scrubber_bounds.x + radius, scrubber_bounds.y + scrubber_bounds.height - radius, radius, 90.0f64.to_radians(), 180.0f64.to_radians());
                    c.close_path();
                    c.clip();
                    c.set_source(&pattern)?;
                    c.paint()?;
                    c.restore()?;
                }

                if primary_info.duration_s() > 0.0 {
                    let progress = (primary_info.position_s() / primary_info.duration_s()).min(1.0).max(0.0);
                    let playhead_x = scrubber_bounds.x + scrubber_bounds.width * progress;
                    if is_dragging {
                        let box_width = 80.0;
                        let box_height = bounds.height * 0.8;
                        let box_x = (playhead_x - box_width / 2.0).max(bounds.x).min(bounds.x + bounds.width - box_width);
                        let box_y = (bounds.height - box_height) / 2.0;
                        let r = 6.0;
                        c.new_path();
                        c.arc(box_x + r, box_y + r, r, 180.0f64.to_radians(), 270.0f64.to_radians());
                        c.arc(box_x + box_width - r, box_y + r, r, -90.0f64.to_radians(), 0.0f64.to_radians());
                        c.arc(box_x + box_width - r, box_y + box_height - r, r, 0.0f64.to_radians(), 90.0f64.to_radians());
                        c.arc(box_x + r, box_y + box_height - r, r, 90.0f64.to_radians(), 180.0f64.to_radians());
                        c.close_path();
                        c.set_source_rgb(0.9, 0.9, 0.9);
                        c.fill()?;
                        let pos_s = primary_info.position_s() as i32;
                        let time_str = format!("{:02}:{:02}", pos_s / 60, pos_s % 60);
                        c.set_source_rgb(0.0, 0.0, 0.0);
                        c.set_font_size(22.0);
                        let extents = c.text_extents(&time_str)?;
                        let text_x = box_x + (box_width - extents.width()) / 2.0;
                        let text_y = box_y + (box_height + extents.height()) / 2.0;
                        c.move_to(text_x, text_y);
                        c.show_text(&time_str)?;
                    } else {
                        c.set_source_rgb(1.0, 1.0, 1.0);
                        c.set_line_width(5.0);
                        c.move_to(playhead_x, scrubber_bounds.y);
                        c.line_to(playhead_x, scrubber_bounds.y + scrubber_bounds.height);
                        c.stroke()?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct DynamicManager;

impl DynamicManager {
    pub fn create_clock_drawable() -> DynamicDrawable {
        let now = chrono::Local::now();
        let time_str = now.format("%-l:%M %p").to_string().trim().to_string();
        DynamicDrawable::Clock(time_str)
    }

    pub fn create_media_drawable(players: &[MediaInfo], active_player_index: usize, height: i32) -> DynamicDrawable {
        if players.is_empty() {
            return DynamicManager::create_clock_drawable();
        }
        let primary_info = players[active_player_index].clone();
        let secondary_info = if players.len() > 1 {
            players.iter().cycle().skip(active_player_index + 1).find(|p| p.player_id != primary_info.player_id).cloned()
        } else {
            None
        };

        let mut primary_icon_pixmap = None;
        if let Some(icon_path) = find_icon_path(&primary_info.icon_name) {
            if let Ok(svg_data) = std::fs::read(icon_path) {
                if let Ok(tree) = Tree::from_data(&svg_data, &usvg::Options::default()) {
                    let icon_size = height as f64 * 0.7;
                    if let Some(mut pm) = Pixmap::new(icon_size as u32, icon_size as u32) {
                        let transform = Transform::from_scale(
                            icon_size as f32 / tree.size().width(),
                            icon_size as f32 / tree.size().height(),
                        );
                        resvg::render(&tree, transform, &mut pm.as_mut());
                        primary_icon_pixmap = Some(Arc::new(pm));
                    }
                }
            }
        }

        let mut secondary_icon_pixmap = None;
        if let Some(sec_info) = &secondary_info {
            if let Some(icon_path) = find_icon_path(&sec_info.icon_name) {
                if let Ok(svg_data) = std::fs::read(icon_path) {
                    if let Ok(tree) = Tree::from_data(&svg_data, &usvg::Options::default()) {
                        let icon_size = height as f64 * 0.7;
                        if let Some(mut pm) = Pixmap::new(icon_size as u32, icon_size as u32) {
                            let transform = Transform::from_scale(
                                icon_size as f32 / tree.size().width(),
                                icon_size as f32 / tree.size().height(),
                            );
                            resvg::render(&tree, transform, &mut pm.as_mut());
                            secondary_icon_pixmap = Some(Arc::new(pm));
                        }
                    }
                }
            }
        }

        let width = 3;
        let stride = Format::ARgb32.stride_for_width(width).unwrap();
        let mut texture_data = vec![0; (stride * height) as usize];
        let line_color = [0x33, 0x33, 0x33, 0xFF];
        for y in 0..height {
            let line_start = (y * stride) as usize;
            let pixel_start = line_start + 1 * 4;
            texture_data[pixel_start..pixel_start + 4].copy_from_slice(&line_color);
        }

        DynamicDrawable::Media {
            primary_info,
            secondary_info,
            primary_icon_pixmap,
            secondary_icon_pixmap,
            scrubber_texture_data: Some(Arc::new(texture_data)),
        }
    }
}
