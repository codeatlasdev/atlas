//! The server side of crewmate isolation (issue #200): the units of the root spawn helper and the
//! shell that creates and removes one Linux user per crewmate. Pure functions, like `layout`: text
//! goes in and out, so every line is tested without ssh or root.
//!
//! Two root services, both `atlas crew-spawn`, both socket-activated (nothing runs until hostd
//! connects) and both serving only the crew user:
//!
//! - the **spawn helper** (`atlas-crew-spawn`) can only start an engine binary as `atlas-cm-<slug>`,
//!   signal it and read its exit; five capabilities, a read-only filesystem.
//! - the **provisioning service** (`atlas-crew-provision`, `crew-spawn --provision`) creates and removes
//!   those users and rewrites the egress rules. It needs to write `/etc`, so it is a separate process
//!   with a protocol of two operations and no free text: a bug in the spawn path cannot become "write
//!   any system file".
//!
//! The installer runs the same steps with sudo for the first install and for crewmates that already
//! exist ([`provision_script`], [`migrate_script`]).
//!
use atlas_host::crew::spawn::layout::{EGRESS_FILE, PROVISION_SOCKET_PATH, SOCKET_PATH};
use atlas_host::crew::spawn::validate::{CM_ROOT, USER_PREFIX, slug_of};

use super::layout::{Account, INSTALL_DIR};

/// systemd unit names (without suffix) of the spawn helper.
pub const SPAWN_UNIT: &str = "atlas-crew-spawn";
/// ... of the provisioning service.
pub const PROVISION_UNIT: &str = "atlas-crew-provision";
/// ... of the oneshot that loads the egress rules at boot.
pub const EGRESS_UNIT: &str = "atlas-crew-egress";

/// The socket unit of `unit` (`atlas-crew-spawn` or `atlas-crew-provision`): a unix socket only the crew
/// user (and root) can open. Its group is the crew user's: the service reads the crew user's name from it.
fn socket_unit_of(unit_desc: &str, path: &str, a: &Account) -> String {
    format!(
        "[Unit]\n\
Description={unit_desc}\n\
[Socket]\n\
ListenStream={path}\n\
SocketUser=root\n\
SocketGroup={group}\n\
SocketMode=0660\n\
DirectoryMode=0755\n\
[Install]\n\
WantedBy=sockets.target\n",
        group = a.user
    )
}

/// `atlas-crew-spawn.socket`.
pub fn socket_unit(a: &Account) -> String {
    socket_unit_of("Atlas Crew spawn helper socket", SOCKET_PATH, a)
}

/// `atlas-crew-provision.socket`.
pub fn provision_socket_unit(a: &Account) -> String {
    socket_unit_of("Atlas Crew provisioning socket", PROVISION_SOCKET_PATH, a)
}

/// `atlas-crew-spawn.service`: runs as root, started by the socket, with the least that spawning
/// as another user needs.
///
/// - `CAP_SETUID`/`CAP_SETGID` switch to the crewmate's user, `CAP_CHOWN` gives it its pty slave,
///   `CAP_KILL` signals its processes, `CAP_DAC_OVERRIDE` lets root reach directories that are
///   0700 for the crewmate or the crew user. Nothing else is in the bounding set.
/// - No `NoNewPrivileges=`: with the other sandboxing options it left the helper without `CAP_SETUID` (systemd 255),
///   so every spawn failed at `setuid`; each child sets no_new_privs on itself instead. The helper changes uid with `setuid(2)`, never by running
///   a setuid binary.
/// - `KillMode=process`: restarting the helper must not kill the agents it started.
///   Uninstall stops it with `--kill-who=all` semantics (see [`uninstall_script`]).
/// - `ProtectSystem=strict` with only `/var/lib/atlas-cm` writable.
/// - A memory cap per crewmate (#296): `Delegate=memory pids` gives the helper its own cgroup subtree,
///   `DelegateSubgroup=helper` (systemd 254; older versions ignore it and the helper moves itself down)
///   keeps the helper process out of the unit's cgroup so that cgroup can have one leaf per crewmate.
///   `ProtectControlGroups=yes` stays, with only the unit's own cgroup writable again ([`cgroup_path`]);
///   the `-` prefix makes a missing path a no-op instead of a unit that cannot start.
pub fn service_unit() -> String {
    let cg = cgroup_path();
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
CapabilityBoundingSet=CAP_SETUID CAP_SETGID CAP_CHOWN CAP_KILL CAP_DAC_OVERRIDE\n\
ProtectSystem=strict\n\
ReadWritePaths={CM_ROOT} -{cg}\n\
Delegate=memory pids\n\
DelegateSubgroup=helper\n\
ProtectHome=yes\n\
PrivateTmp=yes\n\
ProtectKernelTunables=yes\n\
ProtectKernelModules=yes\n\
ProtectControlGroups=yes\n\
RestrictSUIDSGID=yes\n\
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK\n\
LockPersonality=yes\n\
MemoryDenyWriteExecute=yes\n"
    )
}

/// The helper's own cgroup, the one place it may write under `/sys/fs/cgroup` (`ReadWritePaths=`).
pub fn cgroup_path() -> String {
    atlas_host::crew::spawn::cgroup::unit_cgroup_path(&format!("{SPAWN_UNIT}.service"))
        .to_string_lossy()
        .into_owned()
}

/// `atlas-crew-provision.service`: the one root process that may create and remove accounts.
///
/// - `CAP_CHOWN`, `CAP_FOWNER`, `CAP_DAC_OVERRIDE` give the crewmate's directories their owners and
///   modes; `CAP_SETUID`/`CAP_SETGID` are what `useradd` and `userdel` need on `/etc/passwd`;
///   `CAP_KILL` ends a deleted crewmate's processes; `CAP_NET_ADMIN` loads the egress rules.
/// - Writes only what account tools touch: `/etc`, `/var/lib/atlas-cm`, `/var/log` (lastlog), `/run`.
///   `ProtectSystem=strict`, not `full`: `full` keeps `/etc` read-only and a `ReadWritePaths=/etc` does not
///   open it again, so `useradd` could not lock `/etc/passwd`. `strict` makes everything read-only but the listed paths.
/// - No `RestrictSUIDSGID=`: its seccomp filter would make the `chmod 2770` of a crewmate's `work/` fail.
/// - `AF_NETLINK` is for `nft`; there is no network family beyond that and unix sockets.
pub fn provision_service_unit() -> String {
    format!(
        "[Unit]\n\
Description=Atlas Crew provisioning (creates and removes each crewmate's user)\n\
Requires={PROVISION_UNIT}.socket\n\
After={PROVISION_UNIT}.socket\n\
[Service]\n\
Type=simple\n\
User=root\n\
ExecStart={INSTALL_DIR}/atlas crew-spawn --provision\n\
KillMode=process\n\
Restart=on-failure\n\
NoNewPrivileges=yes\n\
CapabilityBoundingSet=CAP_CHOWN CAP_FOWNER CAP_DAC_OVERRIDE CAP_SETUID CAP_SETGID CAP_KILL CAP_NET_ADMIN\n\
ProtectSystem=strict\n\
ReadWritePaths=/etc {CM_ROOT} /var/log /run\n\
ProtectHome=yes\n\
PrivateTmp=yes\n\
ProtectKernelTunables=yes\n\
ProtectKernelModules=yes\n\
ProtectControlGroups=yes\n\
RestrictAddressFamilies=AF_UNIX AF_NETLINK\n\
LockPersonality=yes\n\
MemoryDenyWriteExecute=yes\n"
    )
}

/// `atlas-crew-egress.service`: loads the egress rules at boot, before anything can start a crewmate.
/// The provisioning service rewrites [`EGRESS_FILE`] whenever a user comes or goes; this only replays it.
pub fn egress_unit() -> String {
    format!(
        "[Unit]\n\
Description=Atlas Crew egress rules for crewmate users\n\
After=network-pre.target\n\
Before=network.target\n\
ConditionPathExists={EGRESS_FILE}\n\
[Service]\n\
Type=oneshot\n\
RemainAfterExit=yes\n\
ExecStart=/usr/sbin/nft -f {EGRESS_FILE}\n\
[Install]\n\
WantedBy=multi-user.target\n"
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

/// The writes of the units, as `(file, contents)`; the caller pipes the contents to `sudo tee <file>`.
pub fn unit_files(a: &Account) -> [(String, String); 5] {
    let f = |name: &str, body: String| (format!("/etc/systemd/system/{name}"), body);
    [
        f(&format!("{SPAWN_UNIT}.socket"), socket_unit(a)),
        f(&format!("{SPAWN_UNIT}.service"), service_unit()),
        f(
            &format!("{PROVISION_UNIT}.socket"),
            provision_socket_unit(a),
        ),
        f(
            &format!("{PROVISION_UNIT}.service"),
            provision_service_unit(),
        ),
        f(&format!("{EGRESS_UNIT}.service"), egress_unit()),
    ]
}

/// Steps that start the helpers after their units are written. The sockets are enabled, never the
/// services: systemd starts a service on the first connection (a service that is already running is
/// `try-restart`ed, so it picks up a rewritten unit). The egress oneshot is enabled and only
/// replays a rules file the provisioning service has written (`ConditionPathExists`).
pub fn start_steps() -> Vec<String> {
    let mut v = vec!["sudo systemctl daemon-reload".to_string()];
    for u in [SPAWN_UNIT, PROVISION_UNIT] {
        v.push(format!("sudo systemctl enable {u}.socket"));
    }
    v.push(format!("sudo systemctl enable {EGRESS_UNIT}.service"));
    // `restart` so a re-install over running helpers picks up new units; agents survive it
    // (`KillMode=process`).
    for u in [SPAWN_UNIT, PROVISION_UNIT] {
        v.push(format!("sudo systemctl restart {u}.socket"));
    }
    // systemd makes /run/atlas-crew root:root with the unit's DirectoryMode and leaves an existing one as it
    // is: a server installed with the old 0750 would keep hostd from seeing the sockets until its next boot.
    v.push("sudo chmod 0755 /run/atlas-crew".to_string());
    // Restarting a socket leaves a running service alone, so a re-install would keep the old helper (and
    // its old unit: no cgroup delegation, no memory cap, #296). `try-restart` is a no-op for a service that
    // is not running (a fresh install: the socket starts it on the first connection); agents survive it.
    v.push(format!("sudo systemctl try-restart {SPAWN_UNIT}.service"));
    v
}

/// Shell that brings one crewmate under isolation, idempotently: its Linux user and its directories, made
/// by `atlas crew-spawn --provision-user`, which runs the plan the provisioning service runs for every
/// task (`atlas_host::crew::spawn::provision`). One code path, so the installer cannot drift from the
/// service, and what the service does with a link planted in a crewmate's directory (refuse it, never
/// chown or chmod through it) is what the installer does too. The layout is that module's.
///
/// `useradd --system` allocates from the system uid range (about 900 accounts): a failure there
/// is printed as such, and the script exits non-zero.
pub fn provision_script(a: &Account, user: &str) -> Result<String, BadUser> {
    slug(user)?;
    Ok(format!(
        "set -e; \
sudo {INSTALL_DIR}/atlas crew-spawn --provision-user {user} {crew} \
|| {{ echo 'could not provision {user}: out of system user ids? (useradd --system has about 900)' >&2; exit 1; }}",
        crew = a.user
    ))
}

/// Shell that removes the helper and every crewmate user, for `atlas crew uninstall`. Runs before
/// the crew user is deleted. `KillMode=process` means stopping the helper leaves the agents it
/// started alive, so every `atlas-cm-*` user's processes are killed explicitly first.
pub fn uninstall_script() -> String {
    format!(
        "for u in $(getent passwd | cut -d: -f1 | grep -E '^{USER_PREFIX}[0-9a-f]{{12}}$'); do \
sudo pkill -KILL -u \"$u\" 2>/dev/null || true; sudo userdel \"$u\" 2>/dev/null || true; done; \
sudo systemctl stop {SPAWN_UNIT}.socket {SPAWN_UNIT}.service {PROVISION_UNIT}.socket {PROVISION_UNIT}.service 2>/dev/null || true; \
sudo systemctl disable {SPAWN_UNIT}.socket {PROVISION_UNIT}.socket {EGRESS_UNIT}.service 2>/dev/null || true; \
sudo nft delete table inet atlas_crew 2>/dev/null || true; \
sudo rm -f /etc/systemd/system/{SPAWN_UNIT}.socket /etc/systemd/system/{SPAWN_UNIT}.service \
/etc/systemd/system/{PROVISION_UNIT}.socket /etc/systemd/system/{PROVISION_UNIT}.service \
/etc/systemd/system/{EGRESS_UNIT}.service {EGRESS_FILE}; \
sudo rm -rf {CM_ROOT}; \
sudo systemctl daemon-reload 2>/dev/null || true"
    )
}

/// Where system-wide engines live: the helper looks here first (`exec::ENGINE_DIRS`), so a binary
/// linked into [`ENGINES_DIR`] is the one every crewmate runs.
pub const ENGINES_DIR: &str = "/usr/local/lib/atlas-crew/engines";
/// `$HOME` of the engine installers: they write their trees under it (`.claude`, `.bun`, ...), readable
/// by every crewmate user. Not a crewmate's home and not the crew user's: nobody's sign-in lives here.
pub const ENGINE_HOME: &str = "/usr/local/lib/atlas-crew/engine-home";

/// Where each installer puts its binary under its `$HOME`, in the order they are tried.
const BIN_DIRS: [&str; 5] = [
    ".local/bin",
    ".bun/bin",
    ".opencode/bin",
    ".npm-global/bin",
    ".omp/bin",
];

/// Shell that installs engine `agent` system-wide: the official installer, run as root with
/// [`ENGINE_HOME`] as `$HOME`, then its binary linked into [`ENGINES_DIR`] and the tree made readable
/// and executable for every user. `None` for a name that is not an engine.
///
/// The install command is the one `atlas crew login` has always printed (`agent_install_hint`): the
/// vendor's own line, so an upgrade of the engine is the vendor's concern, not ours.
pub fn engine_install_script(agent: &str, install_cmd: &str) -> Option<String> {
    let bin = atlas_core::agents::get(agent)?.bin;
    // `bin` comes from the agent table, `install_cmd` from the CLI's own hints: neither is user input.
    let search = BIN_DIRS
        .iter()
        .map(|d| format!("{ENGINE_HOME}/{d}/{bin}"))
        .collect::<Vec<_>>()
        .join(" ");
    // `CLAUDE_INSTALL_ALLOW_SUDO`: claude.ai/install.sh refuses to run under sudo from a user's shell
    // (`SUDO_USER` set), because sudo usually leaves `$HOME` at root's home. Here `$HOME` is set to
    // [`ENGINE_HOME`] on purpose, which is the case its guard does not cover; without it every
    // `atlas crew install` failed on a real server (the review of #223 caught it; the fake installers now
    // mirror the vendor's check).
    Some(format!(
        "set -e; sudo install -d -m 0755 {ENGINE_HOME} {ENGINES_DIR}; \
sudo env HOME={ENGINE_HOME} npm_config_prefix={ENGINE_HOME}/.npm-global CLAUDE_INSTALL_ALLOW_SUDO=1 sh -c '{cmd}'; \
found=''; for p in {search}; do [ -x \"$p\" ] && found=$p && break; done; \
[ -n \"$found\" ] || {{ echo 'installed {bin} but could not find its binary under {ENGINE_HOME}' >&2; exit 1; }}; \
sudo ln -sfn \"$found\" {ENGINES_DIR}/{bin}; \
sudo chmod -R a+rX {ENGINE_HOME}",
        cmd = install_cmd.replace('\'', "'\\''")
    ))
}

/// Shell that prints the id of every crewmate the crew user has (one per line): directories of
/// `<home>/.atlas/crew/` (or the pre-rename `bots/`) whose name is a lowercase UUID, which is the only
/// shape [`atlas_host::crew::spawn::validate::user_for`] accepts.
pub fn list_crewmates_script(a: &Account) -> String {
    format!(
        "for d in {h}/.atlas/crew/*/ {h}/.atlas/bots/*/; do [ -d \"$d\" ] && basename \"$d\"; done 2>/dev/null \
| grep -E '^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$' || true",
        h = a.home
    )
}

/// The user of crewmate `id` and the shell that brings it under isolation: provisioning, and moving
/// what was in the crew home. `None` when `id` is not a crewmate id.
///
/// Nothing is deleted: memory is copied by hostd on the crewmate's first task (it keeps the old file as
/// `memory.md.pre-isolation`), and an old checkout is renamed `work.pre-isolation` (it was a git worktree
/// of a clone in the crew home, which the crewmate's user cannot read; the next task makes a fresh clone).
pub fn migrate_script(a: &Account, id: &str) -> Option<String> {
    let user = atlas_host::crew::spawn::validate::user_for(id)?;
    let prov = provision_script(a, &user).ok()?;
    let crew_dir = format!("{}/.atlas/crew/{id}", a.home);
    Some(format!(
        "{prov}; \
if [ -d {crew_dir}/work ]; then sudo mv {crew_dir}/work {crew_dir}/work.pre-isolation; fi"
    ))
}

/// The line `atlas crew install` prints after a migration, naming what changed for each crewmate.
pub fn migration_notice(migrated: usize) -> Option<String> {
    (migrated > 0).then(|| {
        format!(
            "{migrated} crewmate(s) now run as their own Linux user. Their agent sign-ins were shared \
and are not copied: sign each agent in again with `atlas crew login <server> <agent> --crewmate <id>`. \
A project checkout is made again by the next task; the old one is kept as work.pre-isolation next to it."
        )
    })
}

/// The command `atlas crew login --crewmate` runs over ssh: the engine as that crewmate's user, with its
/// home, in its directory, then the hooks registered for it. `bin` is an engine binary of the agent
/// table, `user` is checked against the helper's own shape.
pub fn login_command(user: &str, agent: &str) -> Result<String, BadUser> {
    let slug = slug(user)?;
    let bin = atlas_core::agents::get(agent)
        .map(|a| a.bin)
        .ok_or_else(|| BadUser(agent.chars().take(64).collect()))?;
    let home = format!("{CM_ROOT}/{slug}/home");
    Ok(format!(
        "sudo -u {user} env HOME={home} PATH={ENGINES_DIR}:/usr/local/bin:/usr/bin:/bin \
sh -c 'cd {home} && {bin}'"
    ))
}

/// One thing the installer does for isolation, in the order it must happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Run this shell on the server.
    Run(String),
    /// Write `contents` to `file` (`sudo tee`).
    Write { file: String, contents: String },
    /// Install engine `agent` system-wide with `script` ([`engine_install_script`]). Unlike every other
    /// action a failure here does not fail the install: an engine needs the vendor's site and, for codex,
    /// npm, which a plain server does not have, and the server works without it. The caller says which
    /// engine is missing and `line`, the vendor's install line.
    Engine {
        agent: String,
        line: String,
        script: String,
    },
}

/// Every action `atlas crew install` takes to put a server under isolation, in order, for the
/// crewmates `ids` that already exist and the engines `engines` (`(agent, install command)`).
///
/// The order is the contract: the crew home is closed (0700) and the crewmates' parent opened **before**
/// the helpers exist; the units are written before they are started; existing crewmates get their
/// users **before** hostd restarts (the caller restarts it last, so it sees the socket and the users).
pub fn install_actions(a: &Account, ids: &[String], engines: &[(&str, &str)]) -> Vec<Action> {
    let mut v: Vec<Action> = install_steps(a).into_iter().map(Action::Run).collect();
    for (file, contents) in unit_files(a) {
        v.push(Action::Write { file, contents });
    }
    // The firewall's directory exists before the provisioning service's first rules file.
    v.push(Action::Run(format!(
        "sudo install -d -m 0755 -o root -g root {}",
        EGRESS_FILE.rsplit_once('/').map_or("/etc", |(d, _)| d)
    )));
    v.extend(start_steps().into_iter().map(Action::Run));
    for (agent, cmd) in engines {
        if let Some(script) = engine_install_script(agent, cmd) {
            v.push(Action::Engine {
                agent: (*agent).to_string(),
                line: (*cmd).to_string(),
                script,
            });
        }
    }
    for id in ids {
        if let Some(sh) = migrate_script(a, id) {
            v.push(Action::Run(sh));
        }
    }
    v
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
            "DirectoryMode=0755\n",
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

    // Incident (found by the real-systemd end-to-end test): systemd creates /run/atlas-crew as root:root with
    // DirectoryMode, never as the socket's group. With 0750 the crew user (not root, not in root's group)
    // could not even stat the socket, so hostd saw "no helper" and every crewmate on every real server ran
    // unisolated as the crew user, silently. The directory must be enterable by "other"; who may CONNECT is
    // the socket's own mode (0660, group = crew user), which stays closed. A server installed before the fix
    // has the directory at 0750 until it is rebooted, so the install also fixes it in place.
    #[test]
    fn the_sockets_directory_can_be_entered_by_the_crew_user_who_is_neither_root_nor_in_its_group()
    {
        for unit in [socket_unit(&acct()), provision_socket_unit(&acct())] {
            let s = section(&unit, "Socket");
            let mode = s
                .lines()
                .find_map(|l| l.strip_prefix("DirectoryMode="))
                .map(|m| u32::from_str_radix(m, 8).unwrap())
                .expect("DirectoryMode is set");
            assert!(
                mode & 0o001 != 0,
                "DirectoryMode={mode:o}: the crew user cannot reach the socket inside"
            );
            assert!(
                s.contains("SocketMode=0660\n"),
                "the socket itself stays closed: {s}"
            );
        }
        assert!(
            start_steps()
                .iter()
                .any(|c| c.contains("chmod") && c.contains("/run/atlas-crew")),
            "a directory a previous install made at 0750 is opened by the install"
        );
    }

    #[test]
    fn the_service_is_root_socket_activated_and_does_not_kill_agents_on_restart() {
        let u = service_unit();
        let s = section(&u, "Service");
        for want in [
            "User=root\n",
            "ExecStart=/usr/local/lib/atlas-crew/atlas crew-spawn\n",
            "KillMode=process\n",
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
            "ReadWritePaths=/var/lib/atlas-cm -/sys/fs/cgroup/system.slice/atlas-crew-spawn.service\n",
            "ProtectHome=yes\n",
            "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK\n",
            "RestrictSUIDSGID=yes\n",
        ] {
            assert!(u.contains(want), "missing {want:?}");
        }
    }

    // #296: the helper owns a cgroup subtree to cap each agent in. `Delegate=memory pids` hands it over
    // (systemd 236), `DelegateSubgroup=helper` (254) keeps the helper process out of the unit's own cgroup so
    // that one can have children; older systemd ignores the line and the helper moves itself down
    // (`cgroup::Pool::detect`). `ProtectControlGroups=yes` stays: only the unit's own subtree is writable.
    #[test]
    fn the_service_delegates_its_cgroup_and_may_write_only_that_subtree() {
        let u = service_unit();
        for want in [
            "Delegate=memory pids\n",
            "DelegateSubgroup=helper\n",
            "ProtectControlGroups=yes\n",
        ] {
            assert!(u.contains(want), "missing {want:?}");
        }
        let rw = u
            .lines()
            .find(|l| l.starts_with("ReadWritePaths="))
            .unwrap();
        assert!(rw.contains("/var/lib/atlas-cm"), "{rw}");
        assert!(
            rw.contains(" -/sys/fs/cgroup/system.slice/atlas-crew-spawn.service"),
            "the helper writes only its own cgroup, never the whole hierarchy: {rw}"
        );
        // `-`: a cgroup path that does not exist (a layout this assumes wrong) must not stop the helper
        // from starting; it then runs without the cap (`Pool::detect` says why).
        // Not the hierarchy root, not a sibling service's cgroup.
        assert!(!rw.contains("/sys/fs/cgroup "), "{rw}");
        assert!(!rw.ends_with("/sys/fs/cgroup"), "{rw}");
        // The provisioning service has no use for it.
        assert!(!provision_service_unit().contains("Delegate"));
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
            // 236; `DelegateSubgroup` is 254 and listed apart (see `delegate_subgroup_is_the_only_newer_directive`).
            "Delegate",
            "ListenStream",
            "SocketUser",
            "SocketGroup",
            "SocketMode",
            "DirectoryMode",
            "WantedBy",
            "Before",
            "ConditionPathExists",
            "RemainAfterExit",
        ];
        for unit in [
            service_unit(),
            socket_unit(&acct()),
            provision_service_unit(),
            provision_socket_unit(&acct()),
            egress_unit(),
        ] {
            for line in unit.lines().filter(|l| l.contains('=')) {
                let key = line.split('=').next().unwrap();
                // Unknown directives are ignored with a warning by older systemd, and the helper does not
                // need this one there: it moves itself into `helper` (`cgroup::Pool::detect`).
                if key == "DelegateSubgroup" {
                    continue;
                }
                assert!(
                    KNOWN_245.contains(&key),
                    "{key} is not known to exist in systemd 245"
                );
            }
        }
    }

    // The one directive newer than the install floor, kept deliberately: without it (systemd < 254) the
    // helper starts in the unit's cgroup and moves itself down, which `Pool::detect` covers and tests.
    #[test]
    fn delegate_subgroup_is_the_only_newer_directive_and_matches_the_helpers_subgroup() {
        let u = service_unit();
        let sub = format!(
            "DelegateSubgroup={}\n",
            atlas_host::crew::spawn::cgroup::HELPER_SUBGROUP
        );
        assert!(
            u.contains(&sub),
            "the unit and the helper disagree on the subgroup name"
        );
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
        }
        assert!(provision_script(&a, USER).is_ok());
    }

    // Regression (security review of #448, B1): the installer used to carry its own copy of the layout as
    // `sudo install -d -o <crewmate>` calls, which follow a link a crewmate planted in its directory and
    // chown the target as root. It now runs the provisioning service's plan, so it has none of that text.
    #[test]
    fn provisioning_runs_the_services_own_plan_and_has_no_shell_copy_of_the_layout() {
        let sh = provision_script(&acct(), USER).unwrap();
        assert!(sh.starts_with("set -e;"), "{sh}");
        assert!(
            sh.contains("sudo /usr/local/lib/atlas-crew/atlas crew-spawn --provision-user atlas-cm-3f2a9c1e5b7d atlas-crew"),
            "{sh}"
        );
        assert!(sh.contains("out of system user ids"), "{sh}");
        for gone in [
            "install -d",
            "install -m",
            "useradd --system --user-group",
            "chown",
            "chmod",
            "usermod",
        ] {
            assert!(!sh.contains(gone), "{gone} in:\n{sh}");
        }
    }

    // The crew user of a migrated server is the one the plan puts hostd's files in the group of.
    #[test]
    fn provisioning_names_the_servers_own_crew_user() {
        let old = account(Probe {
            legacy_user: true,
            ..Probe::default()
        });
        let sh = provision_script(&old, USER).unwrap();
        assert!(
            sh.contains("--provision-user atlas-cm-3f2a9c1e5b7d atlas-bot"),
            "{sh}"
        );
        assert!(!sh.contains("atlas-crew crew-spawn") && !sh.contains("3f2a9c1e5b7d atlas-crew"));
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

    // KillMode=process means `systemctl stop` leaves agents running: uninstall must kill every
    // crewmate user's processes itself, and only match the helper's own user shape.
    #[test]
    fn uninstall_kills_every_crewmate_before_stopping_the_helper_and_removes_everything() {
        let sh = uninstall_script();
        assert!(sh.contains(r"grep -E '^atlas-cm-[0-9a-f]{12}$'"), "{sh}");
        assert!(sh.find("pkill").unwrap() < sh.find("userdel").unwrap());
        assert!(sh.find("userdel").unwrap() < sh.find("systemctl stop").unwrap());
        for want in [
            "systemctl stop atlas-crew-spawn.socket atlas-crew-spawn.service atlas-crew-provision.socket atlas-crew-provision.service",
            "systemctl disable atlas-crew-spawn.socket atlas-crew-provision.socket atlas-crew-egress.service",
            "nft delete table inet atlas_crew",
            "/etc/systemd/system/atlas-crew-provision.service",
            "/etc/systemd/system/atlas-crew-egress.service /etc/atlas-crew/egress.nft",
            "rm -rf /var/lib/atlas-cm",
        ] {
            assert!(sh.contains(want), "missing {want:?}");
        }
        // Never a bare `pkill` of a pattern or a user glob that could match someone else's.
        assert!(!sh.contains("pkill -f") && !sh.contains("atlas-cm-*"));
    }

    #[test]
    fn units_are_written_to_etc_systemd_and_only_sockets_and_the_egress_replay_are_enabled() {
        let files = unit_files(&acct());
        let names: Vec<&str> = files.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(
            names,
            [
                "/etc/systemd/system/atlas-crew-spawn.socket",
                "/etc/systemd/system/atlas-crew-spawn.service",
                "/etc/systemd/system/atlas-crew-provision.socket",
                "/etc/systemd/system/atlas-crew-provision.service",
                "/etc/systemd/system/atlas-crew-egress.service",
            ]
        );
        assert_eq!(files[0].1, socket_unit(&acct()));
        assert_eq!(files[1].1, service_unit());
        assert_eq!(files[2].1, provision_socket_unit(&acct()));
        assert_eq!(files[3].1, provision_service_unit());
        assert_eq!(files[4].1, egress_unit());
        let steps = start_steps();
        assert!(steps[0].ends_with("daemon-reload"));
        for want in [
            "enable atlas-crew-spawn.socket",
            "enable atlas-crew-provision.socket",
            "enable atlas-crew-egress.service",
            "restart atlas-crew-spawn.socket",
            "restart atlas-crew-provision.socket",
        ] {
            assert!(steps.iter().any(|s| s.ends_with(want)), "missing {want}");
        }
        // Socket-activated services are never enabled themselves.
        for never in [
            "enable atlas-crew-spawn.service",
            "enable atlas-crew-provision.service",
        ] {
            assert!(steps.iter().all(|s| !s.ends_with(never)), "{never}");
        }
        // #296: restarting a socket does not restart a service that is already running, so a re-install
        // over a running helper kept the old unit (no cgroup delegation, no memory cap) until it died.
        // `try-restart` does nothing for a service that is not running (a fresh install: the socket
        // starts it on the first connection), and agents survive it (`KillMode=process`).
        let socket = steps
            .iter()
            .position(|s| s.ends_with("restart atlas-crew-spawn.socket"))
            .unwrap();
        let service = steps
            .iter()
            .position(|s| s.ends_with("try-restart atlas-crew-spawn.service"))
            .expect("a running helper must pick up its new unit");
        assert!(
            socket < service,
            "the socket first: the new helper inherits it"
        );
        // Everything is enabled before anything is restarted.
        let last_enable = steps.iter().rposition(|s| s.contains(" enable ")).unwrap();
        let first_restart = steps.iter().position(|s| s.contains(" restart ")).unwrap();
        assert!(last_enable < first_restart);
    }

    // The provisioning service is the one process that writes /etc, so its limits are pinned like the
    // spawn helper's: exactly these capabilities, no more.
    #[test]
    fn the_provisioning_service_has_the_account_capabilities_and_a_writable_etc_only() {
        let u = provision_service_unit();
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
                "CAP_FOWNER",
                "CAP_KILL",
                "CAP_NET_ADMIN",
                "CAP_SETGID",
                "CAP_SETUID"
            ]
        );
        for bad in [
            "CAP_SYS_ADMIN",
            "CAP_SYS_PTRACE",
            "CAP_SYS_MODULE",
            "CAP_NET_RAW",
        ] {
            assert!(!caps.contains(bad), "{bad}");
        }
        for want in [
            "User=root\n",
            "ExecStart=/usr/local/lib/atlas-crew/atlas crew-spawn --provision\n",
            "ProtectSystem=strict\n",
            "ReadWritePaths=/etc /var/lib/atlas-cm /var/log /run\n",
            "ProtectHome=yes\n",
            "RestrictAddressFamilies=AF_UNIX AF_NETLINK\n",
            "NoNewPrivileges=yes\n",
        ] {
            assert!(u.contains(want), "missing {want:?}");
        }
        assert!(!u.contains("[Install]"));
        assert!(section(&u, "Unit").contains("Requires=atlas-crew-provision.socket\n"));
        // Same crew-only socket as the helper's, with its own path.
        let sock = provision_socket_unit(&acct());
        assert!(sock.contains("ListenStream=/run/atlas-crew/provision.sock\n"));
        assert!(sock.contains("SocketGroup=atlas-crew\n") && sock.contains("SocketMode=0660\n"));
    }

    // Incident (found by the real-systemd end-to-end test): the provisioning service had `ProtectSystem=full`
    // and `ReadWritePaths=/etc ...`, and `useradd` failed with "cannot lock /etc/passwd". `full` makes /etc
    // read-only and a `ReadWritePaths=` for /etc does not open it again (systemd 255, Ubuntu 24.04: checked with
    // `touch /etc/x` on a real VM and in a container); only `strict` lets a listed path stay writable. So a
    // unit that lists /etc must say `strict`.
    #[test]
    fn a_unit_that_needs_etc_writable_protects_the_system_strictly() {
        for (file, body) in unit_files(&acct()) {
            let Some(rw) = body.lines().find_map(|l| l.strip_prefix("ReadWritePaths=")) else {
                continue;
            };
            if rw.split(' ').any(|p| p == "/etc") {
                assert!(
                    body.contains("ProtectSystem=strict\n"),
                    "{file} lists /etc as writable but `ProtectSystem=full` keeps it read-only"
                );
            }
        }
    }

    // Incident (found by the real-systemd end-to-end test): the provisioning service had
    // `RestrictSUIDSGID=yes`, whose seccomp filter makes any chmod that sets a setuid or setgid bit fail with
    // EPERM, and its own plan gives `work/` mode 2770 (setgid, so what the crew user and the crewmate write
    // keeps one group). Creating a crewmate failed with "preparing .../work: Operation not permitted".
    #[test]
    fn the_provisioning_unit_does_not_forbid_the_setgid_bit_its_own_plan_sets() {
        use atlas_host::crew::spawn::provision::{PlanCfg, Step, provision_plan};
        let cfg = PlanCfg {
            crew: "atlas-crew".into(),
            cm_root: CM_ROOT.into(),
        };
        let plan = provision_plan(&cfg, "atlas-cm-3f2a9c1e5b7d").unwrap();
        let sets_setgid = plan.iter().any(
            |s| matches!(s, Step::Dir { mode, .. } | Step::File { mode, .. } if mode & 0o6000 != 0),
        );
        assert!(
            sets_setgid,
            "the plan no longer sets setuid/setgid: drop this test"
        );
        assert!(
            !provision_service_unit().contains("RestrictSUIDSGID=yes"),
            "RestrictSUIDSGID=yes makes the plan's chmod 2770 fail"
        );
    }

    // Incident (found while reading the spawn helper's unit next to the end-to-end test): the helper had
    // `RestrictAddressFamilies=AF_UNIX`, a seccomp filter its children inherit, so no engine it starts
    // (claude, codex, ... or git in a crewmate's tree) could open a TCP socket or resolve a name. What a
    // crewmate may reach is the egress firewall's job (nft, per user), not the unit's.
    #[test]
    fn the_spawn_helpers_children_can_use_the_network_the_firewall_allows() {
        let u = service_unit();
        let fam = u
            .lines()
            .find_map(|l| l.strip_prefix("RestrictAddressFamilies="))
            .expect("the helper restricts address families");
        for need in ["AF_UNIX", "AF_INET", "AF_INET6", "AF_NETLINK"] {
            assert!(fam.split(' ').any(|f| f == need), "{need} missing in {fam}");
        }
    }

    // Incident (real-systemd end-to-end test): with `NoNewPrivileges=yes` next to the other sandboxing options
    // systemd 255 left the helper without `CAP_SETUID` in its permitted set (`CapPrm` 0x63 instead of 0xe3 though
    // the bounding set had it), and every spawn failed at `setuid` with EPERM. Leaving that one option out
    // restored it. The child sets no_new_privs on itself instead (`spawn::exec`).
    #[test]
    fn the_spawn_helper_keeps_its_setuid_capability_so_its_unit_has_no_no_new_privileges() {
        let u = service_unit();
        assert!(!u.contains("NoNewPrivileges"), "{u}");
        assert!(u.contains("CAP_SETUID"), "{u}");
    }

    // The rules file the provisioning service writes is replayed at boot, but only once it exists.
    #[test]
    fn the_egress_unit_replays_the_rules_file_at_boot_only_when_there_is_one() {
        let u = egress_unit();
        for want in [
            "ConditionPathExists=/etc/atlas-crew/egress.nft\n",
            "ExecStart=/usr/sbin/nft -f /etc/atlas-crew/egress.nft\n",
            "Type=oneshot\n",
            "RemainAfterExit=yes\n",
            "Before=network.target\n",
            "WantedBy=multi-user.target\n",
        ] {
            assert!(u.contains(want), "missing {want:?}");
        }
    }

    #[test]
    fn engines_are_installed_with_the_vendors_line_into_a_shared_home_and_linked_for_the_helper() {
        let sh = engine_install_script("claude", "curl -fsSL https://claude.ai/install.sh | bash")
            .unwrap();
        for want in [
            "set -e",
            "install -d -m 0755 /usr/local/lib/atlas-crew/engine-home /usr/local/lib/atlas-crew/engines",
            "sudo env HOME=/usr/local/lib/atlas-crew/engine-home npm_config_prefix=/usr/local/lib/atlas-crew/engine-home/.npm-global CLAUDE_INSTALL_ALLOW_SUDO=1 sh -c 'curl -fsSL https://claude.ai/install.sh | bash'",
            "/usr/local/lib/atlas-crew/engine-home/.local/bin/claude",
            "ln -sfn \"$found\" /usr/local/lib/atlas-crew/engines/claude",
            "chmod -R a+rX /usr/local/lib/atlas-crew/engine-home",
        ] {
            assert!(sh.contains(want), "missing {want:?}:\n{sh}");
        }
        // The install finds the binary before it links, and fails loudly when it cannot.
        assert!(sh.find("for p in").unwrap() < sh.find("ln -sfn").unwrap());
        assert!(sh.contains("could not find its binary"));
        // Only engines: nothing else gets an install line.
        for bad in ["sh", "root", "", "curl", "../claude"] {
            assert!(engine_install_script(bad, "true").is_none(), "{bad:?}");
        }
    }

    // A quote in the vendor line must not end the sh -c argument early: the single quote is closed,
    // escaped and reopened, so the whole line stays one argument.
    #[test]
    fn an_install_line_with_a_single_quote_stays_one_argument() {
        let sh = engine_install_script("omp", "echo 'x'").unwrap();
        assert!(sh.contains(r"sh -c 'echo '\''x'\'''"), "{sh}");
    }

    // The helper (`spawn::exec`) is Unix-only; the installer's scripts are built on every OS.
    #[cfg(unix)]
    #[test]
    fn the_engine_dir_the_installer_links_into_is_the_one_the_helper_searches_first() {
        use atlas_host::crew::spawn::exec::ENGINE_DIRS;
        assert_eq!(ENGINE_DIRS[0], ENGINES_DIR);
    }

    #[test]
    fn existing_crewmates_are_listed_by_uuid_shape_from_both_store_names() {
        let sh = list_crewmates_script(&acct());
        assert!(sh.contains("/var/lib/atlas-crew/.atlas/crew/*/"));
        assert!(
            sh.contains("/var/lib/atlas-crew/.atlas/bots/*/"),
            "pre-rename store dir"
        );
        assert!(
            sh.contains("grep -E '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'")
        );
        // A migrated server lists its own home.
        let old = account(Probe {
            legacy_user: true,
            ..Probe::default()
        });
        assert!(list_crewmates_script(&old).contains("/var/lib/atlas-bot/.atlas/crew/*/"));
    }

    const ID: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";

    // Nothing is deleted by a migration: the old checkout is renamed, memory is moved by hostd.
    #[test]
    fn migrating_a_crewmate_provisions_it_and_renames_the_old_checkout_never_deletes_it() {
        let sh = migrate_script(&acct(), ID).unwrap();
        assert!(sh.starts_with("set -e;"), "{sh}");
        assert!(sh.contains("--provision-user atlas-cm-3f2a9c1e5b7d atlas-crew"));
        let dir = format!("/var/lib/atlas-crew/.atlas/crew/{ID}");
        assert!(
            sh.contains(&format!(
                "if [ -d {dir}/work ]; then sudo mv {dir}/work {dir}/work.pre-isolation; fi"
            )),
            "{sh}"
        );
        assert!(!sh.contains("rm -rf") && !sh.contains("rm -r"));
        // An id that is not a crewmate id never reaches a shell.
        for bad in [
            "",
            "../x",
            "x; reboot",
            "3F2A9C1E-5B7D-4E8F-9A0B-1C2D3E4F5A6B",
        ] {
            assert!(migrate_script(&acct(), bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn the_migration_notice_names_the_sign_in_command_and_only_appears_when_something_moved() {
        assert_eq!(migration_notice(0), None);
        let n = migration_notice(3).unwrap();
        assert!(n.starts_with("3 crewmate(s)"));
        assert!(n.contains("atlas crew login <server> <agent> --crewmate <id>"));
        assert!(n.contains("work.pre-isolation"));
    }

    #[test]
    fn login_runs_the_engine_as_the_crewmates_user_with_its_home_and_the_shared_engines() {
        let c = login_command(USER, "claude").unwrap();
        assert_eq!(
            c,
            "sudo -u atlas-cm-3f2a9c1e5b7d env HOME=/var/lib/atlas-cm/3f2a9c1e5b7d/home PATH=/usr/local/lib/atlas-crew/engines:/usr/local/bin:/usr/bin:/bin sh -c 'cd /var/lib/atlas-cm/3f2a9c1e5b7d/home && claude'"
        );
        // The injection guard again: neither the user nor the agent can carry shell.
        assert!(login_command("root", "claude").is_err());
        assert!(login_command("atlas-cm-3f2a9c1e5b7d; id", "claude").is_err());
        assert!(login_command(USER, "claude; id").is_err());
        assert!(login_command(USER, "sh").is_err());
    }

    // The install order is the contract. Closing the crew home comes first, units are written before
    // they start, engines and migration come before the (caller's) hostd restart.
    #[test]
    fn install_actions_close_the_home_then_write_units_then_start_them_then_migrate() {
        let engines = [("claude", "true")];
        let acts = install_actions(&acct(), &[ID.to_string()], &engines);
        let run = |needle: &'static str| {
            move |a: &Action| match a {
                Action::Run(s) | Action::Engine { script: s, .. } => s.contains(needle),
                Action::Write { .. } => false,
            }
        };
        let write = |needle: &'static str| move |a: &Action| matches!(a, Action::Write { file, .. } if file.ends_with(needle));
        let pos = |f: &dyn Fn(&Action) -> bool| acts.iter().position(f).unwrap();
        let close = pos(&run("chmod 0700 '/var/lib/atlas-crew'"));
        let root = pos(&run(
            "install -d -m 0711 -o root -g root '/var/lib/atlas-cm'",
        ));
        let first_unit = pos(&write("atlas-crew-spawn.socket"));
        let reload = pos(&run("daemon-reload"));
        let enable = pos(&run("enable atlas-crew-spawn.socket"));
        let engine = pos(&run("engine-home"));
        let migrate = pos(&run("useradd --system"));
        assert_eq!(close, 0);
        assert!(close < root && root < first_unit);
        for f in [
            "atlas-crew-spawn.service",
            "atlas-crew-provision.socket",
            "atlas-crew-provision.service",
            "atlas-crew-egress.service",
        ] {
            assert!(
                pos(&write(f)) < reload,
                "{f} must be written before daemon-reload"
            );
        }
        assert!(reload < enable);
        assert!(
            enable < engine && engine < migrate,
            "users last: after the helpers they depend on"
        );
        // The rules directory exists before the provisioning service's first write.
        assert!(pos(&run("install -d -m 0755 -o root -g root /etc/atlas-crew")) < reload);
    }

    #[test]
    fn install_actions_with_no_crewmates_and_no_engines_still_set_up_the_helpers() {
        let acts = install_actions(&acct(), &[], &[]);
        assert!(acts.iter().any(
            |a| matches!(a, Action::Write { file, .. } if file.ends_with("atlas-crew-provision.service"))
        ));
        assert!(
            !acts
                .iter()
                .any(|a| matches!(a, Action::Run(s) if s.contains("useradd")))
        );
        // A garbage id from a directory listing is skipped, never run.
        let acts = install_actions(&acct(), &["not-a-uuid; reboot".into()], &[]);
        assert!(
            !acts
                .iter()
                .any(|a| matches!(a, Action::Run(s) if s.contains("reboot")))
        );
    }
}
