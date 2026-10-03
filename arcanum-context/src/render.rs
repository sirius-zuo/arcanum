use arcanum_core::types::{BackgroundItem, Passage, RenderFormat};

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Renders passages (already in output order, grouped by document) and
/// background summaries. Equals the concatenation of the fragment pieces,
/// with each wrapper's open part before its content and close part after.
pub fn render(format: RenderFormat, passages: &[Passage], background: &[BackgroundItem]) -> String {
    let f = Some(format);
    let mut out = String::new();
    if format == RenderFormat::Xml && !passages.is_empty() {
        out.push_str("<documents>\n");
    }
    let mut current: Option<(&str, u32)> = None;
    for p in passages {
        let key = (p.source_uri.as_str(), p.version_num);
        if current != Some(key) {
            if current.is_some() {
                out.push_str(document_close(f));
            }
            out.push_str(&document_open(f, key.0, key.1));
            current = Some(key);
        }
        out.push_str(&passage_fragment(f, p));
    }
    if current.is_some() {
        out.push_str(document_close(f));
    }
    if format == RenderFormat::Xml && !passages.is_empty() {
        out.push_str("</documents>\n");
    }
    if !background.is_empty() {
        out.push_str(background_open(f));
        for s in background {
            out.push_str(&summary_fragment(f, s));
        }
        out.push_str(background_close(f));
    }
    out
}

fn document_open(format: Option<RenderFormat>, source_uri: &str, version: u32) -> String {
    match format {
        Some(RenderFormat::Xml) => {
            format!(
                "<document source=\"{}\" version=\"{version}\">\n",
                esc(source_uri)
            )
        }
        Some(RenderFormat::Markdown) => format!("### {source_uri} (v{version})\n\n"),
        _ => String::new(),
    }
}

fn document_close(format: Option<RenderFormat>) -> &'static str {
    match format {
        Some(RenderFormat::Xml) => "</document>\n",
        _ => "",
    }
}

fn background_open(format: Option<RenderFormat>) -> &'static str {
    match format {
        Some(RenderFormat::Numbered) => "Background:\n",
        Some(RenderFormat::Xml) => "<background>\n",
        Some(RenderFormat::Markdown) => "#### Background\n\n",
        None => "",
    }
}

fn background_close(format: Option<RenderFormat>) -> &'static str {
    match format {
        Some(RenderFormat::Xml) => "</background>\n",
        _ => "",
    }
}

pub fn passage_fragment(format: Option<RenderFormat>, p: &Passage) -> String {
    match format {
        None => p.text.clone(),
        Some(RenderFormat::Numbered) => format!(
            "[{}] {} (v{})\n{}\n\n",
            p.ref_id, p.source_uri, p.version_num, p.text
        ),
        Some(RenderFormat::Xml) => format!(
            "<passage ref=\"{}\">{}</passage>\n",
            esc(&p.ref_id),
            esc(&p.text)
        ),
        Some(RenderFormat::Markdown) => format!("[{}] {}\n\n", p.ref_id, p.text),
    }
}

/// Full per-document overhead (open and close parts together).
pub fn document_wrapper(format: Option<RenderFormat>, source_uri: &str, version: u32) -> String {
    format!(
        "{}{}",
        document_open(format, source_uri, version),
        document_close(format)
    )
}

/// Full envelope overhead around all documents.
pub fn documents_envelope(format: Option<RenderFormat>) -> String {
    match format {
        Some(RenderFormat::Xml) => "<documents>\n</documents>\n".to_string(),
        _ => String::new(),
    }
}

pub fn summary_fragment(format: Option<RenderFormat>, s: &BackgroundItem) -> String {
    match format {
        None => s.text.clone(),
        Some(RenderFormat::Numbered) => format!("[{}] {}\n", s.ref_id, s.text),
        Some(RenderFormat::Xml) => format!(
            "<summary ref=\"{}\">{}</summary>\n",
            esc(&s.ref_id),
            esc(&s.text)
        ),
        Some(RenderFormat::Markdown) => format!("[{}] {}\n\n", s.ref_id, s.text),
    }
}

/// Full background overhead (open and close parts together).
pub fn background_wrapper(format: Option<RenderFormat>) -> String {
    format!("{}{}", background_open(format), background_close(format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;

    fn p(r: &str, doc: &str, text: &str) -> Passage {
        Passage {
            ref_id: r.into(),
            document_id: DocumentId::new(),
            version_num: 2,
            source_uri: format!("raw://{doc}"),
            snapshot_uri: String::new(),
            canonical_uri: None,
            section: None,
            page: None,
            offset_start: 0,
            offset_end: 0,
            text: text.into(),
            chunk_ids: vec![],
            strategies: vec![],
            score: 0.0,
        }
    }

    fn s(r: &str, text: &str) -> BackgroundItem {
        BackgroundItem {
            ref_id: r.into(),
            text: text.into(),
            level: 1,
            document_id: DocumentId::new(),
            covers: vec![],
            score: 0.0,
        }
    }

    fn fixture() -> (Vec<Passage>, Vec<BackgroundItem>) {
        (
            vec![
                p("P1", "a", "one"),
                p("P2", "a", "two"),
                p("P3", "b", "three"),
            ],
            vec![s("S1", "sum")],
        )
    }

    #[test]
    fn numbered_golden() {
        let (ps, bg) = fixture();
        assert_eq!(
            render(RenderFormat::Numbered, &ps, &bg),
            "[P1] raw://a (v2)\none\n\n[P2] raw://a (v2)\ntwo\n\n[P3] raw://b (v2)\nthree\n\nBackground:\n[S1] sum\n"
        );
    }

    #[test]
    fn xml_golden() {
        let (ps, bg) = fixture();
        assert_eq!(
            render(RenderFormat::Xml, &ps, &bg),
            "<documents>\n<document source=\"raw://a\" version=\"2\">\n<passage ref=\"P1\">one</passage>\n<passage ref=\"P2\">two</passage>\n</document>\n<document source=\"raw://b\" version=\"2\">\n<passage ref=\"P3\">three</passage>\n</document>\n</documents>\n<background>\n<summary ref=\"S1\">sum</summary>\n</background>\n"
        );
    }

    #[test]
    fn markdown_golden() {
        let (ps, bg) = fixture();
        assert_eq!(
            render(RenderFormat::Markdown, &ps, &bg),
            "### raw://a (v2)\n\n[P1] one\n\n[P2] two\n\n### raw://b (v2)\n\n[P3] three\n\n#### Background\n\n[S1] sum\n\n"
        );
    }

    #[test]
    fn xml_escapes_text_and_attributes() {
        let mut ps = vec![p("P1", "a", "x < y & \"z\" > w")];
        ps[0].source_uri = "a\"b<c>&".into();
        let out = render(RenderFormat::Xml, &ps, &[]);
        assert!(out.contains("source=\"a&quot;b&lt;c&gt;&amp;\""));
        assert!(out.contains(">x &lt; y &amp; &quot;z&quot; &gt; w<"));
    }

    #[test]
    fn empty_selection_renders_empty_string() {
        for f in [
            RenderFormat::Numbered,
            RenderFormat::Xml,
            RenderFormat::Markdown,
        ] {
            assert_eq!(render(f, &[], &[]), "");
        }
    }

    #[test]
    fn text_only_fragments_have_no_wrappers() {
        let (ps, bg) = fixture();
        assert_eq!(passage_fragment(None, &ps[0]), "one");
        assert_eq!(summary_fragment(None, &bg[0]), "sum");
        assert_eq!(document_wrapper(None, "u", 1), "");
        assert_eq!(documents_envelope(None), "");
        assert_eq!(background_wrapper(None), "");
    }

    #[test]
    fn render_equals_concatenated_fragments() {
        let (ps, bg) = fixture();
        for f in [
            RenderFormat::Numbered,
            RenderFormat::Xml,
            RenderFormat::Markdown,
        ] {
            let o = Some(f);
            let mut expected = documents_envelope(o).len() + background_wrapper(o).len();
            expected += document_wrapper(o, "raw://a", 2).len();
            expected += document_wrapper(o, "raw://b", 2).len();
            expected += ps
                .iter()
                .map(|x| passage_fragment(o, x).len())
                .sum::<usize>();
            expected += bg
                .iter()
                .map(|x| summary_fragment(o, x).len())
                .sum::<usize>();
            assert_eq!(render(f, &ps, &bg).len(), expected, "{f:?}");
        }
    }
}
