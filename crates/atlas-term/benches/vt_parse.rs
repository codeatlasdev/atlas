//! Parser throughput of `Vt::feed` per corpus (`scripts/term-corpus-gen.ts`), CPU only: no GPU, no pty, no window.
//!
//! ```text
//! cargo bench -p atlas-term --bench vt_parse -- [--mib 16] [--passes 5] [--only seq,ascii] [--json out.json] [--corpus dir]
//! ```
//!
//! Without `--corpus` the corpora are generated into `target/term-corpus-<mib>` by `bun scripts/term-corpus-gen.ts`
//! (kept between runs; the generator is deterministic). One warm-up pass, then `--passes` timed passes, each on a fresh
//! 120x40 `Vt` fed 64 KiB chunks (the shape of the spike this replaces: docs/terminal-linux-design.md, section 5.1).
//!
//! Every report also carries `calibration_mbps`: a fixed scalar workload (dependent stores into a 1 MiB table, the
//! shape of grid writes) timed in the same process. CI runners differ from the machine that wrote the baseline, so
//! the gate (`scripts/term-parser-gate.ts`) compares `mbps / calibration_mbps` against the committed baseline, not MB/s.

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use atlas_term::Vt;

const COLS: u16 = 120;
const ROWS: u16 = 40;
const CHUNK: usize = 64 << 10;
const CORPORA: [&str; 8] = [
    "seq",
    "ascii",
    "sgr",
    "repaint256",
    "truecolor",
    "ink-2026",
    "ratatui-diff",
    "omp-boxes",
];

struct Args {
    mib: u32,
    passes: usize,
    only: Option<Vec<String>>,
    json: Option<PathBuf>,
    corpus: Option<PathBuf>,
}

fn args() -> Args {
    let mut a = Args {
        mib: 16,
        passes: 5,
        only: None,
        json: None,
        corpus: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| panic!("{k} needs a value"));
        match k.as_str() {
            "--mib" => a.mib = val().parse().expect("--mib N"),
            "--passes" => a.passes = val().parse().expect("--passes N"),
            "--only" => a.only = Some(val().split(',').map(str::to_owned).collect()),
            "--json" => a.json = Some(val().into()),
            "--corpus" => a.corpus = Some(val().into()),
            // `cargo bench` passes `--bench`; anything else is a typo worth failing on.
            "--bench" => {}
            other => panic!("unknown argument {other}"),
        }
    }
    a
}

fn load1() -> f64 {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .unwrap_or(-1.0)
}

fn cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_owned())
        })
        .unwrap_or_else(|| std::env::consts::ARCH.to_owned())
}

fn corpus_dir(a: &Args) -> PathBuf {
    if let Some(d) = &a.corpus {
        return d.clone();
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"))
        .join(format!("term-corpus-{}", a.mib));
    let missing = CORPORA
        .iter()
        .any(|c| !dir.join(format!("{c}.bin")).exists());
    if missing {
        let st = Command::new("bun")
            .arg(root.join("scripts/term-corpus-gen.ts"))
            .arg("--out")
            .arg(&dir)
            .arg("--mib")
            .arg(a.mib.to_string())
            .status()
            .expect("bun is needed to generate the corpora (scripts/term-corpus-gen.ts), or pass --corpus <dir>");
        assert!(st.success(), "term-corpus-gen.ts failed");
    }
    dir
}

/// MB/s of a fixed scalar workload: dependent stores into a 1 MiB table, 64 MiB of them. Best of 7.
fn calibration() -> f64 {
    const BYTES: usize = 64 << 20;
    let mut table = vec![0u64; (1 << 20) / 8];
    let mask = table.len() - 1;
    let mut best = f64::MAX;
    for _ in 0..7 {
        let t = Instant::now();
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..BYTES / 8 {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let i = (x >> 33) as usize & mask;
            table[i] = table[i].wrapping_add(x);
        }
        black_box((&table, x));
        best = best.min(t.elapsed().as_secs_f64());
    }
    BYTES as f64 / 1e6 / best
}

fn pass(data: &[u8]) -> f64 {
    let vt = Vt::new(COLS, ROWS, |_| {});
    let t = Instant::now();
    for chunk in data.chunks(CHUNK) {
        vt.feed(black_box(chunk));
    }
    // Ends any synchronized update still open, so the grid work of the last frame is inside the timing.
    vt.flush_sync();
    black_box(vt.cursor());
    data.len() as f64 / 1e6 / t.elapsed().as_secs_f64()
}

fn main() {
    let a = args();
    let dir = corpus_dir(&a);
    let calib = calibration();
    eprintln!("calibration {calib:.0} MB/s, load {:.2}", load1());
    let mut rows = Vec::new();
    for name in CORPORA {
        if a.only
            .as_ref()
            .is_some_and(|o| !o.iter().any(|n| n == name))
        {
            continue;
        }
        let data = std::fs::read(dir.join(format!("{name}.bin"))).expect("corpus file");
        let load_before = load1();
        pass(&data);
        let mut s: Vec<f64> = (0..a.passes).map(|_| pass(&data)).collect();
        let load_after = load1();
        s.sort_by(f64::total_cmp);
        let (best, median) = (s[s.len() - 1], s[s.len() / 2]);
        eprintln!(
            "{name:<13} best {best:>7.1} MB/s  median {median:>7.1}  ratio {:.4}  load {load_before:.2}->{load_after:.2}",
            best / calib
        );
        rows.push(serde_json::json!({
            "corpus": name,
            "bytes": data.len(),
            "best_mbps": round1(best),
            "median_mbps": round1(median),
            "ratio": (best / calib * 1e4).round() / 1e4,
            "load_before": load_before,
            "load_after": load_after,
        }));
    }
    let report = serde_json::json!({
        "cpu": cpu_model(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "grid": [COLS, ROWS],
        "chunk": CHUNK,
        "passes": a.passes,
        "mib": a.mib,
        "calibration_mbps": round1(calib),
        "corpora": rows,
    });
    let text = serde_json::to_string_pretty(&report).unwrap();
    match a.json {
        Some(p) => std::fs::write(p, text + "\n").expect("write --json"),
        None => println!("{text}"),
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}
