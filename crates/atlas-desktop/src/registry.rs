//! `~/.atlas/desktop/projects.json`: the projects shown in the sidebar. The registry itself lives in
//! `atlas_host::worktrees::registry`, shared with hostd (the phone creates and removes worktrees with
//! this app closed); this module keeps the desktop's old path.

pub use atlas_host::worktrees::registry::*;
