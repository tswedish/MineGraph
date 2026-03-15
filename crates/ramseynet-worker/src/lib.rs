//! RamseyNet worker: CLI binary and worker web-app (visualization + control).
//!
//! The worker web-app provides live search visualization via an embedded
//! web server. In the future, it will also provide operator controls for
//! starting/stopping searches, configuring strategies, and browsing results.

pub mod viz;

pub const WORKER_VERSION: &str = "0.2.0";
