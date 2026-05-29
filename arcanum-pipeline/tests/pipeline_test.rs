use arcanum_core::types::*;
use arcanum_pipeline::{DagExecutor, PipelineDAG, PipelineStage};
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::test]
async fn test_dag_executes_in_topological_order() {
    let order = Arc::new(tokio::sync::Mutex::new(vec![]));
    let order1 = order.clone();
    let order2 = order.clone();
    let order3 = order.clone();

    let dag = PipelineDAG::new()
        .add_stage(PipelineStage {
            id: "load",
            deps: vec![],
            run: Arc::new(move |ctx| {
                let o = order1.clone();
                Box::pin(async move {
                    o.lock().await.push("load");
                    Ok(ctx)
                })
            }),
        })
        .add_stage(PipelineStage {
            id: "chunk",
            deps: vec!["load"],
            run: Arc::new(move |ctx| {
                let o = order2.clone();
                Box::pin(async move {
                    o.lock().await.push("chunk");
                    Ok(ctx)
                })
            }),
        })
        .add_stage(PipelineStage {
            id: "embed",
            deps: vec!["chunk"],
            run: Arc::new(move |ctx| {
                let o = order3.clone();
                Box::pin(async move {
                    o.lock().await.push("embed");
                    Ok(ctx)
                })
            }),
        });

    DagExecutor::execute(&dag, HashMap::new()).await.unwrap();
    let result = order.lock().await;
    assert_eq!(*result, vec!["load", "chunk", "embed"]);
}

#[tokio::test]
async fn test_pipeline_returns_operation_id() {
    let op_id = OperationId::new();
    assert!(!op_id.0.is_nil());
}
