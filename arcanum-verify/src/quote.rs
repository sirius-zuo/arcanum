//! Locate a quoted span inside passage text.

/// Byte range of `quote` within `text`. Tries an exact match first, then a
/// whitespace-insensitive match mapped back to original byte offsets.
pub fn locate(text: &str, quote: &str) -> Option<(usize, usize)> {
    if quote.trim().is_empty() {
        return None;
    }
    if let Some(i) = text.find(quote) {
        return Some((i, i + quote.len()));
    }
    let (norm, starts, ends) = normalize(text);
    let needle = collapse(quote.trim());
    let i = norm.find(&needle)?;
    Some((starts[i], ends[i + needle.len() - 1]))
}

fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// Collapsed text plus, for each normalized byte, the original start and end
/// byte of the char (or whitespace run) it came from.
fn normalize(text: &str) -> (String, Vec<usize>, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut starts = Vec::with_capacity(text.len());
    let mut ends = Vec::with_capacity(text.len());
    let mut run: Option<usize> = None; // index into `ends` of the open space
    for (i, c) in text.char_indices() {
        let end = i + c.len_utf8();
        if c.is_whitespace() {
            match run {
                Some(k) => ends[k] = end,
                None => {
                    run = Some(out.len());
                    out.push(' ');
                    starts.push(i);
                    ends.push(end);
                }
            }
        } else {
            run = None;
            for _ in 0..c.len_utf8() {
                starts.push(i);
                ends.push(end);
            }
            out.push(c);
        }
    }
    (out, starts, ends)
}

#[cfg(test)]
mod tests {
    use super::*;
    const DOC: &str = "The quick brown fox jumps over the lazy dog.";

    #[test]
    fn locate_cases() {
        assert_eq!(locate(DOC, "brown fox jumps"), Some((10, 25)));
        assert_eq!(
            locate("Bob  works\n at Acme", "Bob works at Acme"),
            Some((0, 19))
        );
        assert_eq!(locate("café au lait", "au lait"), Some((6, 13)));
        assert_eq!(locate("x café  au\tlait", "café au lait"), Some((2, 16)));
        assert_eq!(locate(DOC, "purple"), None);
        assert_eq!(locate(DOC, "  "), None);
        assert_eq!(locate("ab ab", "ab"), Some((0, 2)));
    }
}
