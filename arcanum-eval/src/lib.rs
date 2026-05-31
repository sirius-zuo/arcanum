mod metrics;
mod runner;
pub mod dataset;
pub use metrics::{compute_hit_rate_at_k, compute_mrr, compute_ndcg_at_k};
pub use runner::{EvalRunner, EvalReport, GoldenSample};
pub use dataset::{BenchmarkDataset, BenchmarkSample};
