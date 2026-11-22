use anyhow::Result;
use atlas_i18n::t;
use atlas_skills::{InstallOutcome, embedded};

use super::{manifest_path, skills_dir};

/// Install or refresh skills from the copy embedded in this binary.
/// `names` empty = all. Only skills whose content differs are rewritten, so
/// this doubles as `update`.
pub fn handle(names: Vec<String>, global: bool) -> Result<()> {
    let dir = skills_dir(global);
    let mpath = manifest_path(global);

    let results = embedded::install(&dir, &mpath, &names)?;

    let (mut installed, mut updated) = (0, 0);
    for (name, outcome) in &results {
        let version = embedded::get(name).map(|s| s.version).unwrap_or_default();
        match outcome {
            InstallOutcome::Installed => {
                installed += 1;
                println!(
                    "  {name}  \x1b[32m✓\x1b[0m  {}",
                    t!("cmd_skills_installed", version = version)
                );
            }
            InstallOutcome::Updated => {
                updated += 1;
                println!(
                    "  {name}  \x1b[33m↑\x1b[0m  {}",
                    t!("cmd_skills_updated", version = version)
                );
            }
            InstallOutcome::UpToDate => {
                println!(
                    "  {name}  \x1b[2m─\x1b[0m  {}",
                    t!("cmd_skills_up_to_date", version = version)
                );
            }
        }
    }

    println!();
    let dir = dir.display().to_string();
    if installed + updated == 0 {
        println!("{}", t!("cmd_skills_nothing_changed", dir = &dir));
    } else {
        if installed > 0 {
            println!(
                "\x1b[32m✓\x1b[0m  {}",
                t!("cmd_skills_installed_total", count = installed, dir = &dir)
            );
        }
        if updated > 0 {
            println!(
                "\x1b[33m↑\x1b[0m  {}",
                t!("cmd_skills_updated_total", count = updated, dir = &dir)
            );
        }
    }
    Ok(())
}
