//! Unit tests for `docker_tui`, laid out to mirror `src/`.
//!
//! Everything here exercises the crate through its public API; shared mocks
//! and builders live in `support`.

mod app;
mod config;
mod docker;
mod event;
mod format;
mod hub;
mod runtime;
mod support;
mod ui;
