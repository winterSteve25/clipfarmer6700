//! ClipFarmer is an evidence-first short-form publishing pipeline.
//! Adapters are deliberately thin: all irreversible work is recorded first.
pub mod adapters;
pub mod config;
pub mod domain;
pub mod editorial;
pub mod evidence;
pub mod feedback;
pub mod manifest;
pub mod pipeline;
pub mod publishers;
pub mod store;
pub mod timeline;

pub use pipeline::{Service, ServiceDependencies};
