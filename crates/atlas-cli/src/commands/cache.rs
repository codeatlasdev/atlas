//! `atlas cache status|clear|gc`: look at and tidy the machine-wide task cache
//! (`atlas_core::taskcache`).

use anyhow::Result;
use atlas_core::taskcache::{Cache, Freed, Status};
use clap::Subcommand;

use super::AtlasCommand;

#[derive(Debug, Subcommand)]
pub enum CacheCommands {
    /// Entries, size against the cap, hits, misses and time saved
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Drop blobs no entry uses and trim the least recently used entries to the cap
    Gc,
    /// Delete every entry, blob and counter
    Clear,
}

impl AtlasCommand for CacheCommands {
    async fn run(self) -> Result<()> {
        let cache = Cache::default_cache();
        match self {
            Self::Status { json } => {
                let st = cache.status();
                if json {
                    println!("{}", serde_json::to_string_pretty(&st)?);
                } else {
                    print!("{}", render(&st));
                }
            }
            Self::Gc => println!("gc: {}", freed(&cache.gc())),
            Self::Clear => println!("clear: {}", freed(&cache.clear())),
        }
        Ok(())
    }
}

fn freed(f: &Freed) -> String {
    format!(
        "freed {} ({} entr{}, {} blob{})",
        mib(f.bytes),
        f.entries,
        if f.entries == 1 { "y" } else { "ies" },
        f.blobs,
        if f.blobs == 1 { "" } else { "s" },
    )
}

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1 << 20) as f64)
}

fn secs(ms: u64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}

fn render(st: &Status) -> String {
    let t = &st.stats.total;
    let mut s = format!(
        "{}\n{} entr{}, {} of {}\n{} hit{}, {} miss{}, {} saved\n",
        st.dir,
        st.entries,
        if st.entries == 1 { "y" } else { "ies" },
        mib(st.bytes),
        mib(st.max_bytes),
        t.hits,
        if t.hits == 1 { "" } else { "s" },
        t.misses,
        if t.misses == 1 { "" } else { "es" },
        secs(t.saved_ms),
    );
    for (name, p) in &st.stats.projects {
        s.push_str(&format!(
            "  {name}: {} hits, {} misses, {} saved\n",
            p.hits,
            p.misses,
            secs(p.saved_ms)
        ));
    }
    s
}
