use anyhow::Result;
use atlas_skills::{SkillState, SkillsManifest, embedded};

use super::{manifest_path, skills_dir};

pub fn handle(global: bool) -> Result<()> {
    let dir = skills_dir(global);
    let manifest = SkillsManifest::load_from(&manifest_path(global))?;
    let available = embedded::all();

    println!(
        "{:<24} {:<14} \x1b[1mBUNDLED\x1b[0m",
        "\x1b[1mSKILL\x1b[0m", "\x1b[1mINSTALLED\x1b[0m"
    );
    println!("{}", "─".repeat(56));

    let (mut present, mut outdated) = (0, 0);
    for skill in &available {
        match skill.state_in(&dir) {
            SkillState::UpToDate => {
                present += 1;
                println!(
                    "\x1b[32m✓\x1b[0m {:<22} {:<14} {}",
                    skill.name, skill.version, skill.version
                );
            }
            SkillState::Outdated => {
                present += 1;
                outdated += 1;
                let have = manifest
                    .get(&skill.name)
                    .map(|m| m.version.as_str())
                    .unwrap_or("?");
                println!(
                    "\x1b[33m↑\x1b[0m {:<22} {:<14} {}",
                    skill.name, have, skill.version
                );
            }
            SkillState::Missing => {
                println!(
                    "\x1b[2m·\x1b[0m {:<22} {:<14} {}",
                    skill.name, "—", skill.version
                );
            }
        }
    }

    println!();
    println!(
        "  {present}/{} installed  •  directory: {}",
        available.len(),
        dir.display()
    );
    if outdated > 0 {
        println!("  \x1b[33m{outdated} outdated\x1b[0m  →  run \x1b[1matlas skills update\x1b[0m");
    }
    Ok(())
}
