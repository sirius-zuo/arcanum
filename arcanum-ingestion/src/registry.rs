use arcanum_core::{
    traits::{DocumentRegistry, RegistryEntry, RegistryStatus},
    ArcanumError, Result,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SqliteDocumentRegistry {
    conn: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS documents (
    source_uri    TEXT    NOT NULL,
    collection_id TEXT    NOT NULL,
    content_hash  TEXT,
    status        TEXT    NOT NULL DEFAULT 'clean',
    registered_at INTEGER NOT NULL,
    PRIMARY KEY (source_uri, collection_id)
);";

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl SqliteDocumentRegistry {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| ArcanumError::Storage(format!("sqlite open: {}", e)))?;
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| ArcanumError::Storage(format!("sqlite schema: {}", e)))?;
        Ok(Self { conn: Arc::new(std::sync::Mutex::new(conn)) })
    }
}

#[async_trait]
impl DocumentRegistry for SqliteDocumentRegistry {
    async fn get_entry(&self, source_uri: &str, collection_id: &str) -> Result<Option<RegistryEntry>> {
        let conn = self.conn.clone();
        let uri = source_uri.to_string();
        let cid = collection_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| ArcanumError::Storage(format!("lock: {}", e)))?;
            let mut stmt = conn.prepare(
                "SELECT content_hash, status FROM documents WHERE source_uri = ?1 AND collection_id = ?2"
            ).map_err(|e| ArcanumError::Storage(e.to_string()))?;

            match stmt.query_row(rusqlite::params![uri, cid], |row| {
                let hash: Option<String> = row.get(0)?;
                let status: String = row.get(1)?;
                Ok((hash, status))
            }) {
                Ok((hash, s)) => Ok(Some(RegistryEntry {
                    content_hash: hash,
                    status: if s == "replacing" {
                        RegistryStatus::Replacing
                    } else {
                        RegistryStatus::Clean
                    },
                })),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(ArcanumError::Storage(e.to_string())),
            }
        })
        .await
        .map_err(|e| ArcanumError::Storage(format!("join: {}", e)))?
    }

    async fn set_replacing(&self, source_uri: &str, collection_id: &str) -> Result<()> {
        let conn = self.conn.clone();
        let uri = source_uri.to_string();
        let cid = collection_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| ArcanumError::Storage(format!("lock: {}", e)))?;
            conn.execute(
                "INSERT INTO documents (source_uri, collection_id, content_hash, status, registered_at)
                 VALUES (?1, ?2, NULL, 'replacing', ?3)
                 ON CONFLICT (source_uri, collection_id)
                 DO UPDATE SET status = 'replacing', content_hash = NULL",
                rusqlite::params![uri, cid, now_secs()],
            ).map_err(|e| ArcanumError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| ArcanumError::Storage(format!("join: {}", e)))?
    }

    async fn register(&self, source_uri: &str, collection_id: &str, content_hash: &str) -> Result<()> {
        let conn = self.conn.clone();
        let uri = source_uri.to_string();
        let cid = collection_id.to_string();
        let hash = content_hash.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| ArcanumError::Storage(format!("lock: {}", e)))?;
            conn.execute(
                "INSERT INTO documents (source_uri, collection_id, content_hash, status, registered_at)
                 VALUES (?1, ?2, ?3, 'clean', ?4)
                 ON CONFLICT (source_uri, collection_id)
                 DO UPDATE SET content_hash = ?3, status = 'clean', registered_at = ?4",
                rusqlite::params![uri, cid, hash, now_secs()],
            ).map_err(|e| ArcanumError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| ArcanumError::Storage(format!("join: {}", e)))?
    }

    async fn deregister(&self, source_uri: &str, collection_id: &str) -> Result<()> {
        let conn = self.conn.clone();
        let uri = source_uri.to_string();
        let cid = collection_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| ArcanumError::Storage(format!("lock: {}", e)))?;
            conn.execute(
                "DELETE FROM documents WHERE source_uri = ?1 AND collection_id = ?2",
                rusqlite::params![uri, cid],
            ).map_err(|e| ArcanumError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| ArcanumError::Storage(format!("join: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_retrieve_entry() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        assert!(reg.get_entry("file://a.md", "col").await.unwrap().is_none());

        reg.register("file://a.md", "col", "abc123").await.unwrap();
        let entry = reg.get_entry("file://a.md", "col").await.unwrap().unwrap();
        assert_eq!(entry.content_hash.as_deref(), Some("abc123"));
        assert_eq!(entry.status, RegistryStatus::Clean);
    }

    #[tokio::test]
    async fn set_replacing_marks_status() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        reg.register("file://a.md", "col", "hash1").await.unwrap();
        reg.set_replacing("file://a.md", "col").await.unwrap();
        let entry = reg.get_entry("file://a.md", "col").await.unwrap().unwrap();
        assert_eq!(entry.status, RegistryStatus::Replacing);
        assert!(entry.content_hash.is_none());
    }

    #[tokio::test]
    async fn deregister_removes_entry() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        reg.register("file://a.md", "col", "hash1").await.unwrap();
        reg.deregister("file://a.md", "col").await.unwrap();
        assert!(reg.get_entry("file://a.md", "col").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn deregister_nonexistent_is_ok() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        reg.deregister("does_not_exist", "col").await.unwrap();
    }

    #[tokio::test]
    async fn set_replacing_on_new_entry_is_ok() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        reg.set_replacing("file://new.md", "col").await.unwrap();
        let entry = reg.get_entry("file://new.md", "col").await.unwrap().unwrap();
        assert_eq!(entry.status, RegistryStatus::Replacing);
    }
}
