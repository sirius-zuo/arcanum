pub mod detection;
pub mod sanitizer;
pub use detection::MimeDetector;

pub mod loaders;
pub mod preprocessors;
pub mod enrichment;
pub mod chunkers;
pub mod snapshot;
pub mod versioning;

pub use loaders::{
    FileLoader, RawLoader, HttpLoader,
    DatabaseLoader, CloudStorageLoader, GitLoader, ConnectorLoader,
    LoaderRegistry,
};
pub use preprocessors::{PreprocessorCatalog, DoclingPreprocessor, DoclingBackend};
pub use chunkers::{FixedSizeChunker, SemanticChunker, PropositionalChunker};
pub use enrichment::{ContextEnricher, EntityExtractor};
pub use snapshot::local::LocalSnapshotStore;
pub use versioning::postgres::PostgresDocumentVersionStore;
pub use versioning::sqlite::SqliteDocumentVersionStore;
pub use versioning::chunk_metadata::PostgresChunkMetadataStore;

pub mod registry;
pub use registry::{ChunkRegistry, default_registry};
