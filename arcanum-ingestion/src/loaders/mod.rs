mod cloud_storage;
mod connector;
mod database;
mod file;
mod git;
mod http;
mod raw;
mod registry;

pub use cloud_storage::CloudStorageLoader;
pub use connector::ConnectorLoader;
pub use database::DatabaseLoader;
pub use file::FileLoader;
pub use git::GitLoader;
pub use http::HttpLoader;
pub use raw::RawLoader;
pub use registry::LoaderRegistry;
