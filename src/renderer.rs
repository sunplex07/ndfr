use crate::app::Gesture;
use crate::dynamic::{DynamicDrawable, Rect};
use crate::ui::Page;
use anyhow::{anyhow, Result};
use cairo::ImageSurface;
use drm::control::{
    atomic, connector,
    dumbbuffer::DumbBuffer,
    framebuffer, property, AtomicCommitFlags, Device as ControlDevice, Mode,
};
use drm::Device as DrmDevice;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::io::AsFd,
    path::Path,
};

struct Card(File);
impl AsFd for Card {
    fn as_fd(&self) -> std::os::unix::io::BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl ControlDevice for Card {}
impl DrmDevice for Card {}

impl Card {
    fn open(path: &Path) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.read(true);
        options.write(true);
        Ok(Card(options.open(path)?))
    }
}

fn find_prop_id<T: drm::control::ResourceHandle>(
    card: &Card,
    handle: T,
    name: &'static str,
) -> Result<property::Handle> {
    let props = card.get_properties(handle)?;
    let (prop_ids, _) = props.as_props_and_values();
    for &prop_id in prop_ids {
        let info = card.get_property(prop_id)?;
        if info.name().to_str()? == name {
            return Ok(prop_id);
        }
    }
    Err(anyhow!("Could not find property '{}'", name))
}

pub struct DrmBackend {
    card: Card,
    pub mode: Mode,
    db: DumbBuffer,
    fb: framebuffer::Handle,
}

impl DrmBackend {
    pub fn new() -> Result<Self> {
        let mut errors = Vec::new();
        for entry in fs::read_dir("/dev/dri/")? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name().and_then(|s| s.to_str()).map_or(false, |s| s.starts_with("card")) {
                match DrmBackend::try_open_card(&path) {
                    Ok(card) => return Ok(card),
                    Err(err) => errors.push(format!("{}: {}", path.display(), err)),
                }
            }
        }
        Err(anyhow!(
            "No suitable TouchBar device found. Errors encountered:\n - {}",
            errors.join("\n - ")
        ))
    }

    pub fn present(&mut self, surface: &mut cairo::ImageSurface) -> Result<()> {
        let data = surface.data()?;
        let mut mapping = self.card.map_dumb_buffer(&mut self.db)?;
        mapping.as_mut()[..data.len()].copy_from_slice(&data);
        let clip = drm::control::ClipRect::new(0, 0, self.mode.size().0 as u16, self.mode.size().1 as u16);
        self.card.dirty_framebuffer(self.fb, &[clip])?;
        Ok(())
    }

    pub fn get_dimensions(&self) -> (i32, i32) {
        let (w, h) = self.mode.size();
        (w as i32, h as i32)
    }

    fn try_open_card(path: &Path) -> Result<Self> {
        let card = Card::open(path)?;
        card.set_client_capability(drm::ClientCapability::Atomic, true)?;
        card.acquire_master_lock()?;

        let res = card.resource_handles()?;
        let con_handle = res.connectors()
        .iter()
        .find(|con_handle| {
            card.get_connector(**con_handle, true)
            .map_or(false, |c| c.state() == connector::State::Connected)
        })
        .ok_or(anyhow!("No connected connector found"))?;
        let con = card.get_connector(*con_handle, true)?;

        let mode = *con.modes().first().ok_or(anyhow!("No modes found for connector"))?;
        let (disp_width, disp_height) = mode.size();

        if disp_height < disp_width * 5 {
            return Err(anyhow!("Device does not look like a TouchBar (aspect ratio check failed)"));
        }

        let crtc_handle = *res.crtcs().first().ok_or(anyhow!("No CRTCs found"))?;
        let plane_handle = *card.plane_handles()?.first().ok_or(anyhow!("No planes found"))?;

        let (db_width, db_height) = (mode.size().0 as u32, mode.size().1 as u32);
        let db = card.create_dumb_buffer((db_width, db_height), drm::buffer::DrmFourcc::Xrgb8888, 32)?;
        let fb = card.add_framebuffer(&db, 24, 32)?;

        let mut atomic_req = atomic::AtomicModeReq::new();
        let blob = card.create_property_blob(&mode)?;

        atomic_req.add_property(con.handle(), find_prop_id(&card, con.handle(), "CRTC_ID")?, property::Value::CRTC(Some(crtc_handle)));
        atomic_req.add_property(crtc_handle, find_prop_id(&card, crtc_handle, "MODE_ID")?, blob);
        atomic_req.add_property(crtc_handle, find_prop_id(&card, crtc_handle, "ACTIVE")?, property::Value::Boolean(true));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "FB_ID")?, property::Value::Framebuffer(Some(fb)));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "CRTC_ID")?, property::Value::CRTC(Some(crtc_handle)));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "SRC_X")?, property::Value::UnsignedRange(0));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "SRC_Y")?, property::Value::UnsignedRange(0));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "SRC_W")?, property::Value::UnsignedRange((db_width as u64) << 16));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "SRC_H")?, property::Value::UnsignedRange((db_height as u64) << 16));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "CRTC_X")?, property::Value::SignedRange(0));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "CRTC_Y")?, property::Value::SignedRange(0));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "CRTC_W")?, property::Value::UnsignedRange(db_width as u64));
        atomic_req.add_property(plane_handle, find_prop_id(&card, plane_handle, "CRTC_H")?, property::Value::UnsignedRange(db_height as u64));

        card.atomic_commit(AtomicCommitFlags::ALLOW_MODESET, atomic_req)?;

        Ok(DrmBackend { card, mode, db, fb })
    }
}

impl Drop for DrmBackend {
    fn drop(&mut self) {
        let _ = self.card.release_master_lock();
        let _ = self.card.destroy_framebuffer(self.fb);
        let _ = self.card.destroy_dumb_buffer(self.db);
    }
}

pub fn draw_ui(
    surface: &ImageSurface,
    page: &Page,
    gesture: &Gesture,
    dynamic_content: Option<(&DynamicDrawable, &Rect)>,
               animation_progress: f64,
               is_screenshot: bool,
) -> Result<()> {
    let c = cairo::Context::new(surface)?;
    let (width, height) = if is_screenshot {
        (surface.width() as f64, surface.height() as f64)
    } else {
        (surface.height() as f64, surface.width() as f64)
    };

    if !is_screenshot {
        c.translate(surface.width() as f64, 0.0);
        c.rotate(90.0f64.to_radians());
    }

    c.set_source_rgb(0.02, 0.02, 0.02);
    c.paint()?;

    let active_button_index = if let Gesture::ButtonDown { button_index } = gesture {
        Some(*button_index)
    } else {
        None
    };

    let is_scrubber_drag = matches!(gesture, Gesture::ScrubberDrag { .. });

    match page {
        Page::Default(buttons) => {
            if let Some((drawable, bounds)) = dynamic_content {
                drawable.draw(&c, bounds, is_scrubber_drag)?;
            }
            for (i, button) in buttons.iter().enumerate() {
                button.draw(&c, height, active_button_index == Some(i))?;
            }
        }
        Page::MediaInfoShowing(buttons) | Page::MediaInfoHiding(buttons) => {
            if let Some((drawable, bounds)) = dynamic_content {
                let mut animated_bounds = *bounds;
                animated_bounds.x = bounds.x + (bounds.width * (1.0 - animation_progress));
                c.save()?;
                c.rectangle(bounds.x, bounds.y, bounds.width, bounds.height);
                c.clip();
                drawable.draw(&c, &animated_bounds, is_scrubber_drag)?;
                c.restore()?;
            }
            for (i, button) in buttons.iter().enumerate() {
                button.draw(&c, height, active_button_index == Some(i))?;
            }
        }
        Page::FnKeys(buttons) => {
            for (i, button) in buttons.iter().enumerate() {
                button.draw(&c, height, active_button_index == Some(i))?;
            }
        }
        Page::BrightnessSlider(slider) | Page::BrightnessSliderClosing(slider) | Page::VolumeSlider(slider) | Page::VolumeSliderClosing(slider) => {
            slider.draw(&c, height, animation_progress)?;
        }
        Page::ControlStripExpanding(buttons) | Page::ControlStripClosing(buttons) => {
            let total_width: f64 = buttons.iter().map(|b| b.width).sum::<f64>() + (buttons.len() as f64 - 1.0) * 2.0;
            let start_x = width - total_width;
            for (i, button) in buttons.iter().enumerate() {
                let mut new_button = button.clone();
                new_button.x = start_x + (button.x - start_x) * animation_progress;
                new_button.draw(&c, height, active_button_index == Some(i))?;
            }
        }
    }

    Ok(())
}
