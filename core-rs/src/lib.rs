//! Self-hosted Unity asset pipeline for EXMT Modbox.
//!
//! Everything in this crate parses or writes binary Unity formats and depends
//! only on the standard library. The goal is a pipeline we fully understand and
//! can fix ourselves, rather than one that moves when an upstream crate changes.
//!
//! Layers, bottom up:
//!
//! * [`lz4`] -- the block codec UnityFS compresses each data block with.
//! * [`container`] -- UnityFS header, block table, node ranges, NEP2 prefix
//!   encryption.
//! * [`serialized`] -- the SerializedFile that a bundle wraps.
//! * [`typetree`] -- decoding objects through their embedded type trees.
//! * [`asset`] -- opening a bundle and finding its meshes and textures.
//!
//! This crate is the whole runtime: there is no script interpreter, no helper
//! process and no Python fallback. Image work goes through the bundled
//! `texconv.exe`, which is a native executable rather than a script.

pub mod asset;
pub mod anim;
pub mod base64;
pub mod bc;
pub mod commonstrings;
pub mod container;
pub mod crc32;
pub mod dds;
pub mod error;
pub mod inflate;
pub mod lz4;
pub mod mesh;
pub mod npy;
pub mod npz;
pub mod serialized;
pub mod skeleton;
pub mod skin;
pub mod texconv;
pub mod texture;
pub mod typetree;
pub mod zip;

pub use error::{Error, Result};
