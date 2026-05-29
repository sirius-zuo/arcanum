use sha2::{Sha256, Digest};

pub struct DocumentHashTracker;

impl DocumentHashTracker {
    pub fn compute_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }
}
