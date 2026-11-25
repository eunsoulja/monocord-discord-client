//! Runtime host: tokio-based data-source hub with shared, deduplicated
//! pollers and push channels.
//!
//! Widgets never touch network/disk directly. They declare `DataNeed`s; the
//! hub subscribes them to a shared fetcher per unique need key and stops
//! pollers when the last subscriber goes away.

pub mod hub;
pub mod probes;

pub use hub::{DataHub, FetchFn, HubEvent};
