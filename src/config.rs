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

#[derive(Debug, Deserialize)]
pub struct ButtonConfig {
    pub text: Option<String>,
    pub icon: Option<String>,
    pub action: String,
    pub width: f64,
}
