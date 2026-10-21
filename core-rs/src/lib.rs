//! Self-hosted Unity asset pipeline for EXMT Modbox.
//!
//! Everything in this crate parses or writes binary Unity formats and depends
//! only on the standard library. The goal is a pipeline we fully understand and
//! can fix ourselves, rather than one that moves when an upstream crate changes.
//!
//! Layers, bottom up:
//!
//! * [`lz4`] -- the block codec UnityFS compresses each data block with.
//! * container (todo) -- UnityFS header, block table, NEP2 prefix encryption.
//! * serialized (todo) -- the SerializedFile that a bundle wraps.
//! * typetree (todo) -- decoding objects through their embedded type trees.
//!
//! The Python implementation under `core/` is kept as a differential oracle:
//! the same fixtures are asserted on both sides while the Rust path is built out,
//! and it is removed once Rust covers them.

pub mod commonstrings;
pub mod container;
pub mod error;
pub mod lz4;
pub mod serialized;

pub use error::{Error, Result};
