//! What a spoken transcript means to a card (#648). A client sends only what was heard; this module, and the
//! functions in `approval` / `handlers::tasks` that use it, decide. A client never turns words into `allow: true`.
//!
//! - **Approvals** are exact phrases only. After trimming, lowercasing, folding accents, turning commas into
//!   spaces and dropping final punctuation, the whole transcript must be one of the phrases of [`ALLOW`] or
//!   [`DENY`]. Anything else is a [`Heard::Comment`]: it decides nothing, and the client offers it as a comment
//!   to confirm by tap (`allow but only this file`, `don't allow`, `allow allow`, `permitir nesta tarefa`).
//!   Nothing with letters in it is ever "unclear"; only an empty or letterless transcript is.
//! - **Choices** resolve to the index of the option the transcript names: a number (`2`, `two`, `dois`), an
//!   ordinal (`the second`, `a segunda`), the option's whole label, or a prefix of one label of at least
//!   [`MIN_PREFIX`] characters. Two different readings of the same words (a label that is also another option's
//!   number, two labels sharing the prefix) resolve to nothing: hostd never guesses.

/// Phrases that allow (once), by language.
const ALLOW: &[(&str, &[&str])] = &[
    ("en", &["allow", "approve", "yes allow"]),
    ("pt", &["permitir", "permite", "aprovar", "aprova"]),
];
/// Phrases that deny, by language.
const DENY: &[(&str, &[&str])] = &[
    ("en", &["deny", "reject", "no deny"]),
    ("pt", &["negar", "nega", "recusar", "recusa"]),
];

/// The shortest label prefix that names an option.
pub(crate) const MIN_PREFIX: usize = 3;

/// What an approval transcript says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Heard {
    Allow,
    Deny,
    /// Words that are not exactly an approval phrase: offered as a comment, nothing is decided.
    Comment,
    /// Nothing to hear.
    Unclear,
}

/// `s` lowercased, accents folded (pt and en alike), commas as spaces, spaces collapsed, final punctuation gone.
pub(crate) fn normalize(s: &str) -> String {
    let folded: String = s
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            ',' | '\u{ff0c}' => ' ',
            c => c,
        })
        .collect();
    let spaced = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    spaced
        .trim_end_matches(|c: char| c.is_whitespace() || ".!?;:\u{2026}\u{3002}".contains(c))
        .to_string()
}

fn in_table(table: &[(&str, &[&str])], lang: Option<&str>, phrase: &str) -> bool {
    table
        .iter()
        .filter(|(l, _)| lang.is_none_or(|want| want.eq_ignore_ascii_case(l)))
        .any(|(_, phrases)| phrases.contains(&phrase))
}

/// What `transcript` says to a permission prompt. `lang`: `en`, `pt`, or `None` for both tables.
pub(crate) fn approval(transcript: &str, lang: Option<&str>) -> Heard {
    let t = normalize(transcript);
    if !t.chars().any(char::is_alphanumeric) {
        return Heard::Unclear;
    }
    if in_table(ALLOW, lang, &t) {
        Heard::Allow
    } else if in_table(DENY, lang, &t) {
        Heard::Deny
    } else {
        Heard::Comment
    }
}

// ── choices ──────────────────────────────────────────────────────────────────

/// Words that only frame a number: "the second", "a segunda", "option two", "numero 3".
const FILLER: &[&str] = &[
    "the", "a", "o", "as", "os", "number", "numero", "option", "opcao", "choice", "n",
];

/// The 1-based position a number word, digit or ordinal says, in either language.
fn position(token: &str) -> Option<usize> {
    const WORDS: &[&[&str]] = &[
        &["one", "first", "um", "uma", "primeiro", "primeira"],
        &["two", "second", "dois", "duas", "segundo", "segunda"],
        &["three", "third", "tres", "terceiro", "terceira"],
        &["four", "fourth", "quatro", "quarto", "quarta"],
        &["five", "fifth", "cinco", "quinto", "quinta"],
        &["six", "sixth", "seis", "sexto", "sexta"],
        &["seven", "seventh", "sete", "setimo", "setima"],
        &["eight", "eighth", "oito", "oitavo", "oitava"],
        &["nine", "ninth", "nove", "nono", "nona"],
        &["ten", "tenth", "dez", "decimo", "decima"],
    ];
    if let Some(i) = WORDS.iter().position(|w| w.contains(&token)) {
        return Some(i + 1);
    }
    // `2`, `2nd`, `3rd`, `4th`, `2º`, `2ª`
    let digits = token.trim_end_matches(|c: char| !c.is_ascii_digit());
    let suffix = &token[digits.len()..];
    if !digits.is_empty()
        && digits.len() <= 3
        && digits.chars().all(|c| c.is_ascii_digit())
        && matches!(
            suffix,
            "" | "st" | "nd" | "rd" | "th" | "\u{ba}" | "\u{aa}" | "o" | "a"
        )
    {
        return digits.parse().ok().filter(|n| *n > 0);
    }
    None
}

/// The position the whole transcript says ("the second" is 2), when it says nothing else.
fn spoken_position(t: &str) -> Option<usize> {
    let mut found = None;
    for token in t.split(' ') {
        if FILLER.contains(&token) {
            continue;
        }
        // Two numbers in one answer ("one or two") name nothing.
        if found.is_some() {
            return None;
        }
        found = Some(position(token)?);
    }
    found
}

/// The index of the option `transcript` names, or `None` (the client says it was not understood).
pub(crate) fn choice(transcript: &str, options: &[String]) -> Option<usize> {
    let t = normalize(transcript);
    if t.is_empty() || options.is_empty() {
        return None;
    }
    let labels: Vec<String> = options.iter().map(|o| normalize(o)).collect();
    // What the words can mean: the option at a position, an option whose whole label they are.
    let said = spoken_position(&t);
    let mut readings: Vec<usize> = vec![];
    if let Some(n) = said.filter(|n| *n <= options.len()) {
        readings.push(n - 1);
    }
    // A label is the words themselves, or a number saying the same number ("2" for "two").
    readings.extend(
        labels
            .iter()
            .enumerate()
            .filter(|(_, l)| **l == t || said.is_some() && spoken_position(l) == said)
            .map(|(i, _)| i),
    );
    readings.sort_unstable();
    readings.dedup();
    match readings.as_slice() {
        [one] => return Some(*one),
        [] => {}
        // Position and label disagree (or two options carry the same label): never guess.
        _ => return None,
    }
    if t.chars().count() < MIN_PREFIX {
        return None;
    }
    let mut starts = labels
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with(&t))
        .map(|(i, _)| i);
    match (starts.next(), starts.next()) {
        (Some(i), None) => Some(i),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The exact-phrase rule, one row per way a person says it, in both languages. Anything that is not exactly a
    // phrase must come back as a comment, never as an approval.
    #[test]
    fn only_an_exact_unqualified_phrase_decides() {
        let rows: &[(&str, Option<&str>, Heard)] = &[
            ("allow", None, Heard::Allow),
            ("Allow.", None, Heard::Allow),
            ("  APPROVE!  ", Some("en"), Heard::Allow),
            ("yes allow", Some("en"), Heard::Allow),
            ("Yes, allow.", Some("en"), Heard::Allow),
            ("deny", None, Heard::Deny),
            ("Reject.", Some("en"), Heard::Deny),
            ("no deny", None, Heard::Deny),
            ("No, deny", None, Heard::Deny),
            ("permitir", None, Heard::Allow),
            ("Permite.", Some("pt"), Heard::Allow),
            ("aprovar", Some("pt"), Heard::Allow),
            ("Aprova!", None, Heard::Allow),
            ("negar", None, Heard::Deny),
            ("Nega.", Some("pt"), Heard::Deny),
            ("recusar", None, Heard::Deny),
            ("Recusa", Some("pt"), Heard::Deny),
            // the traps: a qualified answer is a comment
            ("allow but only this file", None, Heard::Comment),
            ("allow but only once", None, Heard::Comment),
            ("don't allow", None, Heard::Comment),
            ("do not allow", None, Heard::Comment),
            ("not allow", None, Heard::Comment),
            ("allow allow", None, Heard::Comment),
            ("allow, deny", None, Heard::Comment),
            ("allow it", None, Heard::Comment),
            ("denied", None, Heard::Comment),
            ("não permitir", None, Heard::Comment),
            ("nao permitir", None, Heard::Comment),
            ("permitir nesta tarefa", None, Heard::Comment),
            ("permitir só este arquivo", Some("pt"), Heard::Comment),
            ("aprovar e negar", None, Heard::Comment),
            ("yes", None, Heard::Comment),
            ("open the file", None, Heard::Comment),
            // a phrase of the other language is not a phrase when the language is known
            ("allow", Some("pt"), Heard::Comment),
            ("permitir", Some("en"), Heard::Comment),
            ("deny", Some("pt"), Heard::Comment),
            // nothing to hear
            ("", None, Heard::Unclear),
            ("   ", None, Heard::Unclear),
            ("...", None, Heard::Unclear),
            ("?!", Some("en"), Heard::Unclear),
        ];
        for (said, lang, want) in rows {
            assert_eq!(approval(said, *lang), *want, "{said:?} ({lang:?})");
        }
    }

    fn opts(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_choice_is_named_by_number_ordinal_or_label_in_both_languages() {
        let o = opts(&["Use main", "Create a branch", "Cancel it"]);
        let rows: &[(&str, Option<usize>)] = &[
            ("1", Some(0)),
            ("two", Some(1)),
            ("Three.", Some(2)),
            ("the second", Some(1)),
            ("the first one", None),
            ("option 3", Some(2)),
            ("um", Some(0)),
            ("dois", Some(1)),
            ("a segunda", Some(1)),
            ("a terceira.", Some(2)),
            ("opção 2", Some(1)),
            ("2nd", Some(1)),
            ("2º", Some(1)),
            ("use main", Some(0)),
            ("USE MAIN.", Some(0)),
            ("create a branch", Some(1)),
            // a unique prefix of at least three characters
            ("can", Some(2)),
            ("create", Some(1)),
            // not understood: out of range, no match, two numbers, too short a prefix
            ("four", None),
            ("7", None),
            ("0", None),
            ("one or two", None),
            ("banana", None),
            ("us", None),
            ("", None),
            ("allow", None),
        ];
        for (said, want) in rows {
            assert_eq!(choice(said, &o), *want, "{said:?}");
        }
    }

    #[test]
    fn labels_match_without_regard_to_accents_or_case() {
        let o = opts(&["Usar a main", "Criar uma ramificação", "Não fazer nada"]);
        assert_eq!(choice("criar uma ramificacao", &o), Some(1));
        assert_eq!(choice("NAO FAZER NADA", &o), Some(2));
        assert_eq!(
            choice("ramif", &o),
            None,
            "a prefix starts the label, it is not any part of it"
        );
        assert_eq!(choice("ncriar", &o), None);
        assert_eq!(choice("usar", &o), Some(0));
    }

    // Two options that start with the same words: the prefix names neither, the whole label names one.
    #[test]
    fn a_prefix_two_options_share_names_none_but_a_whole_label_names_one() {
        let o = opts(&["Deploy to staging", "Deploy to production", "Deploy"]);
        assert_eq!(choice("deploy to", &o), None);
        assert_eq!(choice("deploy t", &o), None);
        assert_eq!(choice("deploy to staging", &o), Some(0));
        assert_eq!(
            choice("deploy", &o),
            Some(2),
            "the whole label wins over being a prefix of the others"
        );
        assert_eq!(choice("deploy to p", &o), Some(1));
    }

    // A label that is itself a number: the card says "2" and "1", and a person saying "two" may mean the label or
    // the position. Naming two different options is never guessed; naming one is fine.
    #[test]
    fn a_label_that_is_also_a_number_is_never_guessed() {
        let swapped = opts(&["2", "1", "3"]);
        assert_eq!(
            choice("2", &swapped),
            None,
            "label 2 is first, position 2 is second"
        );
        assert_eq!(choice("1", &swapped), None);
        assert_eq!(
            choice("two", &swapped),
            None,
            "the label 2 says the same number as the second position"
        );
        assert_eq!(
            choice("three", &swapped),
            Some(2),
            "label 3 and position 3 are the same option"
        );
        let words = opts(&["Two", "One"]);
        assert_eq!(
            choice("two", &words),
            None,
            "the label Two is first, the second position is One"
        );
        let aligned = opts(&["1", "2"]);
        assert_eq!(choice("2", &aligned), Some(1));
        let same = opts(&["Yes", "Yes"]);
        assert_eq!(choice("yes", &same), None, "two options with one label");
    }
}
