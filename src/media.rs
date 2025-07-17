use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
    #[serde(other)]
    Unknown,
}

impl Default for PlaybackStatus {
    fn default() -> Self {
        PlaybackStatus::Unknown
    }
}


#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub struct MediaInfo {
    #[serde(default)]
    pub player_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub status: PlaybackStatus,
    #[serde(default, rename = "position")]
    position_usecs: i64,
    #[serde(default, rename = "length")]
    duration_usecs: i64,
    #[serde(default, rename = "icon")]
    pub icon_name: String,
}

impl MediaInfo {
    pub fn position_s(&self) -> f64 {
        self.position_usecs as f64 / 1_000_000.0
    }
    pub fn position_usecs(&self) -> i64 {
        self.position_usecs
    }
    pub fn duration_s(&self) -> f64 {
        self.duration_usecs as f64 / 1_000_000.0
    }
    pub fn set_position(&mut self, pos_usecs: i64) {
        self.position_usecs = pos_usecs;
    }
}
