mod metrics;
mod runner;
pub use metrics::{compute_hit_rate_at_k, compute_mrr, compute_ndcg_at_k};
pub use runner::{EvalRunner, EvalReport, GoldenSample};
