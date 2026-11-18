use anyhow::Result;
use atlas_i18n::t;
use atlas_skills::{SkillState, SkillsManifest, embedded};

use super::{manifest_path, skills_dir};

pub fn handle(global: bool) -> Result<()> {
    let dir = skills_dir(global);
    let manifest = SkillsManifest::load_from(&manifest_path(global))?;
    let available = embedded::all();

    // Pad the translated header by characters, then style the whole cell: the width is measured on
    // the words, not on the escape codes.
    let (skill, installed) = (t!("cmd_skills_col_skill"), t!("cmd_skills_col_installed"));
    println!(
        "\x1b[1m{skill:<24}\x1b[0m \x1b[1m{installed:<14}\x1b[0m \x1b[1m{}\x1b[0m",
        t!("cmd_skills_col_bundled")
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
        "  {}",
        t!(
            "cmd_skills_status_total",
            present = present,
            total = available.len(),
            dir = dir.display().to_string()
        )
    );
    if outdated > 0 {
        println!(
            "  \x1b[33m{}\x1b[0m  →  {}",
            t!("cmd_skills_outdated_count", count = outdated),
            t!(
                "cmd_skills_update_hint",
                command = "\x1b[1matlas skills update\x1b[0m"
            )
        );
    }
    Ok(())
}
