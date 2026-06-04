use thiserror::Error;

#[derive(Error, Debug)]
pub enum ArcanumError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("embedding error: {0}")]
    Embedding(String),
    #[error("enrichment error: {0}")]
    Enrichment(String),
    #[error("ingestion error: {0}")]
    Ingestion(String),
    #[error("retrieval error: {0}")]
    Retrieval(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("authentication error: {0}")]
    Auth(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("queue full")]
    QueueFull,
    #[error("pipeline error in stage '{stage}': {message}")]
    Pipeline { stage: String, message: String },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ArcanumError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = ArcanumError::Storage("connection refused".into());
        assert_eq!(e.to_string(), "storage error: connection refused");
    }

    #[test]
    fn test_queue_full_display() {
        assert_eq!(ArcanumError::QueueFull.to_string(), "queue full");
    }

    #[test]
    fn test_result_type() {
        let ok: Result<i32> = Ok(42);
        assert!(ok.is_ok());
        let err: Result<i32> = Err(ArcanumError::NotFound("chunk".into()));
        assert!(err.is_err());
    }
}
