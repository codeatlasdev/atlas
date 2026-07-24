mod codeatlas;
mod tokens;

pub use codeatlas::CODEATLAS;
pub use tokens::Theme;

/// Returns the current active theme.
pub fn current() -> &'static Theme {
    &CODEATLAS
}
