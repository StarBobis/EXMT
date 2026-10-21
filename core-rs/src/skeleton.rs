//! Merge per-mesh bone tables into one skeleton for a skin.
//!
//! Each GF2 mesh carries only the bones *it* skins (`m_BindPose` +
//! `m_BoneNameHashes`), and those subsets differ per mesh. The workspace export
//! needs the union, with the rest transform (bone -> world) taken from the first
//! mesh that declares a bone.
//!
//! This mirrors `core/exmt_core/skeleton.py`: the same CRC32 name hashing, the
//! same conflict tolerance, cycle removal and 56-byte unique-name rule.

use std::collections::HashMap;

use crate::crc32::crc32;
use crate::error::{Error, Result};

pub const FORMAT: &str = "exmt-skeleton-1";
const BONE_PREFIX: &str = "bone_";

#[derive(Debug, Clone)]
pub struct Bone {
    pub hash: u32,
    pub name: String,
    pub parent: i32,
    /// Rest transform (bone -> world), row-major 4x4.
    pub rest: [f64; 16],
    pub meshes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub hash: u32,
    pub mesh: String,
    pub deviation: f64,
}

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub id: String,
    pub bones: Vec<Bone>,
    pub local: HashMap<String, Vec<u32>>,
    pub conflicts: Vec<Conflict>,
    pub named: usize,
}

/// The per-`.exmtmesh` view embedded in every export file.
#[derive(Debug, Clone)]
pub struct View {
    pub id: String,
    pub count: usize,
    pub named: usize,
    pub bones: Vec<BoneView>,
    pub local: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct BoneView {
    pub hash: u32,
    pub name: String,
    pub parent: i32,
    pub rest: [f64; 16],
}

impl Skeleton {
    pub fn view_for(&self, mesh: &str) -> View {
        View {
            id: self.id.clone(),
            count: self.bones.len(),
            named: self.named,
            bones: self
                .bones
                .iter()
                .map(|bone| BoneView {
                    hash: bone.hash,
                    name: bone.name.clone(),
                    parent: bone.parent,
                    rest: bone.rest,
                })
                .collect(),
            local: self.local.get(mesh).cloned().unwrap_or_default(),
        }
    }
}

/// Merge `(mesh name, bone hashes, bindpose)` tables.
///
/// `bindpose` is flat 16 floats per bone (row-major, world -> bone); it is
/// inverted to the rest transform. `paths` maps bone hash -> full transform path
/// and is optional; without it names fall back to `bone_XXXXXXXX`.
pub fn merge(
    tables: &[(String, Vec<u32>, Vec<f32>)],
    paths: &HashMap<u32, String>,
    tolerance: f64,
) -> Result<Skeleton> {
    let mut bones: Vec<Bone> = Vec::new();
    let mut index_of: HashMap<u32, usize> = HashMap::new();
    let mut local: HashMap<String, Vec<u32>> = HashMap::new();
    let mut conflicts: Vec<Conflict> = Vec::new();

    for (mesh_name, hashes, bindpose) in tables {
        let rest = if bindpose.is_empty() {
            None
        } else {
            if bindpose.len() != hashes.len() * 16 {
                return Err(Error::Invalid {
                    what: "skeleton",
                    detail: format!(
                        "{mesh_name}: {} bindpose floats for {} bones",
                        bindpose.len(),
                        hashes.len()
                    ),
                });
            }
            let mut matrices = Vec::with_capacity(hashes.len());
            for i in 0..hashes.len() {
                let mut m = [0.0f64; 16];
                for (j, value) in bindpose[i * 16..(i + 1) * 16].iter().enumerate() {
                    m[j] = *value as f64;
                }
                matrices.push(invert4(m).map_err(|e| Error::Invalid {
                    what: "skeleton bindpose",
                    detail: format!("{mesh_name} bone {i}: {e}"),
                })?);
            }
            Some(matrices)
        };

        let mut mapping = Vec::with_capacity(hashes.len());
        for (i, &hash) in hashes.iter().enumerate() {
            let matrix = rest.as_ref().map(|m| m[i]).unwrap_or_else(identity4);
            let gi = if let Some(&existing) = index_of.get(&hash) {
                let deviation = max_deviation(&bones[existing].rest, &matrix);
                if deviation > tolerance {
                    conflicts.push(Conflict {
                        hash,
                        mesh: mesh_name.clone(),
                        deviation: round6(deviation),
                    });
                }
                if !bones[existing].meshes.iter().any(|m| m == mesh_name) {
                    bones[existing].meshes.push(mesh_name.clone());
                }
                existing
            } else {
                let gi = bones.len();
                index_of.insert(hash, gi);
                bones.push(Bone {
                    hash,
                    name: bone_name(hash),
                    parent: -1,
                    rest: matrix,
                    meshes: vec![mesh_name.clone()],
                });
                gi
            };
            mapping.push(gi as u32);
        }
        local.insert(mesh_name.clone(), mapping);
    }

    let mut named = 0usize;
    if !paths.is_empty() {
        let mut names = HashMap::new();
        let mut parents = HashMap::new();
        for (&hash, path) in paths {
            let leaf = path.rsplit('/').next().unwrap_or(path);
            if !leaf.is_empty() {
                names.insert(hash, leaf.to_string());
            }
            if let Some(cut) = path.rfind('/') {
                if cut > 0 {
                    parents.insert(hash, crc32(path[..cut].as_bytes()));
                }
            }
        }
        for bone in &mut bones {
            if let Some(real) = names.get(&bone.hash) {
                if !real.starts_with(BONE_PREFIX) {
                    bone.name = real.clone();
                    named += 1;
                }
            }
        }
        for bone in &mut bones {
            if let Some(parent_hash) = parents.get(&bone.hash) {
                if let Some(&parent) = index_of.get(parent_hash) {
                    if parent != index_of[&bone.hash] {
                        bone.parent = parent as i32;
                    }
                }
            }
        }
        resolve_cycles(&mut bones);
    }
    unique_names(&mut bones);

    Ok(Skeleton {
        id: String::new(),
        bones,
        local,
        conflicts,
        named,
    })
}

pub fn bone_name(hash: u32) -> String {
    format!("{BONE_PREFIX}{hash:08X}")
}

fn resolve_cycles(bones: &mut [Bone]) {
    for i in 0..bones.len() {
        let mut seen = vec![i];
        let mut current = bones[i].parent;
        while current >= 0 {
            let at = current as usize;
            if seen.contains(&at) {
                bones[i].parent = -1;
                break;
            }
            seen.push(at);
            current = bones.get(at).map(|b| b.parent).unwrap_or(-1);
        }
    }
}

fn unique_names(bones: &mut [Bone]) {
    let mut used = std::collections::HashSet::new();
    for bone in bones {
        let base: String = bone
            .name
            .trim()
            .chars()
            .take(56)
            .collect::<String>()
            .trim_end()
            .to_string();
        let base = if base.is_empty() {
            bone_name(bone.hash)
        } else {
            base
        };
        let mut name = base.clone();
        let mut n = 1;
        while used.contains(&name) {
            n += 1;
            name = format!("{base}_{n}");
        }
        used.insert(name.clone());
        bone.name = name;
    }
}

fn identity4() -> [f64; 16] {
    let mut m = [0.0f64; 16];
    for i in 0..4 {
        m[i * 5] = 1.0;
    }
    m
}

fn max_deviation(a: &[f64; 16], b: &[f64; 16]) -> f64 {
    (0..16).map(|i| (a[i] - b[i]).abs()).fold(0.0, f64::max)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

/// Gauss-Jordan inverse with partial pivoting; row-major 4x4.
fn invert4(m: [f64; 16]) -> std::result::Result<[f64; 16], &'static str> {
    let mut a = m;
    let mut inv = identity4();
    for col in 0..4 {
        let mut pivot = col;
        for row in col + 1..4 {
            if a[row * 4 + col].abs() > a[pivot * 4 + col].abs() {
                pivot = row;
            }
        }
        if a[pivot * 4 + col].abs() < 1e-12 {
            return Err("singular matrix");
        }
        if pivot != col {
            for k in 0..4 {
                a.swap(col * 4 + k, pivot * 4 + k);
                inv.swap(col * 4 + k, pivot * 4 + k);
            }
        }
        let d = a[col * 4 + col];
        for k in 0..4 {
            a[col * 4 + k] /= d;
            inv[col * 4 + k] /= d;
        }
        for row in 0..4 {
            if row == col {
                continue;
            }
            let f = a[row * 4 + col];
            if f == 0.0 {
                continue;
            }
            for k in 0..4 {
                a[row * 4 + k] -= f * a[col * 4 + k];
                inv[row * 4 + k] -= f * inv[col * 4 + k];
            }
        }
    }
    Ok(inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translation(x: f32, y: f32, z: f32) -> Vec<f32> {
        let mut m = vec![0.0f32; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        m[12] = x;
        m[13] = y;
        m[14] = z;
        m
    }

    fn identity() -> Vec<f32> {
        translation(0.0, 0.0, 0.0)
    }

    #[test]
    fn inverse_of_a_translation_is_a_negative_translation() {
        let inv = invert4([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 3.0, 4.0, 1.0,
        ])
        .unwrap();
        assert_eq!(&inv[12..15], &[-2.0, -3.0, -4.0]);
        assert!(invert4([0.0; 16]).is_err());
    }

    #[test]
    fn merge_unions_hashes_and_inverts_bindpose() {
        // meshA: bone 10 translated +1 X, bone 20 at identity.
        let mut a_pose = translation(1.0, 0.0, 0.0);
        a_pose.extend(identity());
        // meshB: bone 20 translated +1 X/+2 Y (conflict), bone 30 at identity.
        let mut b_pose = translation(1.0, 2.0, 0.0);
        b_pose.extend(identity());
        let a = (String::from("meshA"), vec![10, 20], a_pose);
        let b = (String::from("meshB"), vec![20, 30], b_pose);
        let skel = merge(&[a, b], &HashMap::new(), 0.01).unwrap();
        assert_eq!(skel.bones.len(), 3);
        assert_eq!(skel.local["meshA"], vec![0, 1]);
        assert_eq!(skel.local["meshB"], vec![1, 2]);
        // bindpose translates +1 on X, so the rest transform translates -1.
        assert_eq!(skel.bones[0].rest[12], -1.0);
        // meshB declares bone 20 with a different rest -> conflict.
        assert_eq!(skel.conflicts.len(), 1);
        assert_eq!(skel.conflicts[0].mesh, "meshB");
    }

    #[test]
    fn names_and_parents_come_from_crc32_paths() {
        let paths = HashMap::from([
            (crc32(b"Root"), "Root".to_string()),
            (crc32(b"Root/Hips"), "Root/Hips".to_string()),
            (crc32(b"Root/Hips/Spine"), "Root/Hips/Spine".to_string()),
        ]);
        let table = (
            String::from("mesh"),
            vec![
                crc32(b"Root"),
                crc32(b"Root/Hips"),
                crc32(b"Root/Hips/Spine"),
            ],
            Vec::new(),
        );
        let skel = merge(&[table], &paths, 0.01).unwrap();
        assert_eq!(skel.named, 3);
        assert_eq!(skel.bones[0].name, "Root");
        assert_eq!(skel.bones[1].parent, 0);
        assert_eq!(skel.bones[2].parent, 1);
        assert_eq!(skel.view_for("mesh").local, vec![0, 1, 2]);
    }

    #[test]
    fn duplicate_and_empty_names_are_made_unique() {
        let paths = HashMap::from([(1u32, "A/Bone".to_string()), (2u32, "B/Bone".to_string())]);
        let table = (String::from("mesh"), vec![1, 2], Vec::new());
        let skel = merge(&[table], &paths, 0.01).unwrap();
        assert_eq!(skel.bones[0].name, "Bone");
        assert_eq!(skel.bones[1].name, "Bone_2");
    }

    #[test]
    fn cyclic_parents_are_broken() {
        // Build paths that point at each other after hashing is impossible with
        // real paths, so exercise the defensive resolver directly.
        let mut bones = vec![
            Bone {
                hash: 1,
                name: "a".into(),
                parent: 1,
                rest: identity4(),
                meshes: vec![],
            },
            Bone {
                hash: 2,
                name: "b".into(),
                parent: 0,
                rest: identity4(),
                meshes: vec![],
            },
        ];
        resolve_cycles(&mut bones);
        assert_eq!(bones[0].parent, -1);
        assert_eq!(bones[1].parent, 0);
    }
}
