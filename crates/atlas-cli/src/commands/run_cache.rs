//! `atlas run --cache`: the task cache in front of a command. The key comes from the argv, the
//! input files as they are on disk, the declared env values and tool versions
//! (`atlas_core::taskcache::Spec`); a hit restores the outputs, replays the log and exits without
//! running anything, a miss runs the command (behind the heavy queue with `--heavy`) and stores a
//! successful result.

use std::time::Instant;

use anyhow::{Context, Result};
use atlas_core::heavy::Dir;
use atlas_core::taskcache::{Cache, Chunk, Recorded, Run, Spec};
use clap::Args;

use super::heavy::{RunArgs, label_of, project_of, run_job};

/// The flags of `atlas run` that belong to the cache.
#[derive(Args, Debug, Default)]
pub struct CacheArgs {
    /// Cache the result: an identical run (same argv, same input bytes, same declared env and tools)
    /// restores the outputs, replays the log and runs nothing. Only exit 0 is stored.
    #[arg(long)]
    pub cache: bool,
    /// Files the command reads (glob, `.gitignore` respected, repeatable). Default: every file under
    /// the current folder that git would not ignore.
    #[arg(long = "input", requires = "cache")]
    pub inputs: Vec<String>,
    /// Paths the command produces (file or folder inside the current folder, repeatable): stored
    /// on a miss, restored on a hit.
    #[arg(long = "output", requires = "cache")]
    pub outputs: Vec<String>,
    /// Environment variable the command reads (repeatable): its value is part of the key.
    #[arg(long = "env", requires = "cache")]
    pub env: Vec<String>,
    /// Program whose `--version` is part of the key (repeatable).
    #[arg(long = "tool", requires = "cache")]
    pub tools: Vec<String>,
    /// Store a failing run too: a hit then replays its output and exit code.
    #[arg(long, requires = "cache")]
    pub cache_failures: bool,
}

/// Run `args.command` through the cache and return the exit code.
pub async fn run(args: &RunArgs) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let spec = Spec {
        cwd: cwd.clone(),
        argv: args.command.clone(),
        inputs: args.cache.inputs.clone(),
        outputs: args.cache.outputs.clone(),
        env: args.cache.env.clone(),
        tools: args.cache.tools.clone(),
    };
    // A bad output path or a tool that cannot report its version stops the run here, before the
    // command does anything.
    let key = spec.key().context("cannot compute the cache key")?;
    let mut cache = Cache::default_cache();
    if args.cache.cache_failures {
        cache = cache.keep_failures();
    }

    if let Some(entry) = cache.lookup(&key) {
        let started = Instant::now();
        match cache.restore(&entry, &spec).and_then(|r| {
            cache.replay(&entry, &mut std::io::stdout(), &mut std::io::stderr())?;
            Ok(r)
        }) {
            Ok(restored) => {
                cache.note_hit(&entry, &key);
                eprintln!(
                    "atlas cache: hit {} ({} file{}, {} ms, saved {} ms)",
                    &key.as_str()[..12],
                    restored.files,
                    if restored.files == 1 { "" } else { "s" },
                    started.elapsed().as_millis(),
                    entry.duration_ms,
                );
                return Ok(entry.exit);
            }
            // A damaged entry is a miss: run for real and overwrite it.
            Err(e) => eprintln!(
                "atlas cache: entry {} unusable ({e}), running",
                &key.as_str()[..12]
            ),
        }
    }

    let queue = args.heavy.then(Dir::default_dir);
    let mut log: Vec<Chunk> = Vec::new();
    let started = Instant::now();
    let exit = run_job(queue.as_ref(), args, Some(&mut log)).await?;
    let run = Run {
        project: project_of(&cwd),
        label: label_of(args),
        command: super::open::command_line(&args.command),
        exit,
        duration_ms: started.elapsed().as_millis() as u64,
        log,
    };
    match cache.record(&key, &spec, &run) {
        Ok(Recorded::Stored) => eprintln!("atlas cache: stored {}", &key.as_str()[..12]),
        Ok(Recorded::NotStored(why)) => {
            eprintln!("atlas cache: not stored ({why:?})");
        }
        // The command already ran and its result is the user's: a cache that cannot write
        // must not fail it.
        Err(e) => eprintln!("atlas cache: could not store the result: {e}"),
    }
    Ok(exit)
}
