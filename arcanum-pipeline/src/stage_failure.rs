use arcanum_core::ArcanumError;

pub enum StageFailure {
    Core    { stage: String, error: ArcanumError },
    NonCore { stage: String, error: ArcanumError },
}

pub fn is_core_stage(stage_id: &str) -> bool {
    matches!(stage_id, "load" | "preprocess" | "vector_chunk" | "graph_chunk" | "tree_chunk" | "embed" | "vector_write")
}
