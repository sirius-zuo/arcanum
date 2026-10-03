use std::sync::OnceLock;

use tiktoken_rs::{cl100k_base, CoreBPE};

/// Counts tokens in text for budgeting prompt context.
pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &str) -> usize;
    fn name(&self) -> &'static str;
}

/// Safety margin applied on top of the raw cl100k count, since the target
/// model's tokenizer differs from cl100k.
const MARGIN: f64 = 1.1;

static CL100K: OnceLock<CoreBPE> = OnceLock::new();

fn cl100k() -> &'static CoreBPE {
    CL100K.get_or_init(|| cl100k_base().expect("bundled cl100k_base encoder must load"))
}

/// cl100k token count inflated by 10% to stay conservative for other tokenizers.
pub struct ApproxCl100kCounter;

impl ApproxCl100kCounter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ApproxCl100kCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for ApproxCl100kCounter {
    fn count(&self, text: &str) -> usize {
        // encode_ordinary so special-token strings in documents count as plain text.
        let raw = cl100k().encode_ordinary(text).len();
        ((raw as f64) * MARGIN).ceil() as usize
    }
    fn name(&self) -> &'static str {
        "approx_cl100k"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approx_counter_is_deterministic_and_named() {
        let c = ApproxCl100kCounter::new();
        let t = "Acme Corp builds rockets for orbital delivery.";
        assert_eq!(c.count(t), c.count(t));
        assert_eq!(c.name(), "approx_cl100k");
    }

    #[test]
    fn approx_counter_never_below_raw_cl100k_and_applies_margin() {
        let c = ApproxCl100kCounter::new();
        for t in [
            "",
            "a",
            "café naïve",
            "<passage ref=\"P1\">x</passage>",
            &"word ".repeat(500),
        ] {
            let raw = tiktoken_rs::cl100k_base().unwrap().encode_ordinary(t).len();
            assert!(c.count(t) >= raw);
            assert_eq!(c.count(t), ((raw as f64) * 1.1).ceil() as usize);
        }
    }
}
