use crate::dag::{PipelineDAG, StageContext, StageId};
use arcanum_core::{Result, ArcanumError};
use std::collections::HashSet;
use tracing::instrument;

pub struct DagExecutor;

impl DagExecutor {
    #[instrument(skip(dag, initial_ctx), fields(stage_count = dag.stages.len()), err)]
    pub async fn execute(dag: &PipelineDAG, initial_ctx: StageContext) -> Result<StageContext> {
        let mut completed: HashSet<StageId> = HashSet::new();
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

            // Run ready stages sequentially (parallel would require ctx cloning strategy)
            for id in &ready {
                let stage = dag.stages.iter().find(|s| s.id == *id).unwrap();
                let span = tracing::info_span!("pipeline.stage", stage_id = %id);
                let _enter = span.enter();
                let result = (stage.run)(ctx.clone()).await
                    .map_err(|e| { tracing::error!(stage_id = %id, err = ?e, "pipeline stage failed"); e })?;
                ctx.extend(result);
                completed.insert(id);
            }
            remaining.retain(|s| !completed.contains(s.id));
        }
        Ok(ctx)
    }
}
