//! GPU terminal (wgpu: Metal on macOS). No Tauri here: the host app hands in a
//! native view, feeds pty bytes into `Vt`, and drives `Renderer` from a render thread.

pub mod colors;
mod font;
mod graphics;
pub mod input;
pub mod mouse;
mod render;
mod search;
pub mod vt;
pub mod wheel;

pub use alacritty_terminal::index::Side;
pub use alacritty_terminal::selection::SelectionType;
pub use render::{FontResolution, FontSpec, Renderer, Stats};
pub use vt::{
    Frame, HoverTarget, HyperlinkCell, ImageDraw, Link, LinkKind, SearchStat, SelectionState, Vt,
    VtEvent, WheelState,
};
