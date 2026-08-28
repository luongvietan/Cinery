pub mod adapter;
pub mod commands;
pub mod dry_run;
pub mod error;
pub mod http;
pub mod mock;
pub mod model;
pub mod openai;
pub mod registry;
pub mod repository;
pub mod service;

pub use adapter::GenerationProvider;
pub use error::{redact_secret, ProviderError, ProviderErrorKind};
pub use model::*;
