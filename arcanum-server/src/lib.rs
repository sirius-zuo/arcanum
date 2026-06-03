pub mod metrics;
pub mod portal;
pub mod routes {
    pub mod admin;
    pub mod api;
    pub mod graph;
    pub mod health;
    pub mod metrics;
}
pub mod server;
pub mod ws;
pub use server::build_app;
