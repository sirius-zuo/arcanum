pub mod attribute;
pub mod batch;
pub mod hydrate;
pub mod judge;
pub mod quote;
pub mod response;
pub mod segment;
pub mod verdict;

pub use attribute::{attribute, Attribution};
pub use batch::*;
pub use hydrate::{join_chunks, HydratedPassage, JoinError};
pub use judge::*;
pub use quote::locate;
pub use response::build_sentences;
pub use segment::{segment, Unit};
pub use verdict::{overall, sentence_verdict};
