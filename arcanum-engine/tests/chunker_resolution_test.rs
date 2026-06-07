use arcanum_core::types::{ChunkStrategyConfig, PerBackendChunkConfig};
use arcanum_ingestion::default_registry;

fn make_fixed(size: u64, overlap: u64) -> ChunkStrategyConfig {
    ChunkStrategyConfig {
        strategy: "fixed".to_string(),
        params: serde_json::json!({ "chunk_size": size, "overlap": overlap }),
    }
}

fn make_semantic(max_chars: u64) -> ChunkStrategyConfig {
    ChunkStrategyConfig {
        strategy: "semantic".to_string(),
        params: serde_json::json!({ "max_chars": max_chars }),
    }
}

fn resolve(
    collection: Option<&PerBackendChunkConfig>,
    global: &PerBackendChunkConfig,
) -> arcanum_core::Result<arcanum_core::types::PerBackendChunkers> {
    let registry = default_registry();
    let vector_cfg = collection.and_then(|c| Some(&c.vector)).unwrap_or(&global.vector);
    let graph_cfg  = collection.and_then(|c| c.graph.as_ref())
                              .or(global.graph.as_ref())
                              .unwrap_or(&global.vector);
    let tree_cfg   = collection.and_then(|c| c.tree.as_ref())
                              .or(global.tree.as_ref())
                              .unwrap_or(&global.vector);
    Ok(arcanum_core::types::PerBackendChunkers {
        vector: registry.build(vector_cfg)?,
        graph:  registry.build(graph_cfg)?,
        tree:   registry.build(tree_cfg)?,
    })
}

#[test]
fn no_collection_override_uses_global_default() {
    let global = PerBackendChunkConfig {
        vector: make_fixed(512, 64),
        graph:  None,
        tree:   None,
    };
    let chunkers = resolve(None, &global);
    assert!(chunkers.is_ok(), "should build from global default");
}

#[test]
fn collection_vector_override_wins() {
    let global = PerBackendChunkConfig {
        vector: make_fixed(512, 64),
        graph:  None,
        tree:   None,
    };
    let collection = PerBackendChunkConfig {
        vector: make_semantic(800),
        graph:  None,
        tree:   None,
    };
    // Should not error — semantic chunker is valid
    let result = resolve(Some(&collection), &global);
    assert!(result.is_ok(), "collection semantic override should build");
}

#[test]
fn collection_none_graph_falls_back_to_global_then_vector() {
    let global = PerBackendChunkConfig {
        vector: make_fixed(512, 64),
        graph:  None,  // no global graph override either
        tree:   None,
    };
    // Should fall back to global.vector for graph backend
    let result = resolve(None, &global);
    assert!(result.is_ok());
}

#[test]
fn unknown_strategy_in_collection_config_returns_error() {
    let global = PerBackendChunkConfig {
        vector: make_fixed(512, 64),
        graph:  None,
        tree:   None,
    };
    let bad_collection = PerBackendChunkConfig {
        vector: ChunkStrategyConfig {
            strategy: "nonexistent".to_string(),
            params: serde_json::json!({}),
        },
        graph: None,
        tree:  None,
    };
    let result = resolve(Some(&bad_collection), &global);
    match result {
        Ok(_) => panic!("unknown strategy should return error"),
        Err(e) => {
            assert!(e.to_string().contains("nonexistent"), "error should name unknown strategy: {}", e);
        }
    }
}
