//! Greedy batching of sentences under a token and count budget.

use crate::judge::{user_message, JudgeSentence};
use arcanum_core::traits::TokenCounter;
use arcanum_core::types::Passage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassagesOverBudget;

pub fn plan_batches(
    passages: &[Passage],
    sentences: Vec<JudgeSentence>,
    counter: &dyn TokenCounter,
    max_input_tokens: usize,
    max_sentences: usize,
) -> Result<Vec<Vec<JudgeSentence>>, PassagesOverBudget> {
    if counter.count(&user_message(passages, &[])) > max_input_tokens {
        return Err(PassagesOverBudget);
    }
    let mut batches: Vec<Vec<JudgeSentence>> = Vec::new();
    let mut current: Vec<JudgeSentence> = Vec::new();
    for s in sentences {
        if current.len() >= max_sentences {
            batches.push(std::mem::take(&mut current));
        }
        current.push(s);
        if current.len() > 1 && counter.count(&user_message(passages, &current)) > max_input_tokens
        {
            let last = current.pop().expect("len > 1");
            batches.push(std::mem::replace(&mut current, vec![last]));
        }
    }
    if !current.is_empty() {
        batches.push(current);
    }
    Ok(batches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;

    struct CharCounter;
    impl TokenCounter for CharCounter {
        fn count(&self, text: &str) -> usize {
            text.chars().count()
        }
        fn name(&self) -> &'static str {
            "chars"
        }
    }

    fn passages() -> Vec<Passage> {
        vec![Passage {
            ref_id: "P1".into(),
            document_id: DocumentId::new(),
            version_num: 1,
            source_uri: "raw://a".into(),
            snapshot_uri: String::new(),
            canonical_uri: None,
            section: None,
            page: None,
            offset_start: 0,
            offset_end: 0,
            text: "text".into(),
            chunk_ids: vec![],
            strategies: vec![],
            score: 0.0,
        }]
    }

    fn sents(n: usize, text: &str) -> Vec<JudgeSentence> {
        (1..=n)
            .map(|id| JudgeSentence {
                id,
                text: text.into(),
                cited: vec![],
            })
            .collect()
    }

    fn ids(b: &[Vec<JudgeSentence>]) -> Vec<Vec<usize>> {
        b.iter().map(|x| x.iter().map(|s| s.id).collect()).collect()
    }

    #[test]
    fn batches_split_by_sentence_count() {
        let b = plan_batches(&passages(), sents(5, "x"), &CharCounter, 100_000, 2).unwrap();
        assert_eq!(ids(&b), vec![vec![1, 2], vec![3, 4], vec![5]]);
    }

    #[test]
    fn batches_split_by_tokens() {
        let p = passages();
        let s = sents(4, "same text");
        let max = CharCounter.count(&user_message(&p, &s[..2]));
        let b = plan_batches(&p, s, &CharCounter, max, 10).unwrap();
        assert_eq!(ids(&b), vec![vec![1, 2], vec![3, 4]]);
    }

    #[test]
    fn passages_alone_over_budget() {
        let p = passages();
        let max = CharCounter.count(&user_message(&p, &[])) - 1;
        assert_eq!(
            plan_batches(&p, sents(1, "x"), &CharCounter, max, 10),
            Err(PassagesOverBudget)
        );
    }

    #[test]
    fn oversized_sentence_goes_alone() {
        let p = passages();
        let mut s = sents(3, "small");
        s[1].text = "huge ".repeat(200);
        let max = CharCounter.count(&user_message(&p, &s[..1])) + 20;
        let b = plan_batches(&p, s, &CharCounter, max, 10).unwrap();
        assert_eq!(ids(&b), vec![vec![1], vec![2], vec![3]]);
    }
}
