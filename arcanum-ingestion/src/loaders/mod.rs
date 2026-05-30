mod file;
mod raw;
mod http;
mod registry;
pub use file::FileLoader;
pub use raw::RawLoader;
pub use http::HttpLoader;
pub use registry::LoaderRegistry;
