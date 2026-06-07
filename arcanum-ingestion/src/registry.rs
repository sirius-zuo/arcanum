use arcanum_core::{
    traits::{DocumentRegistry, DocumentEntry, RegistryEntry, RegistryStatus},
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
            let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
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
            let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
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
            let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
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
            let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
            conn.execute(
                "DELETE FROM documents WHERE source_uri = ?1 AND collection_id = ?2",
                rusqlite::params![uri, cid],
            ).map_err(|e| ArcanumError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| ArcanumError::Storage(format!("join: {}", e)))?
    }

    async fn list_by_collection(&self, collection_id: &str) -> Result<Vec<DocumentEntry>> {
        let conn = self.conn.clone();
        let cid = collection_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
            let mut stmt = conn.prepare(
                "SELECT source_uri, registered_at \
                 FROM documents \
                 WHERE collection_id = ?1 AND status = 'clean' \
                 ORDER BY registered_at DESC"
            ).map_err(|e| ArcanumError::Storage(e.to_string()))?;
            let rows = stmt.query_map(rusqlite::params![cid], |row| {
                Ok(DocumentEntry {
                    source_uri: row.get(0)?,
                    registered_at: row.get(1)?,
                })
            }).map_err(|e| ArcanumError::Storage(e.to_string()))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| ArcanumError::Storage(e.to_string()))
        })
        .await
        .map_err(|e| ArcanumError::Storage(format!("join: {}", e)))?
    }

    async fn try_set_replacing(&self, source_uri: &str, collection_id: &str) -> Result<bool> {
        let conn = self.conn.clone();
        let uri = source_uri.to_string();
        let cid = collection_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
            // Step 1: Try to UPDATE an existing Clean row → Replacing.
            let updated = conn.execute(
                "UPDATE documents SET status = 'replacing', content_hash = NULL, registered_at = ?3
                 WHERE source_uri = ?1 AND collection_id = ?2 AND status = 'clean'",
                rusqlite::params![uri, cid, now_secs()],
            ).map_err(|e| ArcanumError::Storage(e.to_string()))?;

            if updated > 0 {
                return Ok(true); // we claimed it
            }

            // Step 2: Try to INSERT (no existing row at all).
            let inserted = conn.execute(
                "INSERT OR IGNORE INTO documents (source_uri, collection_id, content_hash, status, registered_at)
                 VALUES (?1, ?2, NULL, 'replacing', ?3)",
                rusqlite::params![uri, cid, now_secs()],
            ).map_err(|e| ArcanumError::Storage(e.to_string()))?;

            // inserted == 0 means row exists but status != 'clean' → already Replacing → we lost.
            Ok(inserted > 0)
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

    #[tokio::test]
    async fn try_set_replacing_returns_false_when_already_replacing() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        let won = reg.try_set_replacing("file://a.md", "col").await.unwrap();
        assert!(won, "first caller should win the CAS");
        let won2 = reg.try_set_replacing("file://a.md", "col").await.unwrap();
        assert!(!won2, "second caller should lose CAS when already Replacing");
    }

    #[tokio::test]
    async fn try_set_replacing_returns_true_for_new_entry() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        let won = reg.try_set_replacing("file://new.md", "col").await.unwrap();
        assert!(won, "new entry (no prior row) should always win");
    }

    #[tokio::test]
    async fn try_set_replacing_returns_true_for_clean_entry() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        reg.register("file://a.md", "col", "hash1").await.unwrap();
        let won = reg.try_set_replacing("file://a.md", "col").await.unwrap();
        assert!(won, "clean entry should be claimable");
    }

    #[test]
    fn lock_poisoning_does_not_break_registry() {
        use std::sync::{Arc, Mutex};
        let m: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
        let m2 = m.clone();
        let _ = std::panic::catch_unwind(move || {
            let _guard = m2.lock().unwrap();
            panic!("simulate panic while holding lock");
        });
        assert!(m.lock().is_err(), "mutex should be poisoned");
        let val = m.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(*val, 0, "data is still accessible after recovering from poison");
    }

    #[tokio::test]
    async fn list_by_collection_returns_clean_docs_newest_first() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        reg.register("file://a.md", "col", "hash-a").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        reg.register("file://b.md", "col", "hash-b").await.unwrap();
        reg.register("file://c.md", "other-col", "hash-c").await.unwrap();

        let docs = reg.list_by_collection("col").await.unwrap();
        assert_eq!(docs.len(), 2, "only docs from 'col'");
        // newest first — b was registered after a
        assert_eq!(docs[0].source_uri, "file://b.md");
        assert_eq!(docs[1].source_uri, "file://a.md");
        // other-col not returned
        assert!(!docs.iter().any(|d| d.source_uri == "file://c.md"));
    }

    #[tokio::test]
    async fn list_by_collection_excludes_replacing_status() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        reg.register("file://a.md", "col", "hash-a").await.unwrap();
        reg.set_replacing("file://a.md", "col").await.unwrap();

        let docs = reg.list_by_collection("col").await.unwrap();
        assert!(docs.is_empty(), "replacing docs should not appear");
    }

    #[tokio::test]
    async fn list_by_collection_empty_collection_returns_empty() {
        let reg = SqliteDocumentRegistry::open(":memory:").unwrap();
        let docs = reg.list_by_collection("no-such-col").await.unwrap();
        assert!(docs.is_empty());
    }
}
