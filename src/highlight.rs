use grep::{
    matcher::{Match, Matcher},
    regex::RegexMatcherBuilder,
};

use promkit_core::{crossterm::style::ContentStyle, grapheme::StyledGraphemes};

/// Apply style to matched parts in the line.
pub fn highlight(
    query: &str,
    line: &str,
    highlight_style: ContentStyle,
    case_insensitive: bool,
) -> Option<StyledGraphemes> {
    let piped = &query
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>();

    let mut styled = StyledGraphemes::from(line);

    if query.is_empty() {
        Some(styled)
    } else {
        match matched(piped, line, case_insensitive) {
            Ok(matches) => {
                if matches.is_empty() {
                    None
                } else {
                    for m in matches {
                        for i in m.start()..m.end() {
                            styled = styled.apply_style_at(i, highlight_style);
                        }
                    }
                    Some(styled)
                }
            }
            _ => None,
        }
    }
}

fn matched(queries: &[&str], line: &str, case_insensitive: bool) -> anyhow::Result<Vec<Match>> {
    let mut matched = Vec::new();
    RegexMatcherBuilder::new()
        .case_insensitive(case_insensitive)
        .build_many(queries)?
        .find_iter_at(line.as_bytes(), 0, |m| {
            if m.start() >= line.len() {
                return false;
            }
            matched.push(m);
            true
        })?;
    Ok(matched)
}
