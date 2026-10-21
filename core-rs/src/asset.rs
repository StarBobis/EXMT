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
use crate::serialized::{ObjectInfo, SerializedFile};
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

/// Open only the SerializedFile (node 0) of a bundle.
///
/// The scan pass needs object names and prefab hierarchies, both of which live
/// in node 0; sibling `.resS` stream nodes can hold most of the file and are
/// never touched. This is what keeps a full game scan from reading every byte
/// of the install twice.
/// Returns the SerializedFile and the number of bytes actually read.
pub fn open_serialized(path: impl AsRef<Path>) -> Result<(SerializedFile, u64)> {
    let path = path.as_ref();
    let index = container::open_file_index(path)?;
    let (data, bytes) = index.read_node_with_len(path, 0)?;
    let serialized = SerializedFile::parse(&data)?;
    // The header and block table were read too; they are small but the ETA is
    // more honest when they are counted.
    Ok((serialized, bytes + index.info.data_offset as u64))
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

    /// First object of `class_id` whose `m_Name` matches, without decoding the
    /// rest of the bundle.
    ///
    /// Name-first type trees are the norm, so this is a handful of string
    /// comparisons. Objects whose type tree does not lead with `m_Name` are
    /// decoded and compared in a second pass, which keeps the fast path from
    /// changing which object wins.
    fn find_named(&self, class_id: i32, name: &str) -> Option<(i64, &ObjectInfo)> {
        let mut needs_decode = false;
        for (path_id, obj) in &self.serialized.objects {
            let Some(ty) = self.serialized.type_of(obj) else { continue };
            if ty.class_id != class_id {
                continue;
            }
            match self.serialized.peek_name_matches(obj, name) {
                Some(true) => return Some((*path_id, obj)),
                Some(false) => {}
                None => needs_decode = true,
            }
        }
        if needs_decode {
            for (path_id, obj) in &self.serialized.objects {
                let Some(ty) = self.serialized.type_of(obj) else { continue };
                if ty.class_id != class_id {
                    continue;
                }
                if self.serialized.peek_name_matches(obj, name).is_some() {
                    continue;
                }
                let Ok(value) = typetree::read_value(&self.serialized, obj) else { continue };
                if value.get("m_Name").and_then(|v| v.as_str()) == Some(name) {
                    return Some((*path_id, obj));
                }
            }
        }
        None
    }

    /// First mesh whose `m_Name` matches exactly, with its object path ID.
    pub fn mesh_object(&self, name: &str) -> Result<(i64, Mesh)> {
        if let Some((path_id, obj)) = self.find_named(CLASS_MESH, name) {
            let value = typetree::read_value(&self.serialized, obj)?;
            return Ok((path_id, mesh::from_value_with_nodes(&value, &self.nodes)?));
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
        if let Some((path_id, obj)) = self.find_named(CLASS_TEXTURE, name) {
            let value = typetree::read_value(&self.serialized, obj)?;
            return Ok((path_id, Texture::from_value(&value)?));
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
            .map(|(_, v)| mesh::from_value_with_nodes(v, &self.nodes))
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
        if self.info.trailing > 0 {
            return Err(Error::Unsupported {
                what: "writing a bundle that concatenates several UnityFS archives",
            });
        }
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
