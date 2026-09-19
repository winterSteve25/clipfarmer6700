//! Embedded ClipFarmer backend.
//!
//! This crate contains the pipeline that was formerly driven only by the
//! `clipfarmer` CLI, plus a cancellable library runtime suitable for GUI hosts.

pub mod adapters;
pub mod config;
pub mod domain;
pub mod editorial;
pub mod evidence;
pub mod feedback;
pub mod manifest;
pub mod models;
pub mod pipeline;
pub mod progress;
pub mod publishers;
pub mod runtime;
pub mod store;
pub mod timeline;

pub use pipeline::{RunSummary, Service, ServiceDependencies};
pub use runtime::{
    Cancellation, CancellationHandle, JobProgress, JobSource, LibraryRunner, ModelPaths, RunResult,
};
