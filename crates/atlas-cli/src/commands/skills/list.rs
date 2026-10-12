use anyhow::Result;

pub fn handle() -> Result<()> {
    let skills = atlas_skills::embedded::all();

    println!(
        "{:<24} {:<10} \x1b[1mDESCRIPTION\x1b[0m",
        "\x1b[1mNAME\x1b[0m", "\x1b[1mVERSION\x1b[0m"
    );
    println!("{}", "─".repeat(72));

    for skill in &skills {
        println!(
            "{:<24} {:<10} {}",
            skill.name, skill.version, skill.description
        );
    }

    println!();
    println!(
        "  {} skill(s) bundled with atlas v{}",
        skills.len(),
        env!("CARGO_PKG_VERSION")
    );
    println!("  Run \x1b[1matlas skills install\x1b[0m to install all");

    Ok(())
}
