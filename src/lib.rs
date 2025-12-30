// Library crate - exports shared types and processing logic

pub mod types;
pub mod processing;
pub mod supabase;
pub mod api;
pub mod streams;
pub mod execution;
pub mod trading;

// Re-export commonly used types
pub use types::*;
pub use processing::ProcessingState;
