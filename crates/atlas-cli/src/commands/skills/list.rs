use anyhow::Result;
use atlas_i18n::t;

pub fn handle() -> Result<()> {
    let skills = atlas_skills::embedded::all();

    // Pad the translated header by characters, then style the whole cell: the width is measured on
    // the words, not on the escape codes.
    let (name, version) = (t!("cmd_skills_col_name"), t!("cmd_skills_col_version"));
    println!(
        "\x1b[1m{name:<24}\x1b[0m \x1b[1m{version:<10}\x1b[0m \x1b[1m{}\x1b[0m",
        t!("cmd_skills_col_description")
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
        "  {}",
        t!(
            "cmd_skills_list_total",
            count = skills.len(),
            version = env!("CARGO_PKG_VERSION")
        )
    );
    println!(
        "  {}",
        t!(
            "cmd_skills_install_hint",
            command = "\x1b[1matlas skills install\x1b[0m"
        )
    );

    Ok(())
}
