use arcanum_core::ArcanumError;
use arcanum_pipeline::{DagExecutor, PipelineDAG, PipelineStage, StageContext, CTX_STAGE_FAILURES};
use std::sync::Arc;

fn ok_stage(id: &'static str, deps: Vec<&'static str>) -> PipelineStage {
    PipelineStage { id, deps, run: Arc::new(move |ctx| Box::pin(async move {
        let mut ctx = ctx;
        ctx.insert(format!("ran:{}", id), serde_json::json!(true));
        Ok(ctx)
    })) }
}

fn failing_stage(id: &'static str, deps: Vec<&'static str>) -> PipelineStage {
    PipelineStage { id, deps, run: Arc::new(move |_ctx| Box::pin(async move {
        Err(ArcanumError::Pipeline { stage: id.into(), message: "boom".into() })
    })) }
}

#[tokio::test]
async fn non_core_failure_records_and_skips_dependents() {
    // "load" is core and succeeds; "enrich" is non-core and fails;
    // "graph_write" depends on enrich and must be skipped, not run.
    let dag = PipelineDAG::new()
        .add_stage(ok_stage("load", vec![]))
        .add_stage(failing_stage("enrich", vec!["load"]))
        .add_stage(ok_stage("graph_write", vec!["enrich"]));
    let out = DagExecutor::execute(&dag, StageContext::default()).await
        .expect("non-core failure must not abort the pipeline");
    assert!(out.get("ran:load").is_some());
    assert!(out.get("ran:graph_write").is_none(), "dependent of failed stage must be skipped");
    let failures = out.get(CTX_STAGE_FAILURES).and_then(|v| v.as_array()).expect("failure record");
    assert_eq!(failures.len(), 2, "exactly one failure and one skip expected");
    let stages: Vec<&str> = failures.iter().map(|f| f["stage"].as_str().unwrap()).collect();
    assert!(stages.contains(&"enrich"));
    assert!(stages.contains(&"graph_write"));
    let enrich = failures.iter().find(|f| f["stage"] == "enrich").unwrap();
    assert_eq!(enrich["skipped_due_to"], serde_json::Value::Null);
    let gw = failures.iter().find(|f| f["stage"] == "graph_write").unwrap();
    assert_eq!(gw["skipped_due_to"], "enrich");
}

#[tokio::test]
async fn non_core_failure_upstream_of_core_aborts() {
    // "snapshotish" is non-core and fails, but core "vector_write" depends on it:
    // the pipeline must abort (worker retry path fires), not degrade.
    let dag = PipelineDAG::new()
        .add_stage(ok_stage("load", vec![]))
        .add_stage(failing_stage("snapshotish", vec!["load"]))
        .add_stage(ok_stage("vector_write", vec!["snapshotish"]));
    let err = DagExecutor::execute(&dag, StageContext::default()).await
        .expect_err("core stage transitively blocked by a non-core failure must abort");
    let msg = err.to_string();
    assert!(msg.contains("snapshotish"), "error must name the root failed stage: {msg}");
    assert!(msg.contains("vector_write"), "error must name the blocked core stage: {msg}");
}

#[tokio::test]
async fn core_failure_still_aborts() {
    let dag = PipelineDAG::new()
        .add_stage(failing_stage("load", vec![]))
        .add_stage(ok_stage("enrich", vec!["load"]));
    assert!(DagExecutor::execute(&dag, StageContext::default()).await.is_err(),
        "core stage failure must propagate exactly as before");
}

#[tokio::test]
async fn independent_stages_in_a_wave_run_concurrently() {
    // Both stages wait on the same 2-party barrier. If the executor runs them
    // sequentially, the first blocks forever and the timeout trips.
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let make = |id: &'static str, b: Arc<tokio::sync::Barrier>| PipelineStage {
        id, deps: vec![], run: Arc::new(move |ctx| {
            let b = b.clone();
            Box::pin(async move { b.wait().await; Ok(ctx) })
        }),
    };
    let dag = PipelineDAG::new()
        .add_stage(make("alpha", barrier.clone()))
        .add_stage(make("beta", barrier.clone()));
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        DagExecutor::execute(&dag, StageContext::default()),
    ).await.expect("stages must run concurrently — sequential execution deadlocks this test")
     .expect("execute should succeed");
}

#[tokio::test]
async fn multi_hop_cascade_names_root_failure() {
    // "a" is non-core and fails; "b" depends on "a" (non-core, skipped);
    // core "vector_write" depends on "b" (skipped-of-skipped). The abort
    // error must name the true root ("a"), not just the immediate dep ("b").
    let dag = PipelineDAG::new()
        .add_stage(failing_stage("a", vec![]))
        .add_stage(ok_stage("b", vec!["a"]))
        .add_stage(ok_stage("vector_write", vec!["b"]));
    let err = DagExecutor::execute(&dag, StageContext::default()).await
        .expect_err("core stage transitively blocked through a multi-hop cascade must abort");
    let msg = err.to_string();
    assert!(msg.contains("'a'"), "error must name the root failed stage 'a', not just 'b': {msg}");
}

#[tokio::test]
async fn core_failure_with_ok_wave_mate_still_aborts() {
    // Core "load" fails while dep-free "enrich" succeeds in the SAME wave.
    // The join must complete both, metrics record for both (verified by code
    // structure: the metrics pass runs over every joined result before the
    // policy pass — no test recorder is wired for the metrics crate here),
    // and the core error must still abort the pipeline.
    let dag = PipelineDAG::new()
        .add_stage(failing_stage("load", vec![]))
        .add_stage(ok_stage("enrich", vec![]));
    let err = DagExecutor::execute(&dag, StageContext::default()).await
        .expect_err("core failure must abort even when a wave-mate succeeds");
    assert!(err.to_string().contains("load"), "error must come from the core stage: {err}");
}
