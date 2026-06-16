use std::sync::Arc;
use arcanum_core::{traits::TreeStore, types::*, Result};
use linfa::DatasetBase;
use linfa::traits::{Fit, Predict};
use linfa_clustering::KMeans;
use ndarray::Array2;
use tracing::instrument;

pub struct RaptorBuilder<S: TreeStore + ?Sized> {
    store: Arc<S>,
    max_depth: u32,
}

impl<S: TreeStore + Send + Sync + ?Sized + 'static> RaptorBuilder<S> {
    pub fn new(store: Arc<S>, max_depth: u32) -> Self {
        Self { store, max_depth }
    }

    #[instrument(skip(self, leaf_chunks), fields(collection, input_chunk_count = leaf_chunks.len(), max_depth = self.max_depth), err)]
    pub async fn build(&self, collection: &str, source_uri: &str, leaf_chunks: Vec<(ChunkId, String, Vector)>) -> Result<()> {
        // Store level-0 leaf nodes with their chunk IDs.
        for (chunk_id, text, vector) in &leaf_chunks {
            let node = TreeNode {
                id: TreeNodeId::new(), level: 0,
                text: text.clone(), vector: vector.clone(),
                parent: None, children: vec![],
                cluster_centroid: None,
                source_uri: source_uri.to_string(),
                leaf_chunk_ids: vec![chunk_id.clone()],
            };
            self.store.insert_node(collection, node).await?;
        }

        // Track (text, vector, leaf_chunk_ids) for each level.
        let mut current_level: Vec<(String, Vector, Vec<ChunkId>)> = leaf_chunks
            .into_iter()
            .map(|(id, text, vec)| (text, vec, vec![id]))
            .collect();

        for level in 1..=self.max_depth {
            if current_level.len() <= 1 { break; }
            let vectors: Vec<Vector> = current_level.iter().map(|(_, v, _)| v.clone()).collect();
            let k = ((current_level.len() as f64).sqrt().ceil() as usize).max(2);
            let clusters_indices = kmeans_cluster(&vectors, k);
            let mut next_level = vec![];
            for group_indices in &clusters_indices {
                let group: Vec<_> = group_indices.iter().map(|&i| &current_level[i]).collect();
                let summary = format!("{} chunks clustered at level {}", group.len(), level);
                let centroid = self.centroid(&group.iter().map(|(t, v, _)| (t.clone(), v.clone())).collect::<Vec<_>>());
                let leaf_chunk_ids: Vec<ChunkId> = group.iter()
                    .flat_map(|(_, _, ids)| ids.iter().cloned())
                    .collect();
                let node = TreeNode {
                    id: TreeNodeId::new(), level,
                    text: summary.clone(), vector: centroid.clone(),
                    parent: None, children: vec![],
                    cluster_centroid: Some(centroid.clone()),
                    source_uri: source_uri.to_string(),
                    leaf_chunk_ids: leaf_chunk_ids.clone(),
                };
                self.store.insert_node(collection, node).await?;
                next_level.push((summary, centroid, leaf_chunk_ids));
            }
            current_level = next_level;
        }
        Ok(())
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

/// K-means clustering of vectors. Returns groups of indices.
pub fn kmeans_cluster(vectors: &[Vector], k: usize) -> Vec<Vec<usize>> {
    if vectors.is_empty() { return vec![]; }
    let k_actual = k.min(vectors.len());
    if k_actual <= 1 {
        return vec![(0..vectors.len()).collect()];
    }

    let dim = vectors[0].0.len();
    if dim == 0 {
        return vec![(0..vectors.len()).collect()];
    }

    let data: Vec<f64> = vectors.iter()
        .flat_map(|v| v.0.iter().map(|&f| f as f64))
        .collect();

    let array = match Array2::from_shape_vec((vectors.len(), dim), data) {
        Ok(a) => a,
        Err(_) => return vec![(0..vectors.len()).collect()],
    };
    let dataset = DatasetBase::from(array.clone());

    let model = match KMeans::<f64, _>::params(k_actual)
        .max_n_iterations(100)
        .tolerance(1e-4)
        .fit(&dataset) {
        Ok(m) => m,
        Err(_) => return vec![(0..vectors.len()).collect()],
    };

    let predicted = model.predict(DatasetBase::from(array));
    let assignments = predicted.targets();

    let mut clusters: Vec<Vec<usize>> = vec![vec![]; k_actual];
    for (idx, &label) in assignments.iter().enumerate() {
        if label < k_actual {
            clusters[label].push(idx);
        }
    }
    clusters.retain(|c| !c.is_empty());
    clusters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kmeans_cluster_handles_fewer_items_than_k() {
        let vectors = vec![Vector(vec![1.0, 0.0])];
        let clusters = kmeans_cluster(&vectors, 5);
        assert_eq!(clusters.len(), 1);
    }

    #[test]
    fn test_kmeans_cluster_empty() {
        let clusters = kmeans_cluster(&[], 3);
        assert!(clusters.is_empty());
    }

    #[tokio::test]
    async fn test_kmeans_clustering_groups_vectors() {
        let vectors = vec![
            Vector(vec![1.0, 0.1, 0.0]),
            Vector(vec![0.9, 0.0, 0.1]),
            Vector(vec![0.0, 1.0, 0.1]),
            Vector(vec![0.1, 0.9, 0.0]),
        ];
        let clusters = kmeans_cluster(&vectors, 2);
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].len() + clusters[1].len(), 4);
    }
}
