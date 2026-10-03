use arcanum_context::{assemble, AssembleParams};
use arcanum_core::traits::ApproxCl100kCounter;
use arcanum_core::types::*;

/// Seeded xorshift64, so the test needs no extra dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform-ish in `lo..=hi`.
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next() % (hi - lo + 1) as u64) as usize
    }
}

const ALPHABET: &[char] = &['a', 'b', 'c', 'x', 'é', 'ï', '→', ' ', ' ', '.'];

fn document(rng: &mut Rng) -> String {
    let n = rng.range(200, 800);
    let mut s = String::new();
    while s.chars().count() < n {
        s.push(ALPHABET[rng.range(0, ALPHABET.len() - 1)]);
    }
    s
}

fn chunk(doc_id: &DocumentId, text: &str, start: usize, end: usize) -> RetrievedChunk {
    RetrievedChunk {
        indexed_chunk: IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),
                text: text[start..end].to_string(),
                document_id: doc_id.clone(),
                collection_id: CollectionId("c".into()),
                position: ChunkPosition {
                    start,
                    end,
                    index: 0,
                },
                metadata: ChunkMetadata::default(),
                provenance: ChunkProvenance {
                    document_version: 1,
                    source_uri: format!("raw://{}", doc_id.0),
                    ..Default::default()
                },
            },
            vector: Vector(vec![]),
            token_vectors: None,
            store_id: "t".into(),
        },
        score: 0.0,
        strategy: RetrievalStrategy::Vector,
        kind: ChunkKind::Source,
    }
}

#[test]
fn passages_are_exact_slices_and_fit_the_budget() {
    let strategies = [
        RetrievalStrategy::Vector,
        RetrievalStrategy::Bm25,
        RetrievalStrategy::Graph,
    ];
    let formats = [
        None,
        Some(RenderFormat::Numbered),
        Some(RenderFormat::Xml),
        Some(RenderFormat::Markdown),
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for case in 0..200 {
        let mut docs: Vec<(DocumentId, String)> = Vec::new();
        let mut pool: Vec<RetrievedChunk> = Vec::new();
        for _ in 0..rng.range(1, 3) {
            let id = DocumentId::new();
            let text = document(&mut rng);
            let bounds: Vec<usize> = text
                .char_indices()
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect();
            for _ in 0..rng.range(1, 30) {
                let a = rng.range(0, bounds.len() - 2);
                let b = rng.range(a + 1, (a + 120).min(bounds.len() - 1));
                pool.push(chunk(&id, &text, bounds[a], bounds[b]));
            }
            docs.push((id, text));
        }
        let mut lists: Vec<CandidateList> = Vec::new();
        for (qi, s) in strategies.iter().enumerate() {
            let mut chunks = Vec::new();
            for c in &pool {
                if rng.range(0, 2) > 0 {
                    let mut c = c.clone();
                    c.strategy = s.clone();
                    chunks.push(c);
                }
            }
            // Shuffle into a random rank order.
            for i in (1..chunks.len()).rev() {
                chunks.swap(i, rng.range(0, i));
            }
            lists.push(CandidateList {
                query_index: qi % 2,
                strategy: s.clone(),
                chunks,
            });
        }
        let mut summaries = Vec::new();
        for _ in 0..rng.range(0, 3) {
            let (id, text) = &docs[rng.range(0, docs.len() - 1)];
            let mut c = chunk(id, text, 0, text.len());
            c.strategy = RetrievalStrategy::Raptor;
            c.kind = ChunkKind::Summary {
                level: 1,
                covers: vec![],
            };
            summaries.push(c);
        }
        lists.push(CandidateList {
            query_index: 0,
            strategy: RetrievalStrategy::Raptor,
            chunks: summaries,
        });
        let params = AssembleParams {
            token_budget: rng.range(200, 1500),
            background_share: [0.0, 0.2, 0.5][rng.range(0, 2)],
            render: formats[rng.range(0, formats.len() - 1)],
        };
        let out = assemble(
            &Candidates {
                queries: vec![],
                lists,
                failed: vec![],
            },
            &params,
            &ApproxCl100kCounter,
        );
        assert!(
            out.usage.used <= params.token_budget,
            "case {case}: used {} > budget {}",
            out.usage.used,
            params.token_budget
        );
        for p in &out.passages {
            let doc = &docs.iter().find(|(id, _)| *id == p.document_id).unwrap().1;
            assert_eq!(p.text, doc[p.offset_start..p.offset_end], "case {case}");
        }
    }
}
