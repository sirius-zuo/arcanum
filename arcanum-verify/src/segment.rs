use arcanum_generate::scan_markers;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub span: (usize, usize),
    pub code: bool,
}

/// Trim whitespace from both ends of `s[start..end]`, returning absolute span.
fn trimmed(s: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let piece = &s[start..end];
    let t = piece.trim_start_matches(char::is_whitespace);
    let lead = piece.len() - t.len();
    let t = t.trim_end_matches(char::is_whitespace);
    if t.is_empty() {
        None
    } else {
        Some((start + lead, start + lead + t.len()))
    }
}

/// Split one line into sentence spans (relative to `line`), keeping citation
/// marker groups attached to the sentence they follow.
fn split_line(line: &str) -> Vec<(usize, usize)> {
    let groups: Vec<(usize, usize)> = scan_markers(line).into_iter().map(|(sp, _)| sp).collect();
    let mut bounds: Vec<usize> = line
        .split_sentence_bound_indices()
        .map(|(i, _)| i)
        .filter(|&i| i > 0)
        .collect();
    let mut added = Vec::new();
    for (i, &(gs, ge)) in groups.iter().enumerate() {
        // unicode-segmentation may put the boundary at the group start or
        // just inside it (`[` is a Close character).
        let at_start = bounds.iter().any(|&b| gs <= b && b < ge);
        bounds.retain(|&b| b < gs || b >= ge);
        if at_start {
            // Marker trails the previous sentence: split after the marker
            // cluster (adjacent groups) and its trailing whitespace.
            let mut end = ge;
            for &(ns, ne) in &groups[i + 1..] {
                if line[end..ns].chars().all(char::is_whitespace) {
                    end = ne;
                } else {
                    break;
                }
            }
            let rest = &line[end..];
            end += rest.len() - rest.trim_start_matches(char::is_whitespace).len();
            if end < line.len() {
                added.push(end);
            }
        }
    }
    bounds.extend(added);
    bounds.sort_unstable();
    bounds.dedup();
    let mut spans = Vec::new();
    let mut prev = 0;
    for b in bounds.into_iter().chain(std::iter::once(line.len())) {
        if b > prev {
            spans.push((prev, b));
            prev = b;
        }
    }
    spans
}

pub fn segment(answer: &str) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut fence_start: Option<usize> = None;
    let mut offset = 0;
    for line in answer.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let is_fence = line.trim_start().starts_with("```");
        match fence_start {
            Some(start) => {
                if is_fence {
                    if let Some(span) = trimmed(answer, start, offset) {
                        units.push(Unit { span, code: true });
                    }
                    fence_start = None;
                }
            }
            None if is_fence => fence_start = Some(line_start),
            None => {
                let body = line.strip_suffix('\n').unwrap_or(line);
                for (s, e) in split_line(body) {
                    if let Some(span) = trimmed(answer, line_start + s, line_start + e) {
                        units.push(Unit { span, code: false });
                    }
                }
            }
        }
    }
    if let Some(start) = fence_start {
        if let Some(span) = trimmed(answer, start, answer.len()) {
            units.push(Unit { span, code: true });
        }
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(a: &str) -> Vec<&str> {
        segment(a).iter().map(|u| &a[u.span.0..u.span.1]).collect()
    }

    #[test]
    fn marker_after_full_stop_joins_previous_sentence() {
        assert_eq!(
            texts("Acme builds rockets. [P1] It was founded in 1998 [P2]."),
            vec!["Acme builds rockets. [P1]", "It was founded in 1998 [P2]."]
        );
    }
    #[test]
    fn marker_before_full_stop_and_without_space() {
        assert_eq!(texts("foo [P1]. Bar.[P2]"), vec!["foo [P1].", "Bar.[P2]"]);
    }
    #[test]
    fn cjk_markers_stay_whole() {
        assert_eq!(
            texts("Acme 成立于1998年。[P1]它在柏林。[P2]"),
            vec!["Acme 成立于1998年。[P1]", "它在柏林。[P2]"]
        );
    }
    #[test]
    fn lines_split_and_blank_lines_drop() {
        assert_eq!(
            texts("Intro:\n- Bob works there [P1]\n\n  \n- He mentors hires [P2]"),
            vec![
                "Intro:",
                "- Bob works there [P1]",
                "- He mentors hires [P2]"
            ]
        );
    }
    #[test]
    fn crlf_is_trimmed() {
        assert_eq!(texts("One.\r\nTwo."), vec!["One.", "Two."]);
    }
    #[test]
    fn leading_marker_stays_in_first_unit() {
        assert_eq!(
            texts("[P1] Acme builds rockets."),
            vec!["[P1] Acme builds rockets."]
        );
    }
    #[test]
    fn fenced_block_is_one_code_unit() {
        let a = "See:\n```rust\nlet a = 1. [P1]\n```\nDone.";
        assert_eq!(
            texts(a),
            vec!["See:", "```rust\nlet a = 1. [P1]\n```", "Done."]
        );
        assert_eq!(
            segment(a).iter().map(|u| u.code).collect::<Vec<_>>(),
            vec![false, true, false]
        );
        assert_eq!(texts("x\n```\nopen"), vec!["x", "```\nopen"]);
    }
}
