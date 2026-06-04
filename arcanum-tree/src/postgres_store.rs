use arcanum_core::{traits::TreeStore, types::*, ArcanumError, Result};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use tracing::instrument;

/// PostgreSQL-backed TreeStore for production deployments.
pub struct PgTreeStore {
    pool: PgPool,
}

impl PgTreeStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await
            .map_err(|e| ArcanumError::Storage(format!("PgTreeStore connect error: {}", e)))?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS arcanum_tree_nodes (
                id          UUID PRIMARY KEY,
                collection  TEXT NOT NULL,
                level       INTEGER NOT NULL,
                text        TEXT NOT NULL,
                vector      JSONB NOT NULL,
                centroid    JSONB,
                parent_id   UUID,
                children    JSONB NOT NULL DEFAULT '[]'
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure_schema error: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tree_nodes_collection_level ON arcanum_tree_nodes (collection, level)")
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("ensure_schema index error: {}", e)))?;

        sqlx::query("ALTER TABLE arcanum_tree_nodes ADD COLUMN IF NOT EXISTS source_uri TEXT NOT NULL DEFAULT ''")
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("ensure_schema alter: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl TreeStore for PgTreeStore {
    #[instrument(skip(self, node), fields(store = "postgres_tree", collection, node_id = %node.id.0), err)]
    async fn insert_node(&self, collection: &str, node: TreeNode) -> Result<()> {
        let id = node.id.0;
        let parent_id = node.parent.as_ref().map(|p| p.0);
        let vector_json = serde_json::to_value(&node.vector)
            .map_err(|e| ArcanumError::Storage(format!("serialize vector: {}", e)))?;
        let centroid_json = node.cluster_centroid.as_ref()
            .map(|c| serde_json::to_value(c))
            .transpose()
            .map_err(|e| ArcanumError::Storage(format!("serialize centroid: {}", e)))?;
        let children_json = serde_json::to_value(&node.children)
            .map_err(|e| ArcanumError::Storage(format!("serialize children: {}", e)))?;

        sqlx::query(r#"
            INSERT INTO arcanum_tree_nodes (id, collection, level, text, vector, centroid, parent_id, children, source_uri)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE SET
                collection = EXCLUDED.collection,
                level      = EXCLUDED.level,
                text       = EXCLUDED.text,
                vector     = EXCLUDED.vector,
                centroid   = EXCLUDED.centroid,
                parent_id  = EXCLUDED.parent_id,
                children   = EXCLUDED.children,
                source_uri = EXCLUDED.source_uri
        "#)
        .bind(id)
        .bind(collection)
        .bind(node.level as i32)
        .bind(&node.text)
        .bind(vector_json)
        .bind(centroid_json)
        .bind(parent_id)
        .bind(children_json)
        .bind(&node.source_uri)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("insert_node error: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self), fields(store = "postgres_tree", collection, level), err)]
    async fn get_level(&self, collection: &str, level: u32) -> Result<Vec<TreeNode>> {
        let rows = sqlx::query_as::<_, PgTreeNodeRow>(
            "SELECT id, level, text, vector, centroid, parent_id, children, source_uri FROM arcanum_tree_nodes WHERE collection = $1 AND level = $2"
        )
        .bind(collection)
        .bind(level as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get_level error: {}", e)))?;

        rows.into_iter().map(row_to_node).collect()
    }

    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        sqlx::query("DELETE FROM arcanum_tree_nodes WHERE collection = $1 AND source_uri = $2")
            .bind(collection)
            .bind(source_uri)
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("delete_by_source_uri error: {}", e)))?;
        Ok(())
    }

    #[instrument(skip(self, node_id), fields(store = "postgres_tree", node_id = %node_id.0), err)]
    async fn get_children(&self, node_id: &TreeNodeId) -> Result<Vec<TreeNode>> {
        let rows = sqlx::query_as::<_, PgTreeNodeRow>(
            "SELECT id, level, text, vector, centroid, parent_id, children, source_uri FROM arcanum_tree_nodes WHERE parent_id = $1"
        )
        .bind(node_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get_children error: {}", e)))?;

        rows.into_iter().map(row_to_node).collect()
    }
}

impl PgTreeStore {
    #[instrument(skip(self), fields(store = "postgres_tree", collection), err)]
    pub async fn delete_collection(&self, collection: &str) -> Result<()> {
        sqlx::query("DELETE FROM arcanum_tree_nodes WHERE collection = $1")
            .bind(collection)
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("delete_collection error: {}", e)))?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct PgTreeNodeRow {
    id: Uuid,
    level: i32,
    text: String,
    vector: serde_json::Value,
    centroid: Option<serde_json::Value>,
    parent_id: Option<Uuid>,
    children: serde_json::Value,
    source_uri: String,
}

fn row_to_node(row: PgTreeNodeRow) -> Result<TreeNode> {
    let vector: Vector = serde_json::from_value(row.vector)
        .map_err(|e| ArcanumError::Storage(format!("deserialize vector: {}", e)))?;
    let cluster_centroid: Option<Vector> = row.centroid
        .map(|c| serde_json::from_value(c))
        .transpose()
        .map_err(|e| ArcanumError::Storage(format!("deserialize centroid: {}", e)))?;
    let children: Vec<TreeNodeId> = serde_json::from_value(row.children)
        .map_err(|e| ArcanumError::Storage(format!("deserialize children: {}", e)))?;

    Ok(TreeNode {
        id: TreeNodeId(row.id),
        level: row.level as u32,
        text: row.text,
        vector,
        parent: row.parent_id.map(TreeNodeId),
        children,
        cluster_centroid,
        source_uri: row.source_uri,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test — verifies the module compiles and types are correct.
    #[test]
    fn test_pg_tree_store_module_compiles() {
        // If this test compiles, the module is structurally correct.
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<PgTreeStore>();
    }

    /// Integration test — requires a live PostgreSQL instance.
    #[tokio::test]
    #[ignore]
    async fn test_pg_tree_store_integration() {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PgTreeStore::new(&db_url).await.expect("connect");

        let node = TreeNode {
            id: TreeNodeId::new(),
            level: 0,
            text: "test node".to_string(),
            vector: Vector(vec![0.1, 0.2, 0.3]),
            parent: None,
            children: vec![],
            cluster_centroid: None,
            source_uri: "".to_string(),
        };
        store.insert_node("test_collection", node).await.expect("insert");

        let nodes = store.get_level("test_collection", 0).await.expect("get_level");
        assert!(!nodes.is_empty());

        store.delete_collection("test_collection").await.expect("delete");
    }
}
