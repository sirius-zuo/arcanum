//! Sentence and overall verdicts (spec 6.2).

use crate::judge::{ClaimKind, JudgedClaim};
use arcanum_core::types::{OverallVerdict, SentenceVerdict, VerdictCounts};

pub fn sentence_verdict(
    kind: ClaimKind,
    claims: &[JudgedClaim],
    cited: &[String],
) -> SentenceVerdict {
    if kind == ClaimKind::NoClaim {
        return SentenceVerdict::NoClaim;
    }
    let supported = claims.iter().filter(|c| !c.support.is_empty()).count();
    if supported == 0 {
        return SentenceVerdict::Unsupported;
    }
    if supported < claims.len() {
        return SentenceVerdict::Partial;
    }
    if cited.is_empty() {
        return SentenceVerdict::UncitedSupported;
    }
    let all_cited = claims
        .iter()
        .all(|c| c.support.iter().any(|s| cited.contains(&s.ref_id)));
    if all_cited {
        SentenceVerdict::Supported
    } else {
        SentenceVerdict::Miscited
    }
}

pub fn overall(counts: &VerdictCounts, strict: bool) -> OverallVerdict {
    let bad = counts.partial > 0
        || counts.unsupported > 0
        || (strict && (counts.miscited > 0 || counts.uncited_supported > 0));
    if bad {
        OverallVerdict::Fail
    } else {
        OverallVerdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::Support;

    fn claim(refs: &[&str]) -> JudgedClaim {
        JudgedClaim {
            text: "c".into(),
            support: refs
                .iter()
                .map(|r| Support {
                    ref_id: r.to_string(),
                    quote: String::new(),
                })
                .collect(),
        }
    }

    fn cited(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn verdict_table() {
        use SentenceVerdict::*;
        let k = ClaimKind::Claim;
        assert_eq!(sentence_verdict(ClaimKind::NoClaim, &[], &[]), NoClaim);
        assert_eq!(
            sentence_verdict(k, &[claim(&[]), claim(&[])], &cited(&["P1"])),
            Unsupported
        );
        assert_eq!(
            sentence_verdict(k, &[claim(&["P1"]), claim(&[])], &cited(&["P1"])),
            Partial
        );
        assert_eq!(
            sentence_verdict(k, &[claim(&["P1"])], &cited(&["P1"])),
            Supported
        );
        assert_eq!(
            sentence_verdict(k, &[claim(&["P1"])], &[]),
            UncitedSupported
        );
        assert_eq!(
            sentence_verdict(k, &[claim(&["P1"]), claim(&["P2"])], &cited(&["P1"])),
            Miscited
        );
        // cited only P9: attribution drops it, so `cited` is empty
        assert_eq!(
            sentence_verdict(k, &[claim(&["P1"])], &[]),
            UncitedSupported
        );
    }

    #[test]
    fn overall_default_and_strict() {
        let mut c = VerdictCounts::default();
        c.add(SentenceVerdict::NoClaim);
        assert_eq!(overall(&c, false), OverallVerdict::Pass);
        assert_eq!(overall(&c, true), OverallVerdict::Pass);
        c.add(SentenceVerdict::Miscited);
        assert_eq!(overall(&c, false), OverallVerdict::Pass);
        assert_eq!(overall(&c, true), OverallVerdict::Fail);
        let mut p = VerdictCounts::default();
        p.add(SentenceVerdict::Partial);
        assert_eq!(overall(&p, false), OverallVerdict::Fail);
        assert_eq!(overall(&p, true), OverallVerdict::Fail);
    }
}
