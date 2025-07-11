use anyhow::{anyhow, Result};
use std::process::Command;
use std::env;

enum AudioBackend {
    PipeWire,
    PulseAudio,
}

pub struct Volume {
    backend: AudioBackend,
}

fn create_command(program: &str) -> Command {
    if let (Ok(sudo_user), Ok(sudo_uid)) = (env::var("SUDO_USER"), env::var("SUDO_UID")) {
        let mut command = Command::new("sudo");
        command.arg("-u").arg(sudo_user);
        command.arg("env");
        command.arg(format!("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{}/bus", sudo_uid));
        command.arg(format!("XDG_RUNTIME_DIR=/run/user/{}", sudo_uid));
        command.arg(program);
        command
    } else {
        Command::new(program)
    }
}

// controlling audio directly from Rust is finnicky, therefore relies on wpctl or pactl.
impl Volume {
    pub fn new() -> Result<Self> {
        let backend = if create_command("wpctl").arg("--version").output().map_or(false, |o| o.status.success()) {
            println!("[volume] Using PipeWire backend (wpctl)");
            AudioBackend::PipeWire
        } else if create_command("pactl").arg("--version").output().map_or(false, |o| o.status.success()) {
            println!("[volume] Using PulseAudio backend (pactl)");
            AudioBackend::PulseAudio
        } else {
            return Err(anyhow!("No suitable audio backend found. Please install 'wpctl' (pipewire-bin) or 'pactl' (pulseaudio)."));
        };
        Ok(Volume { backend })
    }

    pub fn set_volume(&self, value: f64) -> Result<()> {
        let value = value.max(0.0).min(1.5); // overamp max %150
        let output = match self.backend {
            AudioBackend::PipeWire => {
                create_command("wpctl")
                    .arg("set-volume")
                    .arg("@DEFAULT_AUDIO_SINK@")
                    .arg(format!("{:.2}", value))
                    .output()?
            }
            AudioBackend::PulseAudio => {
                create_command("pactl")
                    .arg("set-sink-volume")
                    .arg("@DEFAULT_SINK@")
                    .arg(format!("{}%", (value * 100.0).round() as u32))
                    .output()?
            }
        };

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to set volume: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    pub fn get_volume(&self) -> Result<f64> {
        let output = match self.backend {
            AudioBackend::PipeWire => {
                create_command("wpctl")
                    .arg("get-volume")
                    .arg("@DEFAULT_AUDIO_SINK@")
                    .output()?
            }
            AudioBackend::PulseAudio => {
                create_command("pactl")
                    .arg("get-sink-volume")
                    .arg("@DEFAULT_SINK@")
                    .output()?
            }
        };

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to get volume: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        match self.backend {
            AudioBackend::PipeWire => {
                stdout.split_whitespace().nth(1)
                    .ok_or_else(|| anyhow!("Failed to parse volume from wpctl output: unexpected format on '{}'", stdout))?
                    .parse::<f64>()
                    .map_err(|e| anyhow!("Failed to parse volume from wpctl output: {} on '{}'", e, stdout))
            }
            AudioBackend::PulseAudio => {
                let volume_str = stdout
                    .split('/')
                    .nth(1)
                    .and_then(|s| s.trim().strip_suffix('%'))
                    .ok_or_else(|| anyhow!("Failed to parse volume from pactl output: unexpected format on '{}'", stdout))?;
                
                let volume_percent: f64 = volume_str.trim().parse()?;
                Ok(volume_percent / 100.0)
            }
        }
    }
}
