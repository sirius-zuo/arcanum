use serde::{Deserialize, Serialize};
use chrono::Utc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub operation: String,
    pub user_id: String,
    pub collection_id: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub entry: AuditEntry,
    pub timestamp: String,
}

#[derive(Debug)]
pub struct AuditLogger {
    records: RwLock<Vec<AuditRecord>>,
}

impl AuditLogger {
    pub fn new() -> Self { Self { records: RwLock::new(vec![]) } }

    pub async fn log(&self, entry: AuditEntry) {
        let record = AuditRecord { entry, timestamp: Utc::now().to_rfc3339() };
        self.records.write().await.push(record);
    }

    pub async fn query(&self, limit: usize) -> Vec<AuditRecord> {
        let records = self.records.read().await;
        records.iter().rev().take(limit).cloned().collect()
    }
}
