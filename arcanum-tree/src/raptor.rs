use std::sync::Arc;
use arcanum_core::{traits::TreeStore, types::*, Result};

pub struct RaptorBuilder<S: TreeStore + ?Sized> {
    store: Arc<S>,
    max_depth: u32,
}

impl<S: TreeStore + Send + Sync + ?Sized + 'static> RaptorBuilder<S> {
    pub fn new(store: Arc<S>, max_depth: u32) -> Self {
        Self { store, max_depth }
    }

    pub async fn build(&self, collection: &str, leaf_chunks: Vec<(String, Vector)>) -> Result<()> {
        for (text, vector) in &leaf_chunks {
            let node = TreeNode {
                id: TreeNodeId::new(), level: 0,
                text: text.clone(), vector: vector.clone(),
                parent: None, children: vec![],
                cluster_centroid: None,
            };
            self.store.insert_node(collection, node).await?;
        }

        let mut current_level_vecs: Vec<(String, Vector)> = leaf_chunks;
        for level in 1..=self.max_depth {
            if current_level_vecs.len() <= 1 { break; }
            let clusters = self.cluster(&current_level_vecs);
            let mut next_level = vec![];
            for cluster in &clusters {
                let summary = format!("{} chunks clustered at level {}", cluster.len(), level);
                let centroid = self.centroid(cluster);
                let node = TreeNode {
                    id: TreeNodeId::new(), level,
                    text: summary.clone(), vector: centroid.clone(),
                    parent: None, children: vec![],
                    cluster_centroid: Some(centroid.clone()),
                };
                self.store.insert_node(collection, node).await?;
                next_level.push((summary, centroid));
            }
            current_level_vecs = next_level;
        }
        Ok(())
    }

    fn cluster(&self, items: &[(String, Vector)]) -> Vec<Vec<(String, Vector)>> {
        items.chunks(2).map(|c| c.to_vec()).collect()
    }

    fn centroid(&self, items: &[(String, Vector)]) -> Vector {
        if items.is_empty() { return Vector(vec![]); }
        let dim = items[0].1.0.len();
        let mut sum = vec![0f32; dim];
        for (_, v) in items { for (i, x) in v.0.iter().enumerate() { sum[i] += x; } }
        let n = items.len() as f32;
        Vector(sum.iter().map(|x| x / n).collect())
    }
}
