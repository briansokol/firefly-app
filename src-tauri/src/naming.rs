/// System prompt for the on-device "quick" model when naming a conversation.
pub const NAMING_SYSTEM_PROMPT: &str = "You name chat conversations. Given the \
user's first message, reply with a short title of 2 to 5 words that captures the \
topic. Reply with ONLY the title: no quotes, no punctuation at the end, no \
preamble, no explanation.";

/// Clean a raw model title into a short, single-line label.
/// Returns `None` when nothing usable remains (caller then leaves the title as-is).
pub fn clean_title(raw: &str) -> Option<String> {
    let first_line = raw.lines().find(|l| !l.trim().is_empty())?.trim();

    // Strip matched surrounding quotes, repeatedly.
    let mut s = first_line.to_string();
    loop {
        let bytes = s.as_bytes();
        let paired = matches!(
            (bytes.first().copied(), bytes.last().copied()),
            (Some(b'"'), Some(b'"')) | (Some(b'\''), Some(b'\''))
        ) && s.chars().count() >= 2;
        let curly = s.starts_with('\u{201c}') && s.ends_with('\u{201d}');
        if paired || curly {
            let start = s.char_indices().nth(1).map(|(i, _)| i).unwrap_or(0);
            let end = s.char_indices().last().map(|(i, _)| i).unwrap_or(s.len());
            s = s[start..end].trim().to_string();
        } else {
            break;
        }
    }

    // Collapse internal whitespace.
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Drop a trailing period (keep ? and !).
    let trimmed = collapsed.trim_end_matches('.').trim().to_string();
    if trimmed.is_empty() {
        return None;
    }

    // Cap at 6 words.
    let mut words: Vec<&str> = trimmed.split(' ').collect();
    if words.len() > 6 {
        words.truncate(6);
    }
    let mut out = words.join(" ");

    // Hard cap length at 48 chars, breaking on a word boundary.
    if out.chars().count() > 48 {
        let cut: String = out.chars().take(48).collect();
        out = match cut.rsplit_once(' ') {
            Some((head, _)) => head.to_string(),
            None => cut,
        };
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_surrounding_quotes() {
        assert_eq!(clean_title("\"Tax filing help\"").as_deref(), Some("Tax filing help"));
        assert_eq!(clean_title("'Dinner ideas'").as_deref(), Some("Dinner ideas"));
    }

    #[test]
    fn takes_first_nonempty_line_and_drops_trailing_period() {
        assert_eq!(clean_title("\n  Trip to Japan.\nextra").as_deref(), Some("Trip to Japan"));
    }

    #[test]
    fn collapses_whitespace_and_caps_words() {
        assert_eq!(
            clean_title("one   two three four five six seven eight").as_deref(),
            Some("one two three four five six")
        );
    }

    #[test]
    fn empty_or_whitespace_is_none() {
        assert_eq!(clean_title("   \n  "), None);
        assert_eq!(clean_title(""), None);
    }
}
