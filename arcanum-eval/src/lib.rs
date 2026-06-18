mod metrics;
mod runner;
pub mod dataset;
pub mod scheduler;
pub mod evaluator;
pub use metrics::{
    compute_answer_relevance, compute_context_precision, compute_context_recall,
    compute_faithfulness, compute_hit_rate_at_k, compute_mrr, compute_ndcg_at_k,
};
pub use runner::{EvalRunner, EvalReport, GoldenSample};
pub use dataset::{BenchmarkDataset, BenchmarkSample};
pub use scheduler::EvalScheduler;
pub use evaluator::StandardEvaluator;
