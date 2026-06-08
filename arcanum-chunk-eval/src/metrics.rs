use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentMetrics {
    pub champion_recall_at_5:   f32,
    pub challenger_recall_at_5: f32,
    pub sample_size:            usize,
    pub computed_at:            String,
}
