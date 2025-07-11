use anyhow::{anyhow, Result};
use std::process::Command;

pub struct Backlight {}

impl Backlight {
    pub fn new() -> Result<Self> {
        if !Command::new("/usr/bin/brightnessctl").arg("--version").output()?.status.success() {
            return Err(anyhow!("brightnessctl command not found at /usr/bin/brightnessctl. Please install it."));
        }
        Ok(Backlight {})
    }

    pub fn set_brightness(&self, value: f64) -> Result<()> {
        let percent = (value * 100.0).round() as u32;
        let status = Command::new("/usr/bin/brightnessctl")
            .arg("--quiet")
            .arg("set")
            .arg(format!("{}%", percent))
            .status()?;

        if !status.success() {
            return Err(anyhow!("Failed to set brightness using brightnessctl."));
        }
        Ok(())
    }

    pub fn get_brightness(&self) -> Result<f64> {
        let output = Command::new("/usr/bin/brightnessctl").arg("get").output()?;
        if !output.status.success() {
            return Err(anyhow!("Failed to get brightness from brightnessctl."));
        }
        let current_str = String::from_utf8_lossy(&output.stdout);
        let current: f64 = current_str.trim().parse()?;

        let max_output = Command::new("/usr/bin/brightnessctl").arg("max").output()?;
        if !max_output.status.success() {
            return Err(anyhow!("Failed to get max brightness from brightnessctl."));
        }
        let max_str = String::from_utf8_lossy(&max_output.stdout);
        let max: f64 = max_str.trim().parse()?;

        if max == 0.0 {
            Ok(0.0)
        } else {
            Ok(current / max)
        }
    }
}