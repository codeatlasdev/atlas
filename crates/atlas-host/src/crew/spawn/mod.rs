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
//! | `layout`   | where a crewmate's files live under isolation, and when isolation is on  |
//! | `wire`     | length-prefixed JSON framing with fds                                    |
//! | `exec`     | fork, drop to the crewmate's user, exec, return the pty master           |
//! | `peer`     | who is on the other end of the socket (`SO_PEERCRED`)                    |
//! | `server`   | the helper's loop: sessions, signals, exit codes, `atlas crew-spawn`     |
//! | `client`   | hostd's side: ask the helper, get the master back                        |
//! | `provision`| the second root service: create/remove a crewmate's user, egress rules   |
//!
//! Everything that touches fds, sockets or users is Unix only; `protocol`, `validate` and `layout`
//! are plain data and compile everywhere (the CLI builds the install scripts from them on any OS).

#[cfg(unix)]
pub mod client;
#[cfg(unix)]
pub mod exec;
pub mod layout;
#[cfg(unix)]
pub mod peer;
pub mod protocol;
#[cfg(unix)]
pub mod provision;
#[cfg(unix)]
pub mod server;
#[cfg(all(test, unix))]
pub(crate) mod testing;
pub mod validate;
#[cfg(unix)]
pub mod wire;
