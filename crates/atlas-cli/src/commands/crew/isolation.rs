//! The server side of crewmate isolation (issue #200): the units of the root spawn helper and the
//! shell that creates and removes one Linux user per crewmate. Pure functions, like `layout`: text
//! goes in and out, so every line is tested without ssh or root.
//!
//! The helper (`atlas crew-spawn`, protocol in `atlas_host::crew::spawn`) is the only root process.
//! It is socket-activated, so nothing runs until hostd connects, and it can only do what its
//! validators allow: start an engine binary as `atlas-cm-<slug>`. Users are *not* created by it:
//! `useradd` rewrites `/etc/passwd`, which the helper's read-only filesystem and short capability
//! list could not do (and should not), so provisioning is a step the installer runs with sudo.
//!
//! Stage 1 of #200: nothing calls these yet. The installer does not enable the socket until the
//! `atlas crew-spawn` subcommand exists (a socket whose service binary lacks the command would
//! break installs); it lands with the stage that routes hostd through the helper.
#![allow(dead_code)]

use atlas_host::crew::spawn::validate::{CM_ROOT, USER_PREFIX, slug_of};

use super::layout::{Account, INSTALL_DIR};

/// systemd unit names (without suffix) of the helper.
pub const SPAWN_UNIT: &str = "atlas-crew-spawn";
/// The socket hostd connects to.
pub const SOCKET_PATH: &str = "/run/atlas-crew/spawn.sock";

/// `atlas-crew-spawn.socket`: a unix socket only the crew user (and root) can open.
pub fn socket_unit(a: &Account) -> String {
    format!(
        "[Unit]\n\
Description=Atlas Crew spawn helper socket\n\
[Socket]\n\
ListenStream={SOCKET_PATH}\n\
SocketUser=root\n\
SocketGroup={group}\n\
SocketMode=0660\n\
DirectoryMode=0750\n\
[Install]\n\
WantedBy=sockets.target\n",
        group = a.user
    )
}

/// `atlas-crew-spawn.service`: runs as root, started by the socket, with the least that spawning
/// as another user needs.
///
/// - `CAP_SETUID`/`CAP_SETGID` switch to the crewmate's user, `CAP_CHOWN` gives it its pty slave,
///   `CAP_KILL` signals its processes, `CAP_DAC_OVERRIDE` lets root reach directories that are
///   0700 for the crewmate or the crew user. Nothing else is in the bounding set.
/// - `NoNewPrivileges=yes` is fine here: the helper changes uid with `setuid(2)`, never by running
///   a setuid binary.
/// - `KillMode=process`: restarting the helper must not kill the agents it started.
///   Uninstall stops it with `--kill-who=all` semantics (see [`uninstall_script`]).
/// - `ProtectSystem=strict` with only `/var/lib/atlas-cm` writable.
pub fn service_unit() -> String {
    format!(
        "[Unit]\n\
Description=Atlas Crew spawn helper (starts each crewmate as its own user)\n\
Requires={SPAWN_UNIT}.socket\n\
After={SPAWN_UNIT}.socket\n\
[Service]\n\
Type=simple\n\
User=root\n\
ExecStart={INSTALL_DIR}/atlas crew-spawn\n\
KillMode=process\n\
Restart=on-failure\n\
NoNewPrivileges=yes\n\
CapabilityBoundingSet=CAP_SETUID CAP_SETGID CAP_CHOWN CAP_KILL CAP_DAC_OVERRIDE\n\
ProtectSystem=strict\n\
ReadWritePaths={CM_ROOT}\n\
ProtectHome=yes\n\
PrivateTmp=yes\n\
ProtectKernelTunables=yes\n\
ProtectKernelModules=yes\n\
ProtectControlGroups=yes\n\
RestrictSUIDSGID=yes\n\
RestrictAddressFamilies=AF_UNIX\n\
LockPersonality=yes\n\
MemoryDenyWriteExecute=yes\n"
    )
}

/// Why a script was not built: the user is not an `atlas-cm-<12 hex>` name. Everything below
/// interpolates it into shell, so it is checked with the helper's own rule first.
#[derive(Debug, PartialEq, Eq)]
pub struct BadUser(pub String);

fn slug(user: &str) -> Result<&str, BadUser> {
    slug_of(user).map_err(|_| BadUser(user.chars().take(64).collect()))
}

/// Steps, in order, that prepare a server for per-crewmate users: the crew home closed to every
/// other user, the parent of the crewmates' directories (traversable by all, listable by none, so
/// a crewmate can reach only its own directory by name), the two units, and the socket started.
/// `unit_tee` runs the install of one unit file; the caller supplies the contents (see
/// [`socket_unit`], [`service_unit`]) on stdin for each.
pub fn install_steps(a: &Account) -> Vec<String> {
    vec![
        // The crew home held every crewmate's data when they all shared one user; with one user per
        // crewmate it must not be traversable by them. `useradd --create-home` leaves it 0755.
        format!("sudo chmod 0700 '{}'", a.home),
        format!("sudo install -d -m 0711 -o root -g root '{CM_ROOT}'"),
    ]
}

/// The two writes of the units, as `(file, contents)`; the caller pipes the contents to
/// `sudo tee <file>`.
pub fn unit_files(a: &Account) -> [(String, String); 2] {
    [
        (
            format!("/etc/systemd/system/{SPAWN_UNIT}.socket"),
            socket_unit(a),
        ),
        (
            format!("/etc/systemd/system/{SPAWN_UNIT}.service"),
            service_unit(),
        ),
    ]
}

/// Steps that start the helper after its units are written. The socket is enabled, never the
/// service: systemd starts the service on the first connection.
pub fn start_steps() -> Vec<String> {
    vec![
        "sudo systemctl daemon-reload".into(),
        format!("sudo systemctl enable {SPAWN_UNIT}.socket"),
        // `restart` so a re-install over a running helper picks up new units; agents survive it
        // (`KillMode=process`).
        format!("sudo systemctl restart {SPAWN_UNIT}.socket"),
    ]
}

/// Shell that creates the Linux user of one crewmate, idempotently, and its directories:
///
/// ```text
/// /var/lib/atlas-cm/<slug>/        user:user 0750   (the crew user is in the group: traverse only)
///   home/                          user:user 0700   $HOME: the engines' sign-ins live here
///   work/                          user:user 2770   setgid; the crew user, in the group, writes it
///   crew-runtime/                  root:user 0750   what the agent reads and must not change
/// ```
///
/// `useradd --system` allocates from the system uid range (about 900 accounts): a failure there
/// is printed as such, and the script exits non-zero.
pub fn provision_script(a: &Account, user: &str) -> Result<String, BadUser> {
    let slug = slug(user)?;
    let dir = format!("{CM_ROOT}/{slug}");
    Ok(format!(
        "set -e; \
if ! id {user} >/dev/null 2>&1; then \
sudo useradd --system --user-group --no-create-home --home-dir {dir}/home --shell /usr/sbin/nologin {user} \
|| {{ echo 'could not create {user}: out of system user ids? (useradd --system has about 900)' >&2; exit 1; }}; fi; \
sudo usermod -aG {user} {crew}; \
sudo install -d -m 0750 -o {user} -g {user} {dir}; \
sudo install -d -m 0700 -o {user} -g {user} {dir}/home; \
sudo install -d -m 2770 -o {user} -g {user} {dir}/work; \
sudo install -d -m 0750 -o root -g {user} {dir}/crew-runtime",
        crew = a.user
    ))
}

/// Shell that removes one crewmate's Linux user: its processes first (`userdel` refuses a user
/// with running processes), then the account and its group. `purge` also deletes its directory
/// (sign-ins, work tree); without it the files stay, owned by a uid that is about to be reused.
pub fn deprovision_script(user: &str, purge: bool) -> Result<String, BadUser> {
    let slug = slug(user)?;
    let rm = if purge {
        format!("; sudo rm -rf {CM_ROOT}/{slug}")
    } else {
        String::new()
    };
    Ok(format!(
        "if id {user} >/dev/null 2>&1; then \
sudo pkill -KILL -u {user} 2>/dev/null || true; \
sudo userdel {user}; fi{rm}"
    ))
}

/// Shell that removes the helper and every crewmate user, for `atlas crew uninstall`. Runs before
/// the crew user is deleted. `KillMode=process` means stopping the helper leaves the agents it
/// started alive, so every `atlas-cm-*` user's processes are killed explicitly first.
pub fn uninstall_script() -> String {
    format!(
        "for u in $(getent passwd | cut -d: -f1 | grep -E '^{USER_PREFIX}[0-9a-f]{{12}}$'); do \
sudo pkill -KILL -u \"$u\" 2>/dev/null || true; sudo userdel \"$u\" 2>/dev/null || true; done; \
sudo systemctl stop {SPAWN_UNIT}.socket {SPAWN_UNIT}.service 2>/dev/null || true; \
sudo systemctl disable {SPAWN_UNIT}.socket 2>/dev/null || true; \
sudo rm -f /etc/systemd/system/{SPAWN_UNIT}.socket /etc/systemd/system/{SPAWN_UNIT}.service; \
sudo rm -rf {CM_ROOT}; \
sudo systemctl daemon-reload 2>/dev/null || true"
    )
}

#[cfg(test)]
mod tests {
    use super::super::layout::{Probe, account};
    use super::*;

    fn acct() -> Account {
        account(Probe::default())
    }

    fn section<'a>(unit: &'a str, name: &str) -> &'a str {
        let rest = unit.split(&format!("[{name}]\n")).nth(1).unwrap();
        let end = rest.find("\n[").map_or(rest.len(), |i| i + 1);
        &rest[..end]
    }

    // Issue #200: only the crew user may reach the helper. SocketMode is what keeps every
    // crewmate user (a different uid, not in the group) from even connecting.
    #[test]
    fn the_socket_is_for_the_crew_group_only_and_never_world_accessible() {
        let u = socket_unit(&acct());
        let s = section(&u, "Socket");
        for want in [
            "ListenStream=/run/atlas-crew/spawn.sock\n",
            "SocketUser=root\n",
            "SocketGroup=atlas-crew\n",
            "SocketMode=0660\n",
            "DirectoryMode=0750\n",
        ] {
            assert!(s.contains(want), "missing {want:?} in {s}");
        }
        assert!(u.contains("WantedBy=sockets.target"));
        // A migrated server keeps its user, and the group follows it.
        let old = socket_unit(&account(Probe {
            legacy_user: true,
            ..Probe::default()
        }));
        assert!(old.contains("SocketGroup=atlas-bot\n"));
    }

    #[test]
    fn the_service_is_root_socket_activated_and_does_not_kill_agents_on_restart() {
        let u = service_unit();
        let s = section(&u, "Service");
        for want in [
            "User=root\n",
            "ExecStart=/usr/local/lib/atlas-crew/atlas crew-spawn\n",
            "KillMode=process\n",
            "NoNewPrivileges=yes\n",
        ] {
            assert!(s.contains(want), "missing {want:?}");
        }
        assert!(section(&u, "Unit").contains("Requires=atlas-crew-spawn.socket\n"));
        assert!(
            !u.contains("[Install]"),
            "socket-activated: the service is never enabled itself"
        );
    }

    // The bounding set is the whole point of "hardened as far as compatible": exactly these five,
    // and none of the capabilities that would let a bug in the helper do more.
    #[test]
    fn the_service_keeps_five_capabilities_and_a_read_only_system() {
        let u = service_unit();
        let caps = u
            .lines()
            .find_map(|l| l.strip_prefix("CapabilityBoundingSet="))
            .unwrap();
        let mut got: Vec<_> = caps.split(' ').collect();
        got.sort();
        assert_eq!(
            got,
            [
                "CAP_CHOWN",
                "CAP_DAC_OVERRIDE",
                "CAP_KILL",
                "CAP_SETGID",
                "CAP_SETUID"
            ]
        );
        for bad in [
            "CAP_SYS_ADMIN",
            "CAP_NET_ADMIN",
            "CAP_SYS_PTRACE",
            "CAP_FOWNER",
        ] {
            assert!(!caps.contains(bad));
        }
        for want in [
            "ProtectSystem=strict\n",
            "ReadWritePaths=/var/lib/atlas-cm\n",
            "ProtectHome=yes\n",
            "RestrictAddressFamilies=AF_UNIX\n",
            "RestrictSUIDSGID=yes\n",
        ] {
            assert!(u.contains(want), "missing {want:?}");
        }
    }

    // Every directive must exist in systemd 245 (Ubuntu 20.04), the install floor. Listing them
    // here makes adding a newer one a deliberate decision: ProtectProc 247, ProtectHome=tmpfs 242
    // would silently break the unit on focal.
    #[test]
    fn every_directive_exists_in_systemd_245() {
        const KNOWN_245: &[&str] = &[
            "Description",
            "Requires",
            "After",
            "Type",
            "User",
            "ExecStart",
            "KillMode",
            "Restart",
            "NoNewPrivileges",
            "CapabilityBoundingSet",
            "ProtectSystem",
            "ReadWritePaths",
            "ProtectHome",
            "PrivateTmp",
            "ProtectKernelTunables",
            "ProtectKernelModules",
            "ProtectControlGroups",
            "RestrictSUIDSGID",
            "RestrictAddressFamilies",
            "LockPersonality",
            "MemoryDenyWriteExecute",
            "ListenStream",
            "SocketUser",
            "SocketGroup",
            "SocketMode",
            "DirectoryMode",
            "WantedBy",
        ];
        for unit in [service_unit(), socket_unit(&acct())] {
            for line in unit.lines().filter(|l| l.contains('=')) {
                let key = line.split('=').next().unwrap();
                assert!(
                    KNOWN_245.contains(&key),
                    "{key} is not known to exist in systemd 245"
                );
            }
        }
    }

    const USER: &str = "atlas-cm-3f2a9c1e5b7d";

    // The user ends up inside `sudo ...` shell: anything that is not the helper's own user shape
    // must stop here, whatever a caller passes. This is the injection guard of the scripts.
    #[test]
    fn scripts_refuse_a_user_that_is_not_a_provisioned_name() {
        let a = acct();
        for bad in [
            "root",
            "",
            "atlas-cm-",
            "atlas-crew",
            "atlas-cm-3f2a9c1e5b7d; rm -rf /",
            "atlas-cm-$(id)aaaa",
            "atlas-cm-../../etc/x",
            "atlas-cm-3F2A9C1E5B7D",
            "atlas-cm-3f2a9c1e5b7d\n",
            "atlas-cm-3f2a9c1e5b7",
        ] {
            assert!(provision_script(&a, bad).is_err(), "{bad:?}");
            assert!(deprovision_script(bad, true).is_err(), "{bad:?}");
        }
        assert!(provision_script(&a, USER).is_ok());
    }

    #[test]
    fn provisioning_makes_the_user_its_group_and_a_private_tree_for_it() {
        let sh = provision_script(&acct(), USER).unwrap();
        for want in [
            "set -e",
            "id atlas-cm-3f2a9c1e5b7d >/dev/null 2>&1",
            "useradd --system --user-group --no-create-home --home-dir /var/lib/atlas-cm/3f2a9c1e5b7d/home --shell /usr/sbin/nologin atlas-cm-3f2a9c1e5b7d",
            "usermod -aG atlas-cm-3f2a9c1e5b7d atlas-crew",
            "install -d -m 0750 -o atlas-cm-3f2a9c1e5b7d -g atlas-cm-3f2a9c1e5b7d /var/lib/atlas-cm/3f2a9c1e5b7d;",
            "install -d -m 0700 -o atlas-cm-3f2a9c1e5b7d -g atlas-cm-3f2a9c1e5b7d /var/lib/atlas-cm/3f2a9c1e5b7d/home",
            "install -d -m 2770 -o atlas-cm-3f2a9c1e5b7d -g atlas-cm-3f2a9c1e5b7d /var/lib/atlas-cm/3f2a9c1e5b7d/work",
            "install -d -m 0750 -o root -g atlas-cm-3f2a9c1e5b7d /var/lib/atlas-cm/3f2a9c1e5b7d/crew-runtime",
            "out of system user ids",
        ] {
            assert!(sh.contains(want), "missing {want:?}:\n{sh}");
        }
        // Idempotent: the useradd sits inside the "not there yet" branch, the rest is install -d.
        assert!(sh.find("if ! id").unwrap() < sh.find("useradd").unwrap());
        assert!(sh.find("fi;").unwrap() < sh.find("usermod").unwrap());
        // No shell, no login: nobody can ssh or `su` into a crewmate.
        assert!(sh.contains("/usr/sbin/nologin") && !sh.contains("/bin/bash"));
    }

    // The crew user of a migrated server is the one added to the crewmate's group.
    #[test]
    fn provisioning_adds_the_servers_own_crew_user_to_the_group() {
        let old = account(Probe {
            legacy_user: true,
            ..Probe::default()
        });
        let sh = provision_script(&old, USER).unwrap();
        assert!(sh.contains("usermod -aG atlas-cm-3f2a9c1e5b7d atlas-bot"));
        assert!(!sh.contains("atlas-crew"));
    }

    #[test]
    fn deprovisioning_kills_its_processes_before_userdel_and_keeps_files_unless_purged() {
        let keep = deprovision_script(USER, false).unwrap();
        assert!(
            keep.find("pkill -KILL -u atlas-cm-3f2a9c1e5b7d")
                < keep.find("userdel atlas-cm-3f2a9c1e5b7d")
        );
        assert!(!keep.contains("rm -rf"));
        let purge = deprovision_script(USER, true).unwrap();
        assert!(
            purge.ends_with("; sudo rm -rf /var/lib/atlas-cm/3f2a9c1e5b7d"),
            "{purge}"
        );
        // Gone already: the check around the kill makes a second run a no-op, not an error.
        assert!(purge.starts_with("if id atlas-cm-3f2a9c1e5b7d"));
    }

    // Nothing may be traversable by a crewmate that holds another's data: this is the first of
    // the install steps, and the order is the contract (close the old home, then open the new root).
    #[test]
    fn install_closes_the_crew_home_and_opens_only_the_crewmate_parent_for_traversal() {
        let steps = install_steps(&acct());
        assert_eq!(steps[0], "sudo chmod 0700 '/var/lib/atlas-crew'");
        assert_eq!(
            steps[1],
            "sudo install -d -m 0711 -o root -g root '/var/lib/atlas-cm'"
        );
        let old = install_steps(&account(Probe {
            legacy_user: true,
            ..Probe::default()
        }));
        assert_eq!(old[0], "sudo chmod 0700 '/var/lib/atlas-bot'");
    }

    #[test]
    fn units_are_written_to_etc_systemd_and_only_the_socket_is_enabled() {
        let files = unit_files(&acct());
        assert_eq!(files[0].0, "/etc/systemd/system/atlas-crew-spawn.socket");
        assert_eq!(files[1].0, "/etc/systemd/system/atlas-crew-spawn.service");
        assert_eq!(files[0].1, socket_unit(&acct()));
        assert_eq!(files[1].1, service_unit());
        let steps = start_steps();
        assert!(steps[0].ends_with("daemon-reload"));
        assert!(
            steps
                .iter()
                .any(|s| s.ends_with("enable atlas-crew-spawn.socket"))
        );
        assert!(
            steps
                .iter()
                .all(|s| !s.contains("enable atlas-crew-spawn.service"))
        );
        assert!(
            steps
                .last()
                .unwrap()
                .ends_with("restart atlas-crew-spawn.socket")
        );
    }

    // KillMode=process means `systemctl stop` leaves agents running: uninstall must kill every
    // crewmate user's processes itself, and only match the helper's own user shape.
    #[test]
    fn uninstall_kills_every_crewmate_before_stopping_the_helper_and_removes_everything() {
        let sh = uninstall_script();
        assert!(sh.contains(r"grep -E '^atlas-cm-[0-9a-f]{12}$'"), "{sh}");
        assert!(sh.find("pkill").unwrap() < sh.find("userdel").unwrap());
        assert!(sh.find("userdel").unwrap() < sh.find("systemctl stop").unwrap());
        for want in [
            "systemctl stop atlas-crew-spawn.socket atlas-crew-spawn.service",
            "rm -f /etc/systemd/system/atlas-crew-spawn.socket /etc/systemd/system/atlas-crew-spawn.service",
            "rm -rf /var/lib/atlas-cm",
        ] {
            assert!(sh.contains(want), "missing {want:?}");
        }
        // Never a bare `pkill` of a pattern or a user glob that could match someone else's.
        assert!(!sh.contains("pkill -f") && !sh.contains("atlas-cm-*"));
    }
}
