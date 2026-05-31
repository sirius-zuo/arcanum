mod file;
mod raw;
mod http;
mod database;
mod cloud_storage;
mod git;
mod connector;
mod registry;

pub use file::FileLoader;
pub use raw::RawLoader;
pub use http::HttpLoader;
pub use database::DatabaseLoader;
pub use cloud_storage::CloudStorageLoader;
pub use git::GitLoader;
pub use connector::ConnectorLoader;
pub use registry::LoaderRegistry;
