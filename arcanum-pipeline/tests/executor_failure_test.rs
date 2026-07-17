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
    let stages: Vec<&str> = failures.iter().map(|f| f["stage"].as_str().unwrap()).collect();
    assert!(stages.contains(&"enrich"));
    assert!(stages.contains(&"graph_write"));
    let gw = failures.iter().find(|f| f["stage"] == "graph_write").unwrap();
    assert_eq!(gw["skipped_due_to"], "enrich");
}

#[tokio::test]
async fn core_failure_still_aborts() {
    let dag = PipelineDAG::new()
        .add_stage(failing_stage("load", vec![]))
        .add_stage(ok_stage("enrich", vec!["load"]));
    assert!(DagExecutor::execute(&dag, StageContext::default()).await.is_err(),
        "core stage failure must propagate exactly as before");
}
