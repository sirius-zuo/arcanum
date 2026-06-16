pub mod detection;
pub mod sanitizer;
pub use detection::MimeDetector;

pub mod loaders;
pub mod preprocessors;
pub mod enrichment;
pub mod chunkers;
pub mod metadata;
pub mod snapshot;
pub mod versioning;

pub use loaders::{
    FileLoader, RawLoader, HttpLoader,
    DatabaseLoader, CloudStorageLoader, GitLoader, ConnectorLoader,
    LoaderRegistry,
};
pub use preprocessors::{HtmlCleaner, PdfParser, EpubParser, PreprocessorRegistry, DoclingPreprocessor, DoclingBackend};
pub use chunkers::{FixedSizeChunker, SemanticChunker, PropositionalChunker};
pub use enrichment::{ContextEnricher, EntityExtractor};
pub use snapshot::local::LocalSnapshotStore;
pub use versioning::postgres::PostgresDocumentVersionStore;

// TODO: Task 6 replaces this with versioning/postgres.rs
// pub mod document_registry;
// pub use document_registry::SqliteDocumentRegistry;

pub mod registry;
pub use registry::{ChunkRegistry, default_registry};
