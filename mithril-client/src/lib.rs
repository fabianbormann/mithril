#![warn(missing_docs)]

//! Define everything necessary to list, download, and validate snapshots from a
//! [Mithril Aggregator](https://mithril.network/rust-doc/mithril_aggregator/index.html).
//!
//! To query an aggregator for snapshots & certificate use the [services::SnapshotService].
//!

pub mod aggregator_client;
pub mod commands;
mod entities;
mod message_adapters;
pub mod services;

pub use entities::*;
pub use message_adapters::{
    FromCertificateMessageAdapter, FromSnapshotListMessageAdapter, FromSnapshotMessageAdapter,
};
