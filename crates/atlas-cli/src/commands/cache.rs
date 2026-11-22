//! `atlas cache status|clear|gc`: look at and tidy the machine-wide task cache
//! (`atlas_core::taskcache`).

use anyhow::Result;
use atlas_core::taskcache::{Cache, Freed, Status};
use atlas_i18n::t;
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
            // i18n-ignore: the prefix is the name of the subcommand that ran (`atlas cache gc`), an API word like the flags
            Self::Gc => println!("gc: {}", freed(&cache.gc())),
            // i18n-ignore: the prefix is the name of the subcommand that ran (`atlas cache clear`)
            Self::Clear => println!("clear: {}", freed(&cache.clear())),
        }
        Ok(())
    }
}

fn freed(f: &Freed) -> String {
    t!(
        "cmd_misc_cache_freed",
        size = mib(f.bytes),
        entries = f.entries,
        blobs = f.blobs
    )
}

fn mib(bytes: u64) -> String {
    format!(
        "{} MiB",
        crate::numfmt::fixed(atlas_i18n::lang(), bytes as f64 / (1 << 20) as f64, 1)
    )
}

fn secs(ms: u64) -> String {
    format!(
        "{}s",
        crate::numfmt::fixed(atlas_i18n::lang(), ms as f64 / 1000.0, 1)
    )
}

fn totals(t: &atlas_core::taskcache::stats::Totals) -> String {
    t!(
        "cmd_misc_cache_totals",
        hits = t.hits,
        misses = t.misses,
        saved = secs(t.saved_ms)
    )
}

fn render(st: &Status) -> String {
    let mut s = format!(
        "{}\n{}\n{}\n",
        st.dir,
        t!(
            "cmd_misc_cache_entries",
            count = st.entries,
            size = mib(st.bytes),
            max = mib(st.max_bytes)
        ),
        totals(&st.stats.total),
    );
    for (name, p) in &st.stats.projects {
        s.push_str(&format!("  {name}: {}\n", totals(p)));
    }
    s
}
