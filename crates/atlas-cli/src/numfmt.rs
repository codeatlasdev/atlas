//! Decimal numbers for a person: the separator follows the display language (`5.5 MiB`, `5,5 MiB` in pt-BR). The
//! message evaluator formats no numbers, so a command formats the value and passes it as text.

/// `v` with `digits` decimals in the decimal separator of `tag`.
pub fn fixed(tag: &str, v: f64, digits: usize) -> String {
    let s = format!("{v:.digits$}");
    if tag.starts_with("pt") {
        s.replace('.', ",")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::fixed;

    #[test]
    fn the_separator_follows_the_language() {
        assert_eq!(fixed("en", 5.5, 1), "5.5");
        assert_eq!(fixed("pt-BR", 5.5, 1), "5,5");
        assert_eq!(fixed("pt-BR", 12.0, 0), "12");
        assert_eq!(fixed("pt-BR", -0.25, 2), "-0,25");
    }
}
