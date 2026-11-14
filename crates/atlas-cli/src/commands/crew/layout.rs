//! Where a crew server keeps things, and how a server installed as "Atlas Bot" is moved to the new
//! names. Pure functions: what the host looked like goes in as a [`Probe`], the commands to run come
//! out of [`plan`], so every case is tested without ssh.

use atlas_core::crew::{LEGACY_USER, USER};

/// Binary dir. The old `/usr/local/lib/atlas-bot` is removed by the migration.
pub const INSTALL_DIR: &str = "/usr/local/lib/atlas-crew";
const LEGACY_INSTALL_DIR: &str = "/usr/local/lib/atlas-bot";
/// systemd unit name (without `.service`). The old one was `atlas-bot`.
pub const SERVICE: &str = "atlas-crew";
const LEGACY_SERVICE: &str = "atlas-bot";

/// What the host already has, asked over ssh before anything is changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Probe {
    /// `/etc/systemd/system/atlas-bot.service` exists (installed before the rename).
    pub legacy_unit: bool,
    /// The system user `atlas-bot` exists.
    pub legacy_user: bool,
    /// The system user `atlas-crew` exists.
    pub new_user: bool,
}

/// The system user a crew server runs as and its home (which holds `.atlas/`, the crewmates' data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub user: &'static str,
    pub home: String,
    /// The user has to be created (it does not exist yet).
    pub create: bool,
}

/// `atlas-crew` for a new install. A server that already has `atlas-bot` keeps it: its home holds
/// the data, and a system user is never renamed or deleted. When both exist the new one wins.
pub fn account(p: Probe) -> Account {
    let user = if p.legacy_user && !p.new_user {
        LEGACY_USER
    } else {
        USER
    };
    Account {
        user,
        home: format!("/var/lib/{user}"),
        create: !(p.new_user || p.legacy_user && user == LEGACY_USER),
    }
}

/// A line to tell the person installing that an existing user was reused.
pub fn reuse_notice(a: &Account) -> Option<String> {
    (a.user == LEGACY_USER).then(|| {
        format!(
            "reusing the existing system user {LEGACY_USER} (its home {} holds the crewmates' data)",
            a.home
        )
    })
}

/// Shell to stop and remove the pre-rename service, in order. Empty when there is none.
/// Runs before the new unit is written: two units must never run one hostd twice.
pub fn retire_legacy_service(p: Probe) -> Vec<String> {
    if !p.legacy_unit {
        return vec![];
    }
    vec![
        format!("sudo systemctl stop {LEGACY_SERVICE} 2>/dev/null || true"),
        format!("sudo systemctl disable {LEGACY_SERVICE} 2>/dev/null || true"),
        format!("sudo rm -f /etc/systemd/system/{LEGACY_SERVICE}.service"),
        "sudo systemctl daemon-reload".to_string(),
    ]
}

/// Shell to drop the old binary dir once the new binary is in place. Data is not under it.
pub fn remove_legacy_install_dir() -> String {
    format!("sudo rm -rf {LEGACY_INSTALL_DIR}")
}

/// Shell that prints the crew user of a host: `atlas-crew` unless only the pre-rename user exists.
/// The same rule as [`account`], for commands that only need the name (login, status, uninstall).
pub const WHICH_USER: &str = "if id atlas-crew >/dev/null 2>&1; then echo atlas-crew; elif id atlas-bot >/dev/null 2>&1; then echo atlas-bot; else echo atlas-crew; fi";

/// Shell that pauses (`true`) or resumes (`false`) every crewmate on a host: the hostd client runs as
/// the crew user in its data dir, the same daemon `status` reports. A failure is the caller's to see.
pub fn pause_script(pause: bool) -> String {
    let flag = if pause {
        "--crew-pause"
    } else {
        "--crew-resume"
    };
    format!(
        "u=$({WHICH_USER}); sudo -u \"$u\" {INSTALL_DIR}/atlas hostd {flag} --dir \"/var/lib/$u/.atlas\""
    )
}

/// The systemd units to query on a host: the current one, then the pre-rename one.
pub fn status_script() -> String {
    format!(
        "systemctl is-active {SERVICE}; systemctl status {SERVICE} --no-pager -l 2>&1 || true; \
         if [ -f /etc/systemd/system/{LEGACY_SERVICE}.service ]; then \
         echo '(pre-rename unit {LEGACY_SERVICE} is still installed: run atlas crew install to migrate)'; fi"
    )
}

/// `journalctl` for the crew service; `follow` streams.
pub fn logs_script(follow: bool) -> String {
    if follow {
        format!("journalctl -u {SERVICE} --no-pager -f")
    } else {
        format!("journalctl -u {SERVICE} -n 200 --no-pager")
    }
}

/// Shell that removes the crew service, binaries and user(s) from a host, both the current names
/// and the pre-rename ones. `userdel` refuses while the user has processes; the caller reports it
/// (uninstall never kills processes on someone's server).
pub fn uninstall_script() -> String {
    let mut s = String::new();
    for unit in [SERVICE, LEGACY_SERVICE] {
        s.push_str(&format!(
            "sudo systemctl stop {unit} 2>/dev/null || true; \
             sudo systemctl disable {unit} 2>/dev/null || true; \
             sudo rm -f /etc/systemd/system/{unit}.service; "
        ));
    }
    s.push_str("sudo systemctl daemon-reload 2>/dev/null || true; ");
    s.push_str(&format!("sudo rm -rf {INSTALL_DIR} {LEGACY_INSTALL_DIR}; "));
    for user in [USER, LEGACY_USER] {
        s.push_str(&format!(
            "if id {user} >/dev/null 2>&1; then sudo userdel -r {user} 2>/dev/null; fi; "
        ));
    }
    let still = [USER, LEGACY_USER]
        .map(|u| format!("id {u} >/dev/null 2>&1"))
        .join(" || ");
    s.push_str(&format!(
        "if {still}; then echo 'the crew user is still logged in (close Atlas Desktop and any ssh session as it), then run uninstall again' >&2; exit 1; fi"
    ));
    s
}

/// The systemd unit text for a crew server running as `a`.
///
/// Hardening, all valid since systemd 245: the filesystem is read-only except the crew home
/// (`ProtectSystem=strict`, `ProtectHome=read-only`, `ReadWritePaths`; every agent dir on `PATH`
/// is inside that home), `MemoryMax=80%` and `TasksMax=4096` keep a runaway crewmate from taking
/// the host down, and the kernel tunables, cgroups and setuid bits are off limits.
pub fn systemd_unit(a: &Account) -> String {
    format!(
        "[Unit]\n\
Description=Atlas Crew (atlas hostd)\n\
After=network-online.target\n\
Wants=network-online.target\n\
[Service]\n\
User={user}\n\
Environment=ATLAS_HOSTD_IDLE_SECS=never\n\
Environment=ATLAS_CREW_HOST=1\n\
Environment=PATH={h}/.atlas/bin:{h}/.bun/bin:{h}/.local/bin:{h}/.opencode/bin:{h}/.npm-global/bin:/usr/local/bin:/usr/bin:/bin\n\
ExecStart={INSTALL_DIR}/atlas hostd\n\
Restart=always\n\
RestartSec=5\n\
NoNewPrivileges=true\n\
ProtectSystem=strict\n\
ProtectHome=read-only\n\
ReadWritePaths={h}\n\
PrivateTmp=true\n\
ProtectKernelTunables=true\n\
ProtectControlGroups=true\n\
RestrictSUIDSGID=true\n\
MemoryMax=80%\n\
TasksMax=4096\n\
[Install]\n\
WantedBy=multi-user.target\n",
        user = a.user,
        h = a.home
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(legacy_unit: bool, legacy_user: bool, new_user: bool) -> Probe {
        Probe {
            legacy_unit,
            legacy_user,
            new_user,
        }
    }

    // A fresh server gets the new names and a user to create.
    #[test]
    fn a_fresh_host_gets_atlas_crew_and_creates_the_user() {
        let a = account(probe(false, false, false));
        assert_eq!(a.user, "atlas-crew");
        assert_eq!(a.home, "/var/lib/atlas-crew");
        assert!(a.create);
        assert_eq!(reuse_notice(&a), None);
    }

    // Luan's servers run crewmates as atlas-bot: its home holds the data, so the install keeps the
    // user (never renames it) and says so.
    #[test]
    fn an_existing_atlas_bot_user_is_reused_never_recreated() {
        let a = account(probe(true, true, false));
        assert_eq!(a.user, "atlas-bot");
        assert_eq!(a.home, "/var/lib/atlas-bot");
        assert!(!a.create);
        assert!(reuse_notice(&a).unwrap().contains("atlas-bot"));
    }

    #[test]
    fn an_atlas_crew_user_already_there_is_kept_and_not_created_again() {
        let a = account(probe(false, false, true));
        assert_eq!((a.user, a.create), ("atlas-crew", false));
    }

    // Both users exist (a half-migrated host): the new one wins; atlas-bot is left alone.
    #[test]
    fn with_both_users_the_new_one_wins() {
        let a = account(probe(true, true, true));
        assert_eq!((a.user, a.create), ("atlas-crew", false));
        assert_eq!(reuse_notice(&a), None);
    }

    #[test]
    fn the_old_service_is_retired_in_order_before_anything_new_runs() {
        let steps = retire_legacy_service(probe(true, true, false));
        assert_eq!(steps.len(), 4);
        assert!(steps[0].contains("systemctl stop atlas-bot"));
        assert!(steps[1].contains("systemctl disable atlas-bot"));
        assert!(steps[2].contains("rm -f /etc/systemd/system/atlas-bot.service"));
        assert!(steps[3].contains("daemon-reload"));
    }

    #[test]
    fn no_old_service_means_nothing_to_retire() {
        assert!(retire_legacy_service(probe(false, true, false)).is_empty());
        assert!(retire_legacy_service(Probe::default()).is_empty());
    }

    #[test]
    fn the_unit_names_crew_runs_as_the_account_and_keeps_its_data_writable() {
        let fresh = systemd_unit(&account(probe(false, false, false)));
        assert!(fresh.contains("Description=Atlas Crew (atlas hostd)"));
        assert!(fresh.contains("User=atlas-crew"));
        assert!(fresh.contains("Environment=ATLAS_CREW_HOST=1"));
        assert!(fresh.contains("ExecStart=/usr/local/lib/atlas-crew/atlas hostd"));
        assert!(fresh.contains("ReadWritePaths=/var/lib/atlas-crew\n"));
        assert!(fresh.contains("Environment=ATLAS_HOSTD_IDLE_SECS=never"));

        // The migrated server keeps its user and its home (the data), under the new unit.
        let old = systemd_unit(&account(probe(true, true, false)));
        assert!(old.contains("User=atlas-bot\n"));
        assert!(old.contains("ReadWritePaths=/var/lib/atlas-bot\n"));
        assert!(old.contains("PATH=/var/lib/atlas-bot/.atlas/bin:"));
        assert!(old.contains("ExecStart=/usr/local/lib/atlas-crew/atlas hostd"));
        assert!(old.contains("ATLAS_CREW_HOST=1") && !old.contains("ATLAS_BOT_HOST"));
    }

    #[test]
    fn the_unit_is_valid_ini_with_the_agent_bins_on_path() {
        let unit = systemd_unit(&account(Probe::default()));
        for needle in [
            "[Unit]",
            "[Service]",
            "[Install]",
            "WantedBy=multi-user.target",
            "NoNewPrivileges=true",
            "ProtectSystem=strict",
            "PrivateTmp=true",
            ".bun/bin",
            ".local/bin",
            ".opencode/bin",
            ".npm-global/bin",
        ] {
            assert!(unit.contains(needle), "missing {needle}");
        }
    }

    // Crew review P0, PR #106 (no kill switch): pause/resume run the hostd client as the crew user, in its data
    // dir, so they reach the same daemon `status` shows (also for a server installed as atlas-bot).
    #[test]
    fn pause_and_resume_scripts_call_hostd_as_the_crew_user_in_its_dir() {
        let pause = pause_script(true);
        let resume = pause_script(false);
        for (sh, flag, other) in [
            (&pause, "--crew-pause", "--crew-resume"),
            (&resume, "--crew-resume", "--crew-pause"),
        ] {
            assert!(sh.contains(WHICH_USER), "{sh}");
            assert!(sh.contains("sudo -u \"$u\""), "{sh}");
            assert!(
                sh.contains("/usr/local/lib/atlas-crew/atlas hostd --crew-"),
                "{sh}"
            );
            assert!(sh.contains(flag) && !sh.contains(other), "{sh}");
            assert!(sh.contains("--dir \"/var/lib/$u/.atlas\""), "{sh}");
            // Unlike `status`, a failure must reach the caller: no `|| true`.
            assert!(!sh.contains("|| true"), "{sh}");
        }
    }

    // Crew review P0, PR #106 comment 6874910355: the unit only had NoNewPrivileges/ProtectSystem/PrivateTmp,
    // so a crewmate could read other users' homes, fork-bomb the host or eat all its memory. Every
    // directive here exists in systemd 245 (Ubuntu 20.04, the oldest server we install on): ProtectHome
    // 214, TasksMax 227, MemoryMax 231 (a percentage since 238), ProtectKernelTunables and
    // ProtectControlGroups 232, RestrictSUIDSGID 242.
    #[test]
    fn the_unit_confines_homes_memory_tasks_and_the_kernel() {
        for a in [account(Probe::default()), account(probe(true, true, false))] {
            let unit = systemd_unit(&a);
            for needle in [
                "ProtectHome=read-only\n",
                "MemoryMax=80%\n",
                "TasksMax=4096\n",
                "ProtectKernelTunables=true\n",
                "ProtectControlGroups=true\n",
                "RestrictSUIDSGID=true\n",
            ] {
                assert!(unit.contains(needle), "{}: missing {needle}", a.user);
            }
        }
    }

    // ProtectHome=read-only makes /home, /root and /run/user read-only; the crew home must stay
    // writable wherever it is, and every dir the agents install into (omp, claude, codex, opencode,
    // bun) is under it and on PATH, so nothing an agent needs falls outside ReadWritePaths.
    #[test]
    fn the_crew_home_stays_writable_and_holds_every_agent_dir() {
        let a = account(Probe::default());
        let unit = systemd_unit(&a);
        assert!(unit.contains(&format!("ReadWritePaths={}\n", a.home)));
        let path = unit
            .lines()
            .find_map(|l| l.strip_prefix("Environment=PATH="))
            .unwrap();
        for dir in path.split(':').filter(|d| d.contains("/.")) {
            assert!(
                dir.starts_with(&format!("{}/", a.home)),
                "{dir} is outside the writable home {}",
                a.home
            );
        }
        for want in [
            ".atlas/bin",
            ".bun/bin",
            ".local/bin",
            ".opencode/bin",
            ".npm-global/bin",
        ] {
            assert!(path.contains(want), "{want} not on PATH");
        }
    }

    // A directive in [Service] only counts there: all hardening must sit before [Install].
    #[test]
    fn hardening_lives_in_the_service_section() {
        let unit = systemd_unit(&account(Probe::default()));
        let svc = unit.split("[Service]").nth(1).unwrap();
        let svc = svc.split("[Install]").next().unwrap();
        for d in [
            "ProtectHome=",
            "MemoryMax=",
            "TasksMax=",
            "RestrictSUIDSGID=",
        ] {
            assert!(svc.contains(d), "{d} is not in [Service]");
        }
    }

    // Uninstall must clean a server of either generation, and say so when a user is still in use.
    #[test]
    fn uninstall_removes_both_generations_of_everything() {
        let sh = uninstall_script();
        for needle in [
            "systemctl stop atlas-crew",
            "systemctl stop atlas-bot",
            "rm -f /etc/systemd/system/atlas-crew.service",
            "rm -f /etc/systemd/system/atlas-bot.service",
            "rm -rf /usr/local/lib/atlas-crew /usr/local/lib/atlas-bot",
            "userdel -r atlas-crew",
            "userdel -r atlas-bot",
            "exit 1",
        ] {
            assert!(sh.contains(needle), "missing {needle}: {sh}");
        }
        // The units are stopped before their files go, and the user goes only after the service.
        assert!(sh.find("stop atlas-crew") < sh.find("userdel"));
    }

    #[test]
    fn status_and_logs_target_the_crew_unit_and_flag_a_leftover_old_one() {
        assert!(status_script().contains("systemctl is-active atlas-crew"));
        assert!(status_script().contains("atlas crew install to migrate"));
        assert_eq!(logs_script(true), "journalctl -u atlas-crew --no-pager -f");
        assert_eq!(
            logs_script(false),
            "journalctl -u atlas-crew -n 200 --no-pager"
        );
    }

    // The same rule as `account`, in shell: the new user wins, the old one is used when it is all there is.
    #[test]
    fn which_user_prefers_the_new_user_and_falls_back_to_the_old_one() {
        let i_new = WHICH_USER.find("id atlas-crew").unwrap();
        let i_old = WHICH_USER.find("id atlas-bot").unwrap();
        assert!(i_new < i_old);
        assert!(WHICH_USER.ends_with("else echo atlas-crew; fi"));
    }
}
