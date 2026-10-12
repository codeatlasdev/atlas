//! Tiny subsequence matcher for the command palette. A hand-rolled scorer
//! (~40 lines) instead of `nucleo-matcher`: the palette holds tens of entries,
//! so a new dependency buys nothing.

/// Score `query` against `text` (case-insensitive subsequence match).
/// Returns the score and the matched char indices into `text`, or `None`.
/// Rewards consecutive runs, word starts and an early first match.
pub fn score(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let chars: Vec<char> = text.chars().collect();
    let mut qi = query.chars().flat_map(char::to_lowercase).peekable();
    let mut hits = Vec::new();
    let mut total = 0i32;
    let mut prev: Option<usize> = None;
    for (i, c) in chars.iter().enumerate() {
        let Some(&q) = qi.peek() else { break };
        if c.to_lowercase().next() == Some(q) {
            let word_start = i == 0 || !chars[i - 1].is_alphanumeric();
            total += 1;
            if prev == Some(i.wrapping_sub(1)) {
                total += 4;
            }
            if word_start {
                total += 3;
            }
            if hits.is_empty() {
                total -= (i as i32).min(8);
            }
            prev = Some(i);
            hits.push(i);
            qi.next();
        }
    }
    qi.peek().is_none().then_some((total, hits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_matches_in_order_only() {
        assert!(score("rsa", "Restart api").is_some());
        assert!(score("asr", "Restart api").is_none());
        assert!(score("", "x").is_some());
    }

    #[test]
    fn returns_matched_indices() {
        let (_, hits) = score("ra", "Restart api").unwrap();
        assert_eq!(hits[0], 0);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn contiguous_and_word_start_beat_scattered() {
        let (tight, _) = score("api", "Stop api").unwrap();
        let (loose, _) = score("api", "xaxxxxxpxxxxxxi").unwrap();
        assert!(tight > loose);
    }

    #[test]
    fn case_insensitive() {
        assert!(score("RESTART", "restart all").is_some());
    }
}
