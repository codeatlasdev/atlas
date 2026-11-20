//! `atlas crew voice install|status <ssh-target>`: the speech engine behind dictation (#647).
//!
//! whisper.cpp publishes no Linux binaries, so the server builds the **pinned source release** itself, as the crew
//! user, under `<crew home>/.atlas/voice/` (the dir hostd reads, `atlas_host::crew::voice`):
//!
//! ```text
//! voice/bin/whisper-cli          built here from WHISPER_SOURCE_URL with cmake (static, one file)
//! voice/models/ggml-<name>.bin   the multilingual ggml model (small by default, base on request)
//! voice/VERSION                  the version the binary was built from
//! ```
//!
//! - **Every download is pinned and verified.** The source tarball and each model have a sha256 in
//!   `atlas_host::crew::voice` (the source's GitHub archive, the models at a Hugging Face *commit*, which serves
//!   the same bytes for ever). `sha256sum -c` runs before anything is unpacked, moved into place or run, and a
//!   mismatch stops the install with the file removed. Nothing is piped into a shell.
//! - **Root runs only `apt-get install` of a fixed list** (`cmake`, a C++ compiler, `make`, `ffmpeg`), built by
//!   atlas and only for what is missing. The download, the unpack, the build and the models are all the crew
//!   user's: whisper.cpp's code never runs as root.
//! - **Idempotent.** A binary built from the pinned version and a model whose checksum matches are kept; run it
//!   again to repair or to move to a newer pin (and to add the other model).
//!
//! The scripts are pure functions pinned by tests; `atlas crew install --voice` runs the same [`install`].

use anyhow::{Result, bail};
use atlas_host::crew::voice::{
    MODELS, ModelPin, WHISPER_SOURCE_SHA256, WHISPER_SOURCE_URL, WHISPER_VERSION, model, model_url,
};
use atlas_i18n::t;
use clap::{Args, Subcommand};

use crate::commands::AtlasCommand;
use crate::commands::deploy::remote::Output;

use super::browsers::TargetArgs;
use super::layout::WHICH_USER;
use super::privileged::Priv;

#[derive(Debug, Args)]
pub struct VoiceArgs {
    #[command(subcommand)]
    pub command: VoiceCommands,
}

#[derive(Debug, Subcommand)]
pub enum VoiceCommands {
    /// Build the pinned whisper.cpp and fetch its model on a crew server; sudo only for `apt-get install` of cmake, a compiler and ffmpeg
    Install(VoiceInstallArgs),
    /// Say what dictation is missing on a server: the binary, its version, the model, ffmpeg
    Status(TargetArgs),
}

#[derive(Debug, Args)]
pub struct VoiceInstallArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    /// The model: `small` (about 470 MB, the default, better with code and accents) or `base` (about 140 MB, faster)
    #[arg(long, default_value = atlas_host::crew::voice::DEFAULT_MODEL, value_parser = model_name)]
    pub model: String,
}

/// clap's check of `--model`.
pub fn model_name(s: &str) -> Result<String, String> {
    model(s).map(|m| m.name.to_string()).ok_or_else(|| {
        format!(
            "one of: {}",
            MODELS.iter().map(|m| m.name).collect::<Vec<_>>().join(", ")
        )
    })
}

impl AtlasCommand for VoiceArgs {
    async fn run(self) -> Result<()> {
        match self.command {
            VoiceCommands::Install(a) => {
                let r = a.target.remote();
                let p = a.target.privileged(&r, "atlas crew voice install").await?;
                install(&p, &a.target.ssh_target, &a.model).await
            }
            VoiceCommands::Status(a) => {
                let r = a.remote();
                let p = a.privileged(&r, "atlas crew voice status").await?;
                status(&p, &a.ssh_target).await
            }
        }
    }
}

/// Packages of the tools the build and the decoder need: `(command, Debian/Ubuntu package)`. Fixed: what root
/// installs is chosen here, never read from the server.
const TOOLS: &[(&str, &str)] = &[
    ("cmake", "cmake"),
    ("g++", "g++"),
    ("make", "make"),
    ("ffmpeg", "ffmpeg"),
];

/// Shell prelude of every script: `u` is the crew user, `d` the voice dir (hostd's `<hostd dir>/voice`, the hostd
/// dir being `<crew home>/.atlas`), `as` runs a command as the crew user.
fn prelude() -> String {
    format!(
        "u=$({WHICH_USER}); d=/var/lib/$u/.atlas/voice; \
         as() {{ sudo -u \"$u\" \"$@\"; }}; "
    )
}

/// Prints `missing: <command>` for each of [`TOOLS`] the host lacks and `apt: yes|no`; always exits 0.
pub fn tools_check_script() -> String {
    let cmds = TOOLS.iter().map(|(c, _)| *c).collect::<Vec<_>>().join(" ");
    format!(
        "if command -v apt-get >/dev/null 2>&1; then echo 'apt: yes'; else echo 'apt: no'; fi; \
         for c in {cmds}; do command -v \"$c\" >/dev/null 2>&1 || echo \"missing: $c\"; done; exit 0"
    )
}

/// What [`tools_check_script`] found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tools {
    pub apt: bool,
    /// The commands that are missing, in the order of [`TOOLS`].
    pub missing: Vec<&'static str>,
}

pub fn parse_tools(stdout: &str) -> Tools {
    let mut t = Tools::default();
    for l in stdout.lines() {
        if let Some(v) = l.strip_prefix("apt: ") {
            t.apt = v.trim() == "yes";
        } else if let Some(c) = l.strip_prefix("missing: ")
            && let Some((cmd, _)) = TOOLS.iter().find(|(n, _)| *n == c.trim())
        {
            t.missing.push(cmd);
        }
    }
    t.missing
        .sort_by_key(|c| TOOLS.iter().position(|(n, _)| n == c));
    t
}

/// The one root command: refresh the package index, then install exactly the packages of the missing tools.
/// The names come from [`TOOLS`] (a constant), never from the host.
pub fn root_script(missing: &[&str]) -> Result<String> {
    let mut pkgs: Vec<&str> = missing
        .iter()
        .filter_map(|c| TOOLS.iter().find(|(n, _)| n == c).map(|(_, p)| *p))
        .collect();
    pkgs.dedup();
    if pkgs.is_empty() {
        // i18n-ignore: internal invariant, root_script is only called with a non-empty list
        bail!("nothing to install");
    }
    Ok(format!(
        "sudo apt-get update && sudo apt-get install -y --no-install-recommends {}",
        pkgs.join(" ")
    ))
}

/// As the crew user: the voice dir and its subdirs exist. Idempotent.
pub fn mkdir_script() -> String {
    format!("{}as mkdir -p \"$d/bin\" \"$d/models\"", prelude())
}

/// As the crew user: download `url` to `file` in `dir` over https only, then check its sha256. A mismatch deletes
/// the file and exits 1 with the checksum tool's own message (`FAILED`) on stderr; nothing is unpacked or moved.
/// `curl` follows redirects (GitHub and Hugging Face both redirect to a CDN) but never leaves https. The `cd`
/// is the crew user's too: the ssh user may not be able to enter the crew's home.
fn fetch_script(url: &str, sha256: &str, dir: &str, file: &str) -> String {
    format!(
        "{p}as mkdir -p \"{dir}\" && as sh -c 'cd \"$1\" && \
         curl -fsSL --proto \"=https\" --proto-redir \"=https\" --tlsv1.2 --retry 3 -o \"$2\" \"$3\" && \
         {{ echo \"$4  $2\" | sha256sum -c - >&2 || {{ rm -f \"$2\"; exit 1; }}; }}' _ \"{dir}\" '{file}' '{url}' '{sha256}'",
        p = prelude(),
    )
}

/// Download the pinned whisper.cpp source tarball into `$d/src` and verify it.
pub fn fetch_source_script() -> String {
    fetch_script(
        WHISPER_SOURCE_URL,
        WHISPER_SOURCE_SHA256,
        "$d/src",
        &format!("whisper.cpp-{WHISPER_VERSION}.tar.gz"),
    )
}

/// As the crew user, from the verified tarball: unpack, build `whisper-cli` statically with cmake (at most 4
/// jobs: each compile unit of ggml is memory hungry), put it in `$d/bin/` atomically and record the version. The
/// sources are removed after.
pub fn build_script() -> String {
    format!(
        "{p}as sh -c 'cd \"$1/src\" && rm -rf \"whisper.cpp-$2\" && tar xzf \"whisper.cpp-$2.tar.gz\" && \
         cd \"whisper.cpp-$2\" && \
         cmake -B build -DCMAKE_BUILD_TYPE=Release -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=OFF -DBUILD_SHARED_LIBS=OFF >&2 && \
         n=$(nproc); [ \"$n\" -gt 4 ] && n=4; \
         cmake --build build -j\"$n\" --target whisper-cli >&2 && \
         cp build/bin/whisper-cli \"$1/bin/whisper-cli.new\" && chmod 0755 \"$1/bin/whisper-cli.new\" && \
         mv \"$1/bin/whisper-cli.new\" \"$1/bin/whisper-cli\" && \
         echo \"$2\" > \"$1/VERSION\" && cd \"$1\" && rm -rf \"$1/src\"' _ \"$d\" '{v}'",
        p = prelude(),
        v = WHISPER_VERSION,
    )
}

/// Exit 0 when the binary is there, runs, and was built from the pinned version: the build is then skipped.
pub fn built_script() -> String {
    format!(
        "{}[ \"$(as cat \"$d/VERSION\" 2>/dev/null)\" = '{WHISPER_VERSION}' ] && as test -x \"$d/bin/whisper-cli\" && \
         as \"$d/bin/whisper-cli\" --version >/dev/null 2>&1",
        prelude()
    )
}

/// Exit 0 when the model file is there and its checksum matches the pin.
pub fn model_present_script(m: &ModelPin) -> String {
    format!(
        "{}as sh -c 'cd \"$1/models\" && echo \"$2  $3\" | sha256sum -c - >/dev/null 2>&1' _ \"$d\" '{}' '{}'",
        prelude(),
        m.sha256,
        m.file
    )
}

/// Download one model to `<file>.part` (so a half download is never taken for a model), verify, then move it
/// into place. The model is data for whisper.cpp, which reads it; it is never run.
pub fn fetch_model_script(m: &ModelPin) -> String {
    format!(
        "{p}as mkdir -p \"$d/models\" && as sh -c 'cd \"$1\" && \
         curl -fsSL --proto \"=https\" --proto-redir \"=https\" --tlsv1.2 --retry 3 -o \"$2.part\" \"$3\" && \
         {{ echo \"$4  $2.part\" | sha256sum -c - >&2 || {{ rm -f \"$2.part\"; exit 1; }}; }} && \
         mv \"$2.part\" \"$2\"' _ \"$d/models\" '{file}' '{url}' '{sha}'",
        p = prelude(),
        url = model_url(m),
        sha = m.sha256,
        file = m.file,
    )
}

/// What `status` runs: one line per missing piece on stdout, nothing when all is there; exit 1 when anything is.
/// The last check runs the binary once, which finds a missing shared library.
pub fn status_script() -> String {
    format!(
        "{p}miss=0; m() {{ echo \"$1\"; miss=1; }}; \
         as test -x \"$d/bin/whisper-cli\" || m 'whisper-cli: missing, run atlas crew voice install'; \
         v=$(as cat \"$d/VERSION\" 2>/dev/null); \
         if as test -x \"$d/bin/whisper-cli\" && [ \"$v\" != '{ver}' ]; then m \"whisper-cli: $v installed, atlas wants {ver}: run atlas crew voice install\"; fi; \
         have=0; for f in {files}; do as test -f \"$d/models/$f\" && have=1; done; \
         [ $have = 1 ] || m 'models: none installed, run atlas crew voice install'; \
         command -v ffmpeg >/dev/null 2>&1 || m 'ffmpeg: missing (Telegram voice notes need it), run atlas crew voice install'; \
         if [ $miss = 0 ]; then as \"$d/bin/whisper-cli\" --version >/dev/null 2>&1 || m 'whisper-cli: does not start, run atlas crew voice install'; fi; \
         exit $miss",
        p = prelude(),
        ver = WHISPER_VERSION,
        files = MODELS.iter().map(|m| m.file).collect::<Vec<_>>().join(" "),
    )
}

/// What failed, one message each (a verb phrase glued to a step name cannot be translated).
enum Failed {
    Dir,
    Download(String),
    Checksum(String),
    Build,
    Apt,
}

fn fail(what: Failed, target: &str, o: &Output) -> anyhow::Error {
    let reason = if o.stderr.trim().is_empty() {
        o.stdout.trim()
    } else {
        // The last lines: a build's stderr is long and its end is where it says why.
        o.stderr.trim()
    };
    let reason: String = {
        let lines: Vec<&str> = reason.lines().collect();
        lines[lines.len().saturating_sub(6)..].join("\n")
    };
    let text = match what {
        Failed::Dir => t!("cmd_crew_voice_fail_dir", target = target, reason = reason),
        Failed::Download(what) => t!(
            "cmd_crew_voice_fail_download",
            what = what,
            target = target,
            reason = reason
        ),
        Failed::Checksum(what) => t!(
            "cmd_crew_voice_fail_checksum",
            what = what,
            target = target,
            reason = reason
        ),
        Failed::Build => t!(
            "cmd_crew_voice_fail_build",
            target = target,
            reason = reason
        ),
        Failed::Apt => t!(
            "cmd_crew_fail_apt_install",
            target = target,
            reason = reason
        ),
    };
    anyhow::anyhow!("{text}")
}

/// A failed fetch is a checksum mismatch when `sha256sum` said so (exit 1 by our script after `FAILED`), else a
/// download problem.
fn fetch_failure(what: String, target: &str, o: &Output) -> anyhow::Error {
    if o.stderr.contains("FAILED") {
        fail(Failed::Checksum(what), target, o)
    } else {
        fail(Failed::Download(what), target, o)
    }
}

/// Install dictation on the host. Idempotent. Shared with `atlas crew install --voice`.
pub async fn install(r: &Priv<'_>, target: &str, model_name: &str) -> Result<()> {
    let Some(m) = model(model_name) else {
        bail!("{}", t!("cmd_crew_voice_unknown_model", model = model_name));
    };
    let step = |msg: &str| eprintln!("\x1b[36m▸\x1b[0m {msg}");

    step(&t!("cmd_crew_voice_step_tools"));
    let o = r.exec(&tools_check_script()).await?;
    let tools = parse_tools(&o.stdout);
    if !tools.missing.is_empty() {
        if !tools.apt {
            bail!(
                "{}",
                t!(
                    "cmd_crew_voice_no_apt",
                    target = target,
                    tools = tools.missing.join(", ")
                )
            );
        }
        step(&t!("cmd_crew_voice_step_apt", count = tools.missing.len()));
        let o = r.exec(&root_script(&tools.missing)?).await?;
        if o.code != 0 {
            return Err(fail(Failed::Apt, target, &o));
        }
    }

    let o = r.exec(&mkdir_script()).await?;
    if o.code != 0 {
        return Err(fail(Failed::Dir, target, &o));
    }

    if r.exec(&built_script()).await?.code == 0 {
        step(&t!("cmd_crew_voice_step_built", version = WHISPER_VERSION));
    } else {
        step(&t!("cmd_crew_voice_step_source", version = WHISPER_VERSION));
        let what = format!("whisper.cpp {WHISPER_VERSION}");
        let o = r.exec(&fetch_source_script()).await?;
        if o.code != 0 {
            return Err(fetch_failure(what, target, &o));
        }
        step(&t!("cmd_crew_voice_step_build"));
        let o = r.exec(&build_script()).await?;
        if o.code != 0 {
            return Err(fail(Failed::Build, target, &o));
        }
    }

    if r.exec(&model_present_script(m)).await?.code == 0 {
        step(&t!("cmd_crew_voice_step_model_present", model = m.name));
    } else {
        step(&t!("cmd_crew_voice_step_model", model = m.name));
        let o = r.exec(&fetch_model_script(m)).await?;
        if o.code != 0 {
            return Err(fetch_failure(format!("the {} model", m.name), target, &o));
        }
    }

    println!(
        "\x1b[32m✓\x1b[0m {}",
        t!("cmd_crew_voice_ready", target = target)
    );
    Ok(())
}

async fn status(r: &Priv<'_>, target: &str) -> Result<()> {
    let o = r.exec(&status_script()).await?;
    if o.code == 255 {
        bail!(
            "{}",
            t!(
                "cmd_crew_cannot_reach",
                target = target,
                reason = o.stderr.trim()
            )
        );
    }
    let lines = o.stdout.trim();
    if lines.is_empty() {
        if o.code == 0 {
            println!(
                "\x1b[32m✓\x1b[0m {}",
                t!(
                    "cmd_crew_voice_status_ready",
                    target = target,
                    version = WHISPER_VERSION
                )
            );
            return Ok(());
        }
        bail!(
            "{}",
            t!(
                "cmd_crew_check_failed_reason",
                target = target,
                reason = o.stderr.trim()
            )
        );
    }
    for l in lines.lines() {
        println!("\x1b[31m✗\x1b[0m {l}");
    }
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_scripts() -> Vec<(&'static str, String)> {
        let base = model("base").unwrap();
        vec![
            ("tools", tools_check_script()),
            ("root", root_script(&["cmake", "ffmpeg"]).unwrap()),
            ("mkdir", mkdir_script()),
            ("fetch_source", fetch_source_script()),
            ("build", build_script()),
            ("built", built_script()),
            ("model_present", model_present_script(base)),
            ("fetch_model", fetch_model_script(base)),
            ("status", status_script()),
        ]
    }

    // whisper.cpp's code, the tarball and the models are all the crew user's: root runs `apt-get install` of names
    // from a constant, and every other `sudo` is `sudo -u "$u"`.
    #[test]
    fn root_runs_nothing_but_apt_get_of_a_fixed_list() {
        let root = root_script(&["cmake", "ffmpeg"]).unwrap();
        assert_eq!(
            root,
            "sudo apt-get update && sudo apt-get install -y --no-install-recommends cmake ffmpeg"
        );
        for prog in ["curl", "tar", "cmake -B", "sh ", "bash", "env", "tee"] {
            assert!(!root.contains(prog), "{prog} in {root}");
        }
        for (name, s) in all_scripts() {
            if name == "root" {
                continue;
            }
            assert_eq!(
                s.matches("sudo ").count(),
                s.matches("sudo -u \"$u\" ").count(),
                "{name} has a sudo that is not 'sudo -u \"$u\"': {s}"
            );
        }
        // An unknown command is never turned into a package name, and nothing missing is an error.
        assert!(root_script(&[]).is_err());
        assert!(root_script(&["rm"]).is_err());
    }

    // Every download is https only, checked against a pinned hash, and never run through a shell.
    #[test]
    fn every_download_is_https_pinned_and_verified_before_use() {
        let base = model("base").unwrap();
        for (name, s) in [
            ("source", fetch_source_script()),
            ("model", fetch_model_script(base)),
        ] {
            assert!(
                s.contains("--proto \"=https\"") && s.contains("--proto-redir \"=https\""),
                "{name}: {s}"
            );
            assert!(s.contains("--tlsv1.2"), "{name}");
            assert!(s.contains("sha256sum -c"), "{name}");
            assert!(!s.contains("http://"), "{name}");
            // "| sha256sum" is the check; a pipe into a shell is `| sh ` or `| bash`.
            assert!(
                !s.contains("| sh ")
                    && !s.contains("| sh'")
                    && !s.contains("| bash")
                    && !s.contains("|sh "),
                "{name}: curl | sh"
            );
            // The check comes before anything else touches the file; a mismatch removes it.
            let (curl, sum) = (s.find("curl ").unwrap(), s.find("sha256sum -c").unwrap());
            assert!(curl < sum, "{name}");
            assert!(
                s.contains("rm -f"),
                "{name}: a file that failed its hash is removed"
            );
        }
        assert!(fetch_source_script().contains(WHISPER_SOURCE_SHA256));
        assert!(fetch_source_script().contains(WHISPER_SOURCE_URL));
        assert!(fetch_model_script(base).contains(base.sha256));
        assert!(fetch_model_script(base).contains(&model_url(base)));
        // A model lands as `.part` and is moved only after the check.
        let m = fetch_model_script(base);
        assert!(
            m.find("sha256sum -c").unwrap() < m.find("mv \"$2.part\"").unwrap(),
            "{m}"
        );
    }

    // The tarball is unpacked only by the build script, which runs after the fetch script succeeded; the two are
    // separate steps and `install` orders them.
    #[test]
    fn install_fetches_and_verifies_before_it_unpacks_and_builds() {
        let src = include_str!("voice.rs");
        let body = &src[src.find("pub async fn install").unwrap()..];
        let order = |a: &str, b: &str| body.find(a).unwrap() < body.find(b).unwrap();
        assert!(order("fetch_source_script()", "build_script()"));
        assert!(order("tools_check_script()", "root_script("));
        assert!(order("root_script(", "fetch_source_script()"));
        assert!(
            !fetch_source_script().contains("tar x"),
            "the fetch does not unpack"
        );
        assert!(build_script().contains("tar xzf"));
        assert!(
            !build_script().contains("curl"),
            "the build does not download"
        );
    }

    #[test]
    fn the_build_is_static_minimal_and_swapped_in_atomically() {
        let b = build_script();
        assert!(b.contains("-DBUILD_SHARED_LIBS=OFF") && b.contains("--target whisper-cli"));
        assert!(
            b.contains("-DWHISPER_BUILD_SERVER=OFF") && b.contains("-DWHISPER_BUILD_TESTS=OFF")
        );
        assert!(
            b.contains("whisper-cli.new")
                && b.find("cp build").unwrap() < b.find("mv \"$1/bin").unwrap()
        );
        assert!(
            b.contains(&format!("'{WHISPER_VERSION}'")) && b.contains("> \"$1/VERSION\""),
            "the version is recorded after the build"
        );
        assert!(
            b.contains("rm -rf \"$1/src\""),
            "the sources are removed after"
        );
    }

    // The build is skipped when the pinned version is already there, and a model only when its hash matches.
    #[test]
    fn a_built_binary_and_a_verified_model_are_kept() {
        let built = built_script();
        assert!(built.contains(WHISPER_VERSION) && built.contains("--version"));
        let base = model("base").unwrap();
        let present = model_present_script(base);
        assert!(
            present.contains(base.sha256) && present.contains("sha256sum -c"),
            "{present}"
        );
    }

    #[test]
    fn the_status_script_names_every_model_file_and_the_pinned_version() {
        let s = status_script();
        for m in &MODELS {
            assert!(s.contains(m.file), "{}", m.file);
        }
        assert!(s.contains(WHISPER_VERSION));
        assert!(s.contains("ffmpeg"));
    }

    #[test]
    fn the_tools_report_is_read_in_order_and_ignores_anything_else() {
        let t = parse_tools("apt: yes\nmissing: ffmpeg\nmissing: cmake\nmissing: rm -rf\nnoise\n");
        assert_eq!(
            t,
            Tools {
                apt: true,
                missing: vec!["cmake", "ffmpeg"]
            }
        );
        assert_eq!(parse_tools("apt: no\n"), Tools::default());
        assert_eq!(parse_tools(""), Tools::default());
    }

    #[test]
    fn the_models_flag_takes_the_pinned_names_only() {
        assert_eq!(model_name("small").unwrap(), "small");
        assert_eq!(model_name("base").unwrap(), "base");
        for bad in ["large", "small.en", "", "../x", "tiny"] {
            assert!(model_name(bad).is_err(), "{bad}");
        }
    }

    // Nothing written into a script is user text: the only inputs are constants.
    #[test]
    fn the_scripts_hold_no_double_quote_or_backtick_surprises() {
        for (name, s) in all_scripts() {
            assert!(!s.contains('`'), "{name}");
            assert!(!s.contains("eval "), "{name}");
        }
    }
}

/// The generated scripts run for real against a simulated crew server (`server_sim`): fake `sudo`, `curl`, `cmake`
/// and `nproc`, the real `tar`, `sh` and checksum tool. Nothing of the scripts is mocked.
#[cfg(all(test, unix))]
mod sim_tests {
    use super::super::server_sim::Sim;
    use super::*;
    use sha2::{Digest, Sha256};

    const VERSION_DIR: &str = "var/lib/atlas-crew/.atlas/voice";

    fn sim() -> Sim {
        let s = Sim::new();
        std::fs::create_dir_all(s.path("var/lib/atlas-crew")).unwrap();
        std::fs::create_dir_all(s.path("served")).unwrap();
        // `curl -o <file> <url>`: serves `$ROOT/served/<basename of url>`, fails like `curl -f` when absent.
        s.fake(
            "curl",
            "while [ $# -gt 0 ]; do case \"$1\" in -o) out=\"$2\"; shift;; --retry|--proto|--proto-redir) shift;; -*) ;; *) url=\"$1\";; esac; shift; done\n\
             echo \"curl $url\" >> \"$ROOT/log/curl\"\n\
             f=\"$ROOT/served/${url##*/}\"\n\
             [ -f \"$f\" ] || { echo 'curl: (22) The requested URL returned error: 404' >&2; exit 22; }\n\
             cp \"$f\" \"$out\"",
        );
        s.fake("nproc", "echo 8");
        // `cmake -B build ...` configures; `cmake --build build ...` leaves the binary where whisper.cpp does.
        s.fake(
            "cmake",
            "echo \"cmake $*\" >> \"$ROOT/log/cmake\"\n\
             case \"$1\" in --build) mkdir -p build/bin; printf '#!/bin/sh\\necho whisper-cli fake\\n' > build/bin/whisper-cli; chmod +x build/bin/whisper-cli;; esac",
        );
        // macOS has no `sha256sum`; the checksum tool is the one shipped with perl there.
        if !["/usr/bin/sha256sum", "/bin/sha256sum"]
            .iter()
            .any(|p| Path::new(p).exists())
        {
            s.fake("sha256sum", "exec shasum -a 256 \"$@\"");
        }
        s
    }

    use std::path::Path;

    fn sha(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    /// A real `.tar.gz` of `whisper.cpp-<v>/CMakeLists.txt`, made with this machine's tar.
    fn tarball(s: &Sim, name: &str) -> Vec<u8> {
        let work = tempfile::tempdir().unwrap();
        let top = work.path().join(format!("whisper.cpp-{WHISPER_VERSION}"));
        std::fs::create_dir_all(&top).unwrap();
        std::fs::write(top.join("CMakeLists.txt"), "project(whisper)\n").unwrap();
        let out = work.path().join(name);
        let st = std::process::Command::new("tar")
            .arg("czf")
            .arg(&out)
            .arg("-C")
            .arg(work.path())
            .arg(format!("whisper.cpp-{WHISPER_VERSION}"))
            .status()
            .unwrap();
        assert!(st.success());
        let bytes = std::fs::read(&out).unwrap();
        s.write(&format!("served/{name}"), &bytes);
        bytes
    }

    fn tar_name() -> String {
        format!("whisper.cpp-{WHISPER_VERSION}.tar.gz")
    }

    // The pinned download of the real script, served wrong bytes: it fails with the checksum tool's words, the
    // file is deleted, and the build script (which unpacks) was never given anything.
    #[test]
    fn a_download_that_fails_its_checksum_is_deleted_and_nothing_is_built() {
        let s = sim();
        tarball(&s, &format!("v{WHISPER_VERSION}.tar.gz"));
        let run = s.run(&fetch_source_script(), b"");
        assert_eq!(run.code, Some(1), "{}\n{}", run.stdout, run.stderr);
        assert!(run.stderr.contains("FAILED"), "{}", run.stderr);
        assert!(
            !s.exists(&format!("{VERSION_DIR}/src/{}", tar_name())),
            "the bad file stayed"
        );
        assert!(
            !s.exists("log/cmake"),
            "something was built from an unverified file"
        );
        // `install` tells a mismatch from a network problem by that word.
        let o = Output {
            code: 1,
            stdout: run.stdout,
            stderr: run.stderr,
        };
        assert!(
            fetch_failure("x".into(), "me@srv", &o)
                .to_string()
                .contains("pinned checksum")
        );
        let gone = Output {
            code: 22,
            stdout: String::new(),
            stderr: "curl: (22) 404".into(),
        };
        assert!(
            !fetch_failure("x".into(), "me@srv", &gone)
                .to_string()
                .contains("pinned checksum")
        );
    }

    // A download that does match is unpacked and built statically with at most four jobs (the machine says
    // eight), swapped in atomically, versioned, and the sources are gone.
    #[test]
    fn a_verified_source_is_built_swapped_in_and_the_sources_removed() {
        let s = sim();
        let bytes = tarball(&s, &format!("v{WHISPER_VERSION}.tar.gz"));
        // The same script as `fetch_source_script`, with the hash of this test's tarball.
        let fetch = fetch_script(WHISPER_SOURCE_URL, &sha(&bytes), "$d/src", &tar_name());
        let run = s.run(&fetch, b"");
        assert_eq!(run.code, Some(0), "{}\n{}", run.stdout, run.stderr);
        assert!(s.exists(&format!("{VERSION_DIR}/src/{}", tar_name())));
        assert!(
            s.read("log/curl").contains(WHISPER_SOURCE_URL),
            "{}",
            s.read("log/curl")
        );

        let run = s.run(&format!("{} && {}", mkdir_script(), build_script()), b"");
        assert_eq!(run.code, Some(0), "{}\n{}", run.stdout, run.stderr);
        let cmake = s.read("log/cmake");
        assert!(
            cmake.contains("-DBUILD_SHARED_LIBS=OFF")
                && cmake.contains("-DWHISPER_BUILD_SERVER=OFF"),
            "{cmake}"
        );
        assert!(
            cmake.contains("-j4 ") || cmake.contains("-j4\n"),
            "capped at four jobs: {cmake}"
        );
        assert!(cmake.contains("--target whisper-cli"), "{cmake}");
        assert!(s.exists(&format!("{VERSION_DIR}/bin/whisper-cli")));
        assert!(
            !s.exists(&format!("{VERSION_DIR}/bin/whisper-cli.new")),
            "the swap left its temp file"
        );
        assert_eq!(
            s.read(&format!("{VERSION_DIR}/VERSION")).trim(),
            WHISPER_VERSION
        );
        assert!(
            !s.exists(&format!("{VERSION_DIR}/src")),
            "the sources were left on the server"
        );

        // Built from the pinned version: the next install skips the build; any other version does not.
        assert_eq!(s.run(&built_script(), b"").code, Some(0));
        s.write(&format!("{VERSION_DIR}/VERSION"), b"0.0.1\n");
        assert_eq!(
            s.run(&built_script(), b"").code,
            Some(1),
            "an older build is rebuilt"
        );
    }

    /// A model of this test's own bytes, so its pin can be matched.
    fn model_of(bytes: &[u8]) -> ModelPin {
        ModelPin {
            name: "base",
            file: "ggml-test.bin",
            sha256: Box::leak(sha(bytes).into_boxed_str()),
            bytes: bytes.len() as u64,
        }
    }

    // A model is moved into place only after its check; a half or wrong download is never a model.
    #[test]
    fn a_model_lands_only_after_its_checksum_and_a_tampered_one_is_not_kept() {
        let s = sim();
        let good = b"pretend these are ggml weights".to_vec();
        let m = model_of(&good);
        let models = format!("{VERSION_DIR}/models");

        // Wrong bytes served under the model's name: refused, nothing at all left.
        s.write("served/ggml-test.bin", b"someone else's file");
        let run = s.run(&fetch_model_script(&m), b"");
        assert_eq!(run.code, Some(1), "{}\n{}", run.stdout, run.stderr);
        assert!(run.stderr.contains("FAILED"), "{}", run.stderr);
        assert!(
            !s.exists(&format!("{models}/ggml-test.bin"))
                && !s.exists(&format!("{models}/ggml-test.bin.part"))
        );
        assert_eq!(s.run(&model_present_script(&m), b"").code, Some(1));

        // The right bytes: in place, no `.part`, and recognised as present.
        s.write("served/ggml-test.bin", &good);
        let run = s.run(&fetch_model_script(&m), b"");
        assert_eq!(run.code, Some(0), "{}\n{}", run.stdout, run.stderr);
        assert_eq!(
            std::fs::read(s.path(&format!("{models}/ggml-test.bin"))).unwrap(),
            good
        );
        assert!(!s.exists(&format!("{models}/ggml-test.bin.part")));
        assert_eq!(s.run(&model_present_script(&m), b"").code, Some(0));

        // A file that changed after the install is no longer "present": it is fetched again.
        s.write(&format!("{models}/ggml-test.bin"), b"corrupted");
        assert_eq!(s.run(&model_present_script(&m), b"").code, Some(1));
        // A network failure leaves the old file alone.
        std::fs::remove_file(s.path("served/ggml-test.bin")).unwrap();
        let run = s.run(&fetch_model_script(&m), b"");
        assert_eq!(run.code, Some(22));
        assert!(!s.exists(&format!("{models}/ggml-test.bin.part")));
    }

    // The tool check reads the host; a host without apt is told which tools to install by hand.
    #[test]
    fn the_tools_check_finds_what_the_host_lacks() {
        let s = sim();
        for t in ["cmake", "g++", "make", "ffmpeg"] {
            s.remove_fake(t);
        }
        s.fake("apt-get", "exit 0");
        let t = parse_tools(&s.run(&tools_check_script(), b"").stdout);
        assert!(t.apt);
        assert_eq!(t.missing, ["cmake", "g++", "make", "ffmpeg"]);
        s.fake("cmake", "exit 0");
        s.fake("ffmpeg", "exit 0");
        s.remove_fake("apt-get");
        let t = parse_tools(&s.run(&tools_check_script(), b"").stdout);
        assert!(!t.apt);
        assert_eq!(t.missing, ["g++", "make"]);
    }

    // `status` is empty when all is there, and says each missing piece otherwise.
    #[test]
    fn the_status_names_what_is_missing_and_is_silent_when_ready() {
        let s = sim();
        s.fake("ffmpeg", "exit 0");
        let run = s.run(&status_script(), b"");
        assert_eq!(run.code, Some(1));
        assert!(
            run.stdout.contains("whisper-cli: missing")
                && run.stdout.contains("models: none installed"),
            "{}",
            run.stdout
        );
        s.write(
            &format!("{VERSION_DIR}/bin/whisper-cli"),
            b"#!/bin/sh\nexit 0\n",
        );
        s.write(
            &format!("{VERSION_DIR}/VERSION"),
            format!("{WHISPER_VERSION}\n").as_bytes(),
        );
        s.write(&format!("{VERSION_DIR}/models/{}", MODELS[0].file), b"m");
        let run = s.run(&status_script(), b"");
        assert_eq!(
            (run.code, run.stdout.as_str()),
            (Some(0), ""),
            "{}",
            run.stderr
        );
        s.write(&format!("{VERSION_DIR}/VERSION"), b"0.0.1\n");
        let run = s.run(&status_script(), b"");
        assert!(run.stdout.contains("0.0.1 installed"), "{}", run.stdout);
    }
}
