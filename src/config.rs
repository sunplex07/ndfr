use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Layout {
    pub left: ButtonGroup,
    pub right: ButtonGroup,
}

#[derive(Debug, Deserialize)]
pub struct ButtonGroup {
    pub spacing: f64,
    pub buttons: Vec<ButtonConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub enum ButtonRenderMode {
    Mask,
    Color,
}

impl Default for ButtonRenderMode {
    fn default() -> Self {
        ButtonRenderMode::Mask
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ButtonConfig {
    pub text: Option<String>,
    pub icon: Option<String>,
    pub action: String,
    pub width: f64,
    #[serde(default)]
    pub render_mode: ButtonRenderMode,
}