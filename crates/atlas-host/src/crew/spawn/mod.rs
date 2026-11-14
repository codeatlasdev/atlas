//! The crew spawn helper: how hostd, which stays the unprivileged `atlas-crew` user under
//! `NoNewPrivileges`, starts each crewmate as its own Linux user (`atlas-cm-<slug>`). Issue #200.
//!
//! A small root process (`atlas crew-spawn`, socket-activated by `atlas-crew-spawn.socket`) is the
//! only thing that can change uid. hostd asks it over a unix socket; the helper forks, drops to the
//! crewmate's user and execs an allowed engine binary, then hands the pty master back over
//! `SCM_RIGHTS` (the same mechanism as the live-upgrade handoff). hostd keeps the master and never
//! gains a privilege.
//!
//! | module     | what it owns                                                            |
//! |------------|-------------------------------------------------------------------------|
//! | `protocol` | request/reply types, the message size cap                                |
//! | `validate` | the pure rules every request must pass (user, binary, cwd, env, signal)  |
//! | `wire`     | length-prefixed JSON framing with fds                                    |
//! | `exec`     | Linux only: fork, drop to the crewmate's user, exec, return the pty master |
//! | `peer`     | who is on the other end of the socket (`SO_PEERCRED`)                    |
//!
//! Not wired into hostd yet: stage 2 routes `pty/spawn.rs`, signals and the engines through it.

#[cfg(target_os = "linux")]
pub mod exec;
pub mod peer;
pub mod protocol;
pub mod validate;
pub mod wire;
