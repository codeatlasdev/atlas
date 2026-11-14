//! Git worktrees as registry entries (`parent` = the main checkout's id), done by hostd itself so Atlas
//! Mobile can list, create and remove them (and start an agent in one) with the desktop closed.
//!
//! The desktop uses the same pieces: the [`registry`] file both processes edit, [`naming`] for names and
//! port offsets, [`clean`] for "would removing this lose work?". [`local`] is hostd's own create/list/remove
//! of a checkout on this machine; it follows the layout and rules of the desktop's `worktree_create`.

pub mod clean;
pub mod local;
pub mod naming;
pub mod registry;
