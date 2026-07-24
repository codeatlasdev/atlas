use ratatui::style::Color;

use super::tokens::Theme;

pub const CODEATLAS: Theme = Theme {
    name: "codeatlas",
    // Brand
    primary: Color::Rgb(0x5B, 0x8D, 0xEF),
    secondary: Color::Rgb(0x1E, 0x40, 0xAF),
    accent: Color::Rgb(0x38, 0xBD, 0xF8),
    // Status
    error: Color::Rgb(0xEF, 0x44, 0x44),
    warning: Color::Rgb(0xF5, 0x9E, 0x0B),
    success: Color::Rgb(0x22, 0xC5, 0x5E),
    info: Color::Rgb(0x60, 0xA5, 0xFA),
    // Text
    text: Color::Rgb(0xFA, 0xFA, 0xFA),
    text_muted: Color::Rgb(0xA1, 0xA1, 0xAA),
    text_dim: Color::Rgb(0x71, 0x71, 0x7A),
    // Backgrounds
    bg: Color::Rgb(0x09, 0x09, 0x0B),
    bg_panel: Color::Rgb(0x18, 0x18, 0x1B),
    bg_float: Color::Rgb(0x27, 0x27, 0x2A),
    // Borders
    border: Color::Rgb(0x3F, 0x3F, 0x46),
    border_active: Color::Rgb(0x5B, 0x8D, 0xEF),
    border_subtle: Color::Rgb(0x27, 0x27, 0x2A),
    // Service status indicators
    status_running: Color::Rgb(0x22, 0xC5, 0x5E),
    status_stopped: Color::Rgb(0x71, 0x71, 0x7A),
    status_starting: Color::Rgb(0xF5, 0x9E, 0x0B),
    status_failed: Color::Rgb(0xEF, 0x44, 0x44),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codeatlas_theme_exists() {
        let theme = &CODEATLAS;
        assert_eq!(theme.name, "codeatlas");
    }

    #[test]
    fn test_codeatlas_all_colors_defined() {
        let theme = &CODEATLAS;

        // Brand
        assert!(matches!(theme.primary, Color::Rgb(0x5B, 0x8D, 0xEF)));
        assert!(matches!(theme.secondary, Color::Rgb(0x1E, 0x40, 0xAF)));
        assert!(matches!(theme.accent, Color::Rgb(0x38, 0xBD, 0xF8)));

        // Status
        assert!(matches!(theme.error, Color::Rgb(0xEF, 0x44, 0x44)));
        assert!(matches!(theme.warning, Color::Rgb(0xF5, 0x9E, 0x0B)));
        assert!(matches!(theme.success, Color::Rgb(0x22, 0xC5, 0x5E)));
        assert!(matches!(theme.info, Color::Rgb(0x60, 0xA5, 0xFA)));

        // Text
        assert!(matches!(theme.text, Color::Rgb(0xFA, 0xFA, 0xFA)));
        assert!(matches!(theme.text_muted, Color::Rgb(0xA1, 0xA1, 0xAA)));
        assert!(matches!(theme.text_dim, Color::Rgb(0x71, 0x71, 0x7A)));

        // Backgrounds
        assert!(matches!(theme.bg, Color::Rgb(0x09, 0x09, 0x0B)));
        assert!(matches!(theme.bg_panel, Color::Rgb(0x18, 0x18, 0x1B)));
        assert!(matches!(theme.bg_float, Color::Rgb(0x27, 0x27, 0x2A)));

        // Borders
        assert!(matches!(theme.border, Color::Rgb(0x3F, 0x3F, 0x46)));
        assert!(matches!(theme.border_active, Color::Rgb(0x5B, 0x8D, 0xEF)));
        assert!(matches!(theme.border_subtle, Color::Rgb(0x27, 0x27, 0x2A)));

        // Service status
        assert!(matches!(theme.status_running, Color::Rgb(0x22, 0xC5, 0x5E)));
        assert!(matches!(theme.status_stopped, Color::Rgb(0x71, 0x71, 0x7A)));
        assert!(matches!(theme.status_starting, Color::Rgb(0xF5, 0x9E, 0x0B)));
        assert!(matches!(theme.status_failed, Color::Rgb(0xEF, 0x44, 0x44)));
    }
}
