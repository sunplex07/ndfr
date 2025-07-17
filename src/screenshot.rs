use crate::app::AppState;
use crate::renderer;
use crate::ui::Page;
use anyhow::{anyhow, Result};
use cairo::{Format, ImageSurface};
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

fn get_screenshot_path() -> Result<PathBuf> {
    let pictures_dir = dirs::picture_dir()
    .or_else(dirs::home_dir)
    .ok_or_else(|| anyhow!("Could not find a suitable directory for screenshots."))?;

    let now = chrono::Local::now();
    let filename = now
    .format("Screen Shot %Y-%m-%d at %I.%M.%S %p.png")
    .to_string();

    Ok(pictures_dir.join(filename))
}

pub fn take_screenshot(state: &AppState) -> Result<()> {
    let path = get_screenshot_path()?;
    println!("[screenshot] Saving to {}", path.display());

    let (width, height) = (state.width, state.height);
    let mut surface = ImageSurface::create(Format::ARgb32, width, height)?;

    let animation_progress = state.get_animation_progress();

    let dynamic_content = if let Page::Default(layout) = &state.page {
        if Arc::ptr_eq(layout, &state.default_layout) {
            Some((&state.dynamic_drawable, &state.default_dynamic_area_bounds))
        } else {
            None
        }
    } else {
        None
    };

    renderer::draw_ui(&mut surface, &state.page, &state.gesture, dynamic_content, animation_progress, true)?;

    let mut file = File::create(path)?;
    surface.write_to_png(&mut file)?;

    println!("[screenshot] Screenshot saved successfully.");
    Ok(())
}
