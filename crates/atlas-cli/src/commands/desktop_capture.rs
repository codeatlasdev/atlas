//! `atlas desktop capture start|stop|shot|status` and `atlas desktop record --script <file>`: record or
//! screenshot Atlas Desktop's own window through the control socket, without focusing or driving the
//! desktop. The app does the capture natively (ScreenCaptureKit / WKWebView snapshot); scripted
//! recordings run inside the app through its own stores. Works in every build, local only.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use super::wt::call;

/// A recording can legitimately run for minutes; the app caps it at 30 min.
const RUN_TIMEOUT_S: u64 = 31 * 60;
const SHORT: Duration = Duration::from_secs(60);

#[derive(Debug, Subcommand)]
pub enum CaptureCmd {
    /// Start recording the app window to an HEVC .mov (needs macOS 15+ and Screen Recording permission)
    Start(StartArgs),
    /// Stop the running recording and print its measured frame pacing
    Stop,
    /// Screenshot the app window (no input is sent, the window may be occluded)
    Shot(ShotArgs),
    /// Whether a recording is running and whether Screen Recording is granted
    Status,
}

#[derive(Debug, Args)]
pub struct StartArgs {
    /// Output .mov/.mp4 (absolute or relative to the current directory)
    pub out: PathBuf,
    #[arg(long, default_value_t = 60)]
    pub fps: u32,
    /// Resize the window to WIDTHxHEIGHT (logical px) before recording
    #[arg(long, value_name = "WxH", value_parser = parse_size)]
    pub size: Option<(u32, u32)>,
    /// Draw the OS cursor into the video
    #[arg(long)]
    pub cursor: bool,
}

#[derive(Debug, Args)]
pub struct ShotArgs {
    /// Output .png (absolute or relative to the current directory)
    pub out: PathBuf,
    /// `window` = ScreenCaptureKit (native chrome, needs permission); `snapshot` = page only, no permission, works hidden-behind
    #[arg(long, default_value = "window", value_parser = ["window", "snapshot"])]
    pub mode: String,
    #[arg(long, value_name = "WxH", value_parser = parse_size)]
    pub size: Option<(u32, u32)>,
}

#[derive(Debug, Args)]
pub struct RecordArgs {
    /// Script: a .json file, or a .ts/.js file whose default export is the script (evaluated with `bun`)
    #[arg(long, value_name = "FILE")]
    pub script: PathBuf,
    /// Output directory: `video.mov`, `timeline.json` and `shots/` are written here
    #[arg(long, value_name = "DIR", default_value = "capture-out")]
    pub out: PathBuf,
}

pub fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or("expected WIDTHxHEIGHT, e.g. 1280x800")?;
    let n = |v: &str| {
        v.trim()
            .parse::<u32>()
            .map_err(|_| format!("{v:?} is not a number"))
    };
    Ok((n(w)?, n(h)?))
}

fn absolute(p: &Path) -> Result<PathBuf> {
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }
    Ok(std::env::current_dir()
        .context("current directory")?
        .join(p))
}

fn size_json(s: Option<(u32, u32)>) -> Value {
    s.map_or(Value::Null, |(w, h)| json!({ "width": w, "height": h }))
}

pub fn start_request(a: &StartArgs) -> Result<Value> {
    Ok(
        json!({ "out": absolute(&a.out)?, "fps": a.fps, "size": size_json(a.size), "cursor": a.cursor }),
    )
}

pub fn shot_request(a: &ShotArgs) -> Result<Value> {
    Ok(json!({ "out": absolute(&a.out)?, "mode": a.mode, "size": size_json(a.size) }))
}

pub fn run(cmd: CaptureCmd) -> Result<()> {
    let (name, args) = match &cmd {
        CaptureCmd::Start(a) => ("capture_start", start_request(a)?),
        CaptureCmd::Stop => ("capture_stop", json!({})),
        CaptureCmd::Shot(a) => ("capture_shot", shot_request(a)?),
        CaptureCmd::Status => ("capture_status", json!({})),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&call(name, args, SHORT)?)?
    );
    Ok(())
}

/// The script as a JSON value: `.json` is read as is, `.ts`/`.js` are evaluated by `bun` (default export).
pub fn load_script(path: &Path) -> Result<Value> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let text = match ext {
        "json" => {
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
        }
        "ts" | "js" | "mts" | "mjs" => {
            let abs = absolute(path)?;
            let program = format!(
                "const m = await import({}); console.log(JSON.stringify(m.default ?? m.script));",
                serde_json::to_string(&abs.to_string_lossy())?
            );
            let out = Command::new("bun")
                .envs(atlas_core::login_env::vars())
                .args(["-e", &program])
                .output()
                .context("running `bun` to evaluate the script (is bun installed?)")?;
            if !out.status.success() {
                bail!(
                    "evaluating {} failed:\n{}",
                    path.display(),
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            String::from_utf8(out.stdout).context("script output is not UTF-8")?
        }
        other => bail!("script must be .json, .ts or .js, got {other:?}"),
    };
    serde_json::from_str(text.trim())
        .with_context(|| format!("{} is not valid JSON", path.display()))
}

pub fn record_request(script: Value, out_dir: &Path) -> Result<Value> {
    let dir = absolute(out_dir)?;
    Ok(json!({
        "script": script,
        "out": dir.join("video.mov"),
        "shots_dir": dir.join("shots"),
        "timeout_s": RUN_TIMEOUT_S,
    }))
}

pub fn record(a: RecordArgs) -> Result<()> {
    let script = load_script(&a.script)?;
    let req = record_request(script, &a.out)?;
    let dir = absolute(&a.out)?;
    std::fs::create_dir_all(dir.join("shots"))
        .with_context(|| format!("creating {}", dir.display()))?;
    let result = call("capture_run", req, Duration::from_secs(RUN_TIMEOUT_S))?;
    let path = dir.join("timeline.json");
    std::fs::write(&path, serde_json::to_string_pretty(&result["timeline"])?)?;
    println!("{}", path.display());
    if result["ok"] != true {
        bail!(
            "{}",
            result["error"].as_str().unwrap_or("the script failed")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_parses_wxh_and_rejects_garbage() {
        assert_eq!(parse_size("1280x800"), Ok((1280, 800)));
        assert_eq!(parse_size("1920X1080"), Ok((1920, 1080)));
        for bad in ["1280", "x800", "1280x", "axb", "1280x-1", ""] {
            assert!(parse_size(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn requests_use_absolute_paths_and_the_field_names_the_app_validates() {
        let r = start_request(&StartArgs {
            out: "rel/a.mov".into(),
            fps: 30,
            size: Some((1280, 800)),
            cursor: true,
        })
        .unwrap();
        assert!(Path::new(r["out"].as_str().unwrap()).is_absolute());
        assert_eq!(
            (r["fps"].clone(), r["cursor"].clone()),
            (json!(30), json!(true))
        );
        assert_eq!(r["size"], json!({"width": 1280, "height": 800}));
        let s = shot_request(&ShotArgs {
            out: "/tmp/a.png".into(),
            mode: "snapshot".into(),
            size: None,
        })
        .unwrap();
        assert_eq!(
            (s["mode"].clone(), s["size"].clone()),
            (json!("snapshot"), Value::Null)
        );
    }

    // Unix absolute paths (`/tmp/…`).
    #[cfg(unix)]
    #[test]
    fn record_request_gives_the_app_a_video_path_and_the_long_timeout() {
        let r = record_request(json!({"version": 1}), Path::new("/tmp/o")).unwrap();
        assert_eq!(r["out"], "/tmp/o/video.mov");
        assert_eq!(r["shots_dir"], "/tmp/o/shots");
        assert_eq!(r["timeout_s"], RUN_TIMEOUT_S);
    }

    #[test]
    fn json_scripts_load_and_unknown_extensions_and_bad_json_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let ok = d.path().join("a.json");
        std::fs::write(&ok, r#"{"version":1,"steps":[]}"#).unwrap();
        assert_eq!(load_script(&ok).unwrap()["version"], 1);
        let bad = d.path().join("b.json");
        std::fs::write(&bad, "{nope").unwrap();
        assert!(
            load_script(&bad)
                .unwrap_err()
                .to_string()
                .contains("not valid JSON")
        );
        let yml = d.path().join("c.yaml");
        std::fs::write(&yml, "a: 1").unwrap();
        assert!(
            load_script(&yml)
                .unwrap_err()
                .to_string()
                .contains("must be .json")
        );
        assert!(load_script(&d.path().join("missing.json")).is_err());
    }

    /// Needs `bun` (the repo's own toolchain); this is the path `bun studio` recipes use.
    #[test]
    fn a_typescript_script_is_evaluated_by_bun_to_its_default_export() {
        if Command::new("bun").arg("--version").output().is_err() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("s.ts");
        std::fs::write(&f, "const steps: {do: string; ms: number}[] = [{ do: 'pause', ms: 5 }];\nexport default { version: 1, steps };\n").unwrap();
        let v = load_script(&f).unwrap();
        assert_eq!(v["steps"][0]["do"], "pause");
    }
}
