use std::path::Path;

use atlas::samples::{load_manifest, read_sample};

fn dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/samples"))
}

#[test]
fn manifest_loads() {
    let m = load_manifest(dir()).unwrap();
    assert_eq!(m.files.iter().filter(|f| !f.is_update).count(), 10);
    assert_eq!(m.files.iter().filter(|f| f.is_update).count(), 1);
    for f in &m.files {
        assert!(
            !f.title.is_empty() && !f.description.is_empty(),
            "{}",
            f.path
        );
        assert!(!read_sample(dir(), f).unwrap().is_empty(), "{}", f.path);
    }
}

#[test]
fn update_shares_source_uri() {
    let m = load_manifest(dir()).unwrap();
    let update = m.files.iter().find(|f| f.is_update).unwrap();
    assert_eq!(update.source_uri, "security-policy.md");
    assert!(m
        .files
        .iter()
        .any(|f| !f.is_update && f.source_uri == update.source_uri));
}

#[test]
fn golden_points_at_real_files() {
    let m = load_manifest(dir()).unwrap();
    assert_eq!(m.golden.len(), 12);
    for g in &m.golden {
        assert!(!g.query.is_empty());
        assert!(
            m.files
                .iter()
                .any(|f| f.source_uri == g.relevant_source_uri),
            "{}",
            g.relevant_source_uri
        );
    }
}

#[test]
fn flawed_answers_complete() {
    let m = load_manifest(dir()).unwrap();
    let ids: Vec<&str> = m.flawed_answers.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["supported", "flawed", "uncited"]);
    for a in &m.flawed_answers {
        assert!(!a.question.is_empty() && !a.answer.is_empty() && !a.title.is_empty());
        assert!(a.answer.contains("[P"), "{}", a.id);
        assert!(!a.expected.is_empty());
    }
}

#[test]
fn tour_has_nine_steps() {
    let m = load_manifest(dir()).unwrap();
    let got: Vec<(&str, &str, &str)> = m
        .tour
        .iter()
        .map(|t| (t.id.as_str(), t.route.as_str(), t.completes_when.as_str()))
        .collect();
    let want = [
        ("load", "/library", "corpus_loaded"),
        ("search", "/search", "searched"),
        ("context", "/context", "context_built"),
        ("ask", "/ask", "asked"),
        ("verify", "/ask", "verified"),
        ("catch", "/verify", "flawed_checked"),
        ("update", "/library", "update_applied"),
        ("trace", "/evidence", "evidence_opened"),
        ("measure", "/lab", "evaluated"),
    ];
    assert_eq!(got, want);
    for t in &m.tour {
        assert!(!t.title.is_empty() && !t.why.is_empty() && !t.action.is_empty());
        assert!(!t.payoff.is_empty());
    }
}

#[test]
fn corpus_hygiene() {
    let m = load_manifest(dir()).unwrap();
    for f in &m.files {
        let text = String::from_utf8(read_sample(dir(), f).unwrap()).unwrap();
        assert!(!text.contains('\u{2014}'), "em-dash in {}", f.path);
        if !f.is_update {
            let words = text.split_whitespace().count();
            assert!((250..=800).contains(&words), "{}: {} words", f.path, words);
        }
    }
    for name in ["golden.json", "flawed-answers.json", "tour.json"] {
        let text = std::fs::read_to_string(dir().join(name)).unwrap();
        assert!(!text.contains('\u{2014}'), "em-dash in {name}");
    }
}
