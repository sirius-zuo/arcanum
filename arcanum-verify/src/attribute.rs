use crate::segment::Unit;
use arcanum_generate::scan_markers;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Attribution {
    pub cited: Vec<String>,
    pub invalid_refs: Vec<String>,
}

fn push_unique(v: &mut Vec<String>, id: &str) {
    if !v.iter().any(|x| x == id) {
        v.push(id.to_string());
    }
}

pub fn attribute(answer: &str, units: &[Unit], available: &[String]) -> Vec<Attribution> {
    let mut out = vec![Attribution::default(); units.len()];
    for ((start, _), ids) in scan_markers(answer) {
        let Some(i) = units
            .iter()
            .position(|u| !u.code && u.span.0 <= start && start < u.span.1)
        else {
            continue;
        };
        for id in &ids {
            if available.iter().any(|a| a == id) {
                push_unique(&mut out[i].cited, id);
            } else {
                push_unique(&mut out[i].invalid_refs, id);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::segment;

    #[test]
    fn attribution_splits_dedups_and_flags_invalid() {
        let a = "A [P1, P1]. B [P2][P7] [P2]. C [S1].\n```\n[P1]\n```";
        let at = attribute(a, &segment(a), &["P1".into(), "P2".into()]);
        assert_eq!(
            at[0],
            Attribution {
                cited: vec!["P1".into()],
                invalid_refs: vec![]
            }
        );
        assert_eq!(
            at[1],
            Attribution {
                cited: vec!["P2".into()],
                invalid_refs: vec!["P7".into()]
            }
        );
        assert_eq!(
            at[2],
            Attribution {
                cited: vec![],
                invalid_refs: vec!["S1".into()]
            }
        );
        assert_eq!(at[3], Attribution::default());
    }
}
