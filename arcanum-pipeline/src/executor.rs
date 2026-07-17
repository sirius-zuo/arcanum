use crate::dag::{PipelineDAG, StageContext, StageId, CTX_STAGE_FAILURES};
use crate::stage_failure::is_core_stage;
use arcanum_core::{Result, ArcanumError};
use std::collections::{HashMap, HashSet};
use tracing::{instrument, Instrument};
use metrics;

pub struct DagExecutor;

impl DagExecutor {
    #[instrument(skip(dag, initial_ctx), fields(stage_count = dag.stages.len()), err)]
    pub async fn execute(dag: &PipelineDAG, initial_ctx: StageContext) -> Result<StageContext> {
        let mut completed: HashSet<StageId> = HashSet::new();
        let mut unusable: HashSet<StageId> = HashSet::new();
        // Root cause per unusable stage: (root failed stage id, its error display).
        let mut root_failures: HashMap<StageId, (StageId, String)> = HashMap::new();
        let mut failures: Vec<serde_json::Value> = vec![];
        let mut ctx = initial_ctx;
        let mut remaining: Vec<_> = dag.stages.iter().collect();

        while !remaining.is_empty() {
            let ready: Vec<_> = remaining.iter()
                .filter(|s| s.deps.iter().all(|d| completed.contains(d)))
                .map(|s| s.id)
                .collect();

            if ready.is_empty() {
                return Err(ArcanumError::Pipeline {
                    stage: "executor".into(),
                    message: "circular dependency or unresolvable stages".into(),
                });
            }

            // Stages in a wave run concurrently on cloned ctxs; shared document state
            // lives behind Arc<Mutex<IngestionState>> inside each stage closure. Returned
            // ctxs merge into the parent in wave order (deterministic; later stages win
            // on key conflicts, which today are disjoint per-stage flags).
            let mut to_run: Vec<&crate::dag::PipelineStage> = vec![];
            for id in &ready {
                let stage = dag.stages.iter().find(|s| s.id == *id).unwrap();

                // Skip stages whose deps failed or were skipped.
                if let Some(bad_dep) = stage.deps.iter().find(|d| unusable.contains(*d)) {
                    let (root, root_err) = root_failures.get(bad_dep).cloned()
                        .unwrap_or((bad_dep, "unknown".to_string()));
                    // A core stage must never be silently skipped: abort so the
                    // worker's retry path fires, exactly as a direct core failure.
                    if is_core_stage(id) {
                        tracing::error!(stage_id = %id, root_stage = %root,
                            "core stage blocked by failed dependency; aborting pipeline");
                        return Err(ArcanumError::Pipeline {
                            stage: root.into(),
                            message: format!(
                                "non-core stage '{}' failed ({}); aborting because core stage '{}' depends on it",
                                root, root_err, id
                            ),
                        });
                    }
                    tracing::warn!(stage_id = %id, failed_dep = %bad_dep, "skipping stage: dependency failed");
                    failures.push(serde_json::json!({
                        "stage": id, "error": format!("skipped: dependency '{}' failed", bad_dep),
                        "skipped_due_to": bad_dep,
                    }));
                    unusable.insert(id);
                    root_failures.insert(id, (root, root_err));
                    completed.insert(id);
                    continue;
                }

                to_run.push(stage);
            }

            let wave_results = futures::future::join_all(to_run.iter().map(|stage| {
                let span = tracing::info_span!("pipeline.stage", stage_id = %stage.id);
                let fut = (stage.run)(ctx.clone());
                async move {
                    let stage_start = std::time::Instant::now();
                    let run_result = fut.instrument(span).await;
                    (stage_start.elapsed().as_secs_f64(), run_result)
                }
            })).await;

            // Merge results deterministically in wave order, applying the same
            // per-stage Ok/core-Err/non-core-Err policy as before. All wave
            // futures have already resolved by this point, so a core failure
            // orphans no in-flight work; the first core error in wave order wins.
            for (stage, (elapsed, run_result)) in to_run.iter().zip(wave_results) {
                let id = stage.id;
                let status = if run_result.is_ok() { "ok" } else { "error" };
                metrics::counter!("arcanum_pipeline_stages_total",
                    "stage_id" => id.to_string(), "status" => status).increment(1);
                metrics::histogram!("arcanum_pipeline_stage_duration_seconds",
                    "stage_id" => id.to_string()).record(elapsed);

                match run_result {
                    Ok(result) => {
                        ctx.extend(result);
                        completed.insert(id);
                    }
                    Err(e) if is_core_stage(id) => {
                        tracing::error!(stage_id = %id, err = ?e, "core pipeline stage failed");
                        return Err(e);
                    }
                    Err(e) => {
                        tracing::warn!(stage_id = %id, err = ?e, "non-core pipeline stage failed; continuing");
                        failures.push(serde_json::json!({
                            "stage": id, "error": e.to_string(), "skipped_due_to": serde_json::Value::Null,
                        }));
                        unusable.insert(id);
                        root_failures.insert(id, (id, e.to_string()));
                        completed.insert(id); // unblocks the wave loop; dependents are caught by the skip check
                    }
                }
            }
            remaining.retain(|s| !completed.contains(s.id));
        }

        if !failures.is_empty() {
            ctx.insert(CTX_STAGE_FAILURES.to_string(), serde_json::Value::Array(failures));
        }
        Ok(ctx)
    }
}
