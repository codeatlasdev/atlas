//! `atlas hostd` end-to-end: real binary, isolated `ATLAS_HOSTD_DIR`.

// Unix scripts (sh -c) and Unix-only live upgrade; Windows has hostd_windows.rs.
#![cfg(unix)]

mod agents;
mod cli;
mod common;
mod crew;
mod idle;
mod lab;
mod pty;
mod upgrade;
