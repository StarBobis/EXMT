//! Opening a UnityFS bundle and finding the objects the studio asks for.
//!
//! This is the sixth layer: container -> SerializedFile -> type-tree value ->
//! [`Mesh`]/[`Texture`]. Keeping it here means the shell never assembles those
//! four steps itself, and tools/tests get the same view as the app.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::container::{self, BundleInfo, Node};
use crate::error::{Error, Result};
use crate::mesh::{self, Mesh};
use crate::serialized::SerializedFile;
use crate::texture::Texture;
use crate::typetree::{self, Value};

pub const CLASS_MESH: i32 = 43;
pub const CLASS_TEXTURE: i32 = 28;

/// A bundle decrypted, split into nodes and parsed as a SerializedFile.
#[derive(Debug, Clone)]
pub struct OpenedBundle {
    pub path: PathBuf,
    pub serialized: SerializedFile,
    pub nodes: Vec<Node>,
    /// Container header template, reused when rebuilding the bundle.
    pub info: BundleInfo,
    /// Original NEP2 ciphertext header; re-encryption must reproduce it.
    pub cipher_head: [u8; 4],
}

/// Read and open a bundle from disk.
pub fn open(path: impl AsRef<Path>) -> Result<OpenedBundle> {
    let path = path.as_ref();
    let raw = std::fs::read(path)?;
    let mut opened = open_bytes(&raw)?;
    opened.path = path.to_path_buf();
    Ok(opened)
}

/// The same, from bytes already in memory (bundles inside another archive).
pub fn open_bytes(raw: &[u8]) -> Result<OpenedBundle> {
    if raw.len() < 4 {
        return Err(Error::Truncated {
            what: "bundle header",
            offset: 0,
            wanted: 4,
            available: raw.len(),
        });
    }
    let mut cipher_head = [0u8; 4];
    cipher_head.copy_from_slice(&raw[..4]);
    let plain = container::decrypt(raw)?;
    let info = container::parse(&plain)?;
    let nodes = container::node_payloads(&plain, &info)?;
    let first = nodes.first().ok_or(Error::Invalid {
        what: "bundle",
        detail: "no nodes".into(),
    })?;
    let serialized = SerializedFile::parse(&first.data)?;
    Ok(OpenedBundle {
        path: PathBuf::new(),
        serialized,
        nodes,
        info,
        cipher_head,
    })
}

impl OpenedBundle {
    /// Decoded objects of a class, in file order.
    pub fn values_of(&self, class_id: i32) -> Result<Vec<(i64, Value)>> {
        let mut out = Vec::new();
        for (path_id, obj) in &self.serialized.objects {
            let Some(ty) = self.serialized.type_of(obj) else {
                continue;
            };
            if ty.class_id != class_id {
                continue;
            }
            out.push((*path_id, typetree::read_value(&self.serialized, obj)?));
        }
        Ok(out)
    }

    /// First mesh whose `m_Name` matches exactly, with its object path ID.
    pub fn mesh_object(&self, name: &str) -> Result<(i64, Mesh)> {
        for (path_id, value) in self.values_of(CLASS_MESH)? {
            let found = mesh::from_value(&value)?;
            if found.name == name {
                return Ok((path_id, found));
            }
        }
        Err(Error::Invalid {
            what: "mesh",
            detail: format!("{name:?} not found in bundle"),
        })
    }

    /// First mesh whose `m_Name` matches exactly.
    pub fn mesh(&self, name: &str) -> Result<Mesh> {
        Ok(self.mesh_object(name)?.1)
    }

    /// First Texture2D whose `m_Name` matches exactly, with its object path ID.
    pub fn texture_object(&self, name: &str) -> Result<(i64, Texture)> {
        for (path_id, value) in self.values_of(CLASS_TEXTURE)? {
            let found = Texture::from_value(&value)?;
            if found.name == name {
                return Ok((path_id, found));
            }
        }
        Err(Error::Invalid {
            what: "texture",
            detail: format!("{name:?} not found in bundle"),
        })
    }

    /// First Texture2D whose `m_Name` matches exactly.
    pub fn texture(&self, name: &str) -> Result<Texture> {
        Ok(self.texture_object(name)?.1)
    }

    pub fn meshes(&self) -> Result<Vec<Mesh>> {
        self.values_of(CLASS_MESH)?
            .iter()
            .map(|(_, v)| mesh::from_value(v))
            .collect()
    }

    pub fn textures(&self) -> Result<Vec<Texture>> {
        self.values_of(CLASS_TEXTURE)?
            .iter()
            .map(|(_, v)| Texture::from_value(v))
            .collect()
    }

    /// Rebuild the bundle with some objects' payloads replaced.
    ///
    /// The SerializedFile is rebuilt from its own bytes (type trees and tables
    /// copied verbatim), then the container is rebuilt and re-encrypted with
    /// the ciphertext header the source used. Passing an empty map is a useful
    /// identity check: for a real GF2 bundle it must reproduce the same object
    /// payloads, though the LZ4 stream will differ.
    pub fn save_patched(
        &self,
        replacements: &BTreeMap<i64, Vec<u8>>,
        compress: bool,
    ) -> Result<Vec<u8>> {
        let mut nodes = self.nodes.clone();
        let first = nodes.first_mut().ok_or(Error::Invalid {
            what: "bundle",
            detail: "no nodes".into(),
        })?;
        first.data = self.serialized.rebuild(replacements)?;
        let plain = container::build(&nodes, &self.info, compress)?;
        container::encrypt(&plain, &self.cipher_head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_bundle_is_rejected() {
        assert!(open_bytes(b"not a bundle").is_err());
    }

    #[test]
    fn missing_bundle_is_an_io_error() {
        let result = open(Path::new("definitely-not-here.bundle"));
        assert!(result.is_err());
    }
}
