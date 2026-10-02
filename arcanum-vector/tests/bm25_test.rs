use arcanum_core::types::ChunkId;
use arcanum_vector::Bm25Index;
use std::sync::Arc;

fn new_index() -> (tempfile::TempDir, Bm25Index) {
    let dir = tempfile::tempdir().unwrap();
    let idx = Bm25Index::new(dir.path().to_str().unwrap()).unwrap();
    (dir, idx)
}

#[test]
fn search_returns_chunk_ids_ranked() {
    let (_dir, idx) = new_index();
    let a = ChunkId::new();
    let b = ChunkId::new();
    idx.index_chunks(
        "col",
        "file://x.md",
        &[
            (a.clone(), "the quick brown fox".to_string()),
            (b.clone(), "jumps over the lazy dog".to_string()),
        ],
    )
    .unwrap();

    let results = idx.search("col", "quick fox", 5).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, a);
    assert!(results[0].1 > 0.0);
}

#[test]
fn search_is_scoped_to_collection() {
    let (_dir, idx) = new_index();
    let in_a = ChunkId::new();
    let in_b = ChunkId::new();
    idx.index_chunks("a", "u", &[(in_a.clone(), "shared text here".into())])
        .unwrap();
    idx.index_chunks("b", "u", &[(in_b.clone(), "shared text here".into())])
        .unwrap();

    let results = idx.search("a", "shared", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, in_a);
}

#[test]
fn delete_by_source_uri_removes_only_that_source() {
    let (_dir, idx) = new_index();
    let keep = ChunkId::new();
    let gone = ChunkId::new();
    let other_col = ChunkId::new();
    idx.index_chunks("c", "file://keep", &[(keep.clone(), "needle one".into())])
        .unwrap();
    idx.index_chunks("c", "file://gone", &[(gone.clone(), "needle two".into())])
        .unwrap();
    idx.index_chunks(
        "d",
        "file://gone",
        &[(other_col.clone(), "needle three".into())],
    )
    .unwrap();

    idx.delete_by_source_uri("c", "file://gone").unwrap();

    let c: Vec<_> = idx
        .search("c", "needle", 10)
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(c, vec![keep]);
    let d: Vec<_> = idx
        .search("d", "needle", 10)
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(d, vec![other_col]);
}

#[test]
fn concurrent_index_chunks_all_succeed() {
    let (_dir, idx) = new_index();
    let idx = Arc::new(idx);
    let ids: Vec<ChunkId> = (0..4).map(|_| ChunkId::new()).collect();
    let handles: Vec<_> = ids
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, id)| {
            let idx = idx.clone();
            std::thread::spawn(move || {
                idx.index_chunks("col", &format!("u{i}"), &[(id, "common token".into())])
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap().unwrap();
    }
    let found: std::collections::HashSet<_> = idx
        .search("col", "common", 10)
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(found.len(), 4);
    for id in ids {
        assert!(found.contains(&id));
    }
}

/// A failed commit must not poison the shared writer: after the failure clears,
/// later writes have to be searchable. Induced by making the index dir read-only,
/// so it is skipped when running as a user that ignores permissions (root).
#[cfg(unix)]
#[test]
fn writes_succeed_after_a_failed_commit() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, idx) = new_index();
    let before = ChunkId::new();
    idx.index_chunks("col", "u", &[(before.clone(), "alpha text".into())])
        .unwrap();

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let failed = idx.index_chunks("col", "u2", &[(ChunkId::new(), "lost text".into())]);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    if failed.is_ok() {
        return; // permissions not enforced (root); failure could not be induced
    }

    let after = ChunkId::new();
    idx.index_chunks("col", "u3", &[(after.clone(), "bravo text".into())])
        .unwrap();
    let hits = idx.search("col", "bravo", 5).unwrap();
    assert_eq!(
        hits.len(),
        1,
        "write after a failed commit must be searchable"
    );
    assert_eq!(hits[0].0, after);
    assert!(idx.search("col", "lost", 5).unwrap().is_empty());
    assert_eq!(idx.search("col", "alpha", 5).unwrap()[0].0, before);
}
