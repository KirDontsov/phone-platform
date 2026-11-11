//! Deterministic business rules independent of providers and persistence.
pub mod callbacks;
pub mod filters;
pub mod scoring;
pub use callbacks::{callback_episodes, contact_kind, human_connected, validate_callback_policy};
pub use filters::{matches_filter, validate_filter};
pub use scoring::validate_analysis;
