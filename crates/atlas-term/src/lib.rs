//! GPU terminal (wgpu: Metal on macOS). No Tauri here: the host app hands in a
//! native view, feeds pty bytes into `Vt`, and drives `Renderer` from a render thread.

pub mod colors;
mod font;
mod graphics;
pub mod input;
mod render;
mod search;
pub mod vt;

pub use alacritty_terminal::index::Side;
pub use alacritty_terminal::selection::SelectionType;
pub use render::{Renderer, Stats};
pub use vt::{Frame, HyperlinkCell, ImageDraw, SearchStat, Vt, VtEvent};
