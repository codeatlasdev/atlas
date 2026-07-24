use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    // Brand
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    // Status
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,
    // Text
    pub text: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    // Backgrounds
    pub bg: Color,
    pub bg_panel: Color,
    pub bg_float: Color,
    // Borders
    pub border: Color,
    pub border_active: Color,
    pub border_subtle: Color,
    // Service status indicators
    pub status_running: Color,
    pub status_stopped: Color,
    pub status_starting: Color,
    pub status_failed: Color,
}
