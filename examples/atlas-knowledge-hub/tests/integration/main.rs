// All integration tests live in this ONE test binary. Add new tests as modules here
// (never as new files directly under tests/): every top-level file under tests/ becomes
// its own crate that links the whole engine plus LanceDB (~510 MB per binary).
mod common;
mod demo_basic;
mod demo_ingest;
mod demo_library;
mod engine_setup;
mod samples;
