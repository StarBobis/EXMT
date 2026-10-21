//! Geometry payload for the three.js preview.
//!
//! Ported from `preview._geometry_payload`: positions and normals are mirrored
//! on Z (Unity is left-handed, three.js is right-handed) and the triangle
//! winding is reversed to match. Arrays travel as base64 little-endian f32/u32.
//!
//! Keeping this typed (instead of emitting JSON) lets the shell own serde while
//! the asset logic stays dependency-free.

use crate::base64;
use crate::container::Node;
use crate::error::{Error, Result};
use crate::mesh::Mesh;
use crate::texconv::Texconv;
use crate::texture::Texture;

#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub name: String,
    pub count: u32,
    pub positions: String,
    pub normals: Option<String>,
    pub uvs: Option<String>,
    pub indices: String,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

fn mirror_z(src: &[f32]) -> Vec<f32> {
    src.chunks_exact(3)
        .flat_map(|v| [v[0], v[1], -v[2]])
        .collect()
}

fn bytes_f32(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Build the preview payload for one decoded mesh.
pub fn geometry(mesh: &Mesh) -> Result<Geometry> {
    let positions = mesh
        .streams
        .attributes
        .first()
        .and_then(|v| v.as_ref())
        .ok_or_else(|| Error::Invalid {
            what: "mesh geometry",
            detail: format!("mesh {:?} has no positions", mesh.name),
        })?;
    if positions.len() != mesh.vertex_count * 3 {
        return Err(Error::SizeMismatch {
            what: "mesh positions",
            got: positions.len(),
            want: mesh.vertex_count * 3,
        });
    }
    let positions = mirror_z(positions);
    let normals = mesh
        .streams
        .attributes
        .get(1)
        .and_then(|v| v.as_ref())
        .map(|v| mirror_z(v));
    let uvs = mesh
        .streams
        .attributes
        .get(4)
        .and_then(|v| v.as_ref())
        .map(|v| {
            // Positions are xyz; UVs are xy.
            v.chunks_exact(2)
                .flat_map(|uv| [uv[0], uv[1]])
                .collect::<Vec<f32>>()
        });

    // Unity's winding is clockwise for a left-handed space; reverse each
    // triangle after the Z mirror so the front face survives.
    let mut indices = Vec::with_capacity(mesh.indices.len() / 3 * 3);
    for tri in mesh.indices.chunks_exact(3) {
        indices.extend_from_slice(&[tri[2], tri[1], tri[0]]);
    }

    let mut bounds_min = [f32::INFINITY; 3];
    let mut bounds_max = [f32::NEG_INFINITY; 3];
    for v in positions.chunks_exact(3) {
        for i in 0..3 {
            bounds_min[i] = bounds_min[i].min(v[i]);
            bounds_max[i] = bounds_max[i].max(v[i]);
        }
    }
    if positions.is_empty() {
        bounds_min = [0.0; 3];
        bounds_max = [0.0; 3];
    }

    Ok(Geometry {
        name: mesh.name.clone(),
        count: mesh.vertex_count as u32,
        positions: base64::encode(&bytes_f32(&positions)),
        normals: normals.as_ref().map(|v| base64::encode(&bytes_f32(v))),
        uvs: uvs.as_ref().map(|v| base64::encode(&bytes_f32(v))),
        indices: base64::encode(
            &indices
                .iter()
                .flat_map(|i| i.to_le_bytes())
                .collect::<Vec<u8>>(),
        ),
        bounds_min,
        bounds_max,
    })
}

/// One downscaled texture thumbnail, PNG bytes already base64-encoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Thumb {
    pub name: String,
    /// Source dimensions, which is what the Python cache records (the PNG
    /// itself is downscaled).
    pub width: u32,
    pub height: u32,
    pub png: String,
}

/// Downscale one texture for the grid. `limit` is the longest side.
pub fn thumb(texture: &Texture, nodes: &[Node], texconv: &Texconv, limit: usize) -> Result<Thumb> {
    let (_, _, png) = texture.decode_png(nodes, texconv, Some(limit))?;
    Ok(Thumb {
        name: texture.name.clone(),
        width: texture.width as u32,
        height: texture.height as u32,
        png: base64::encode(&png),
    })
}

/// A diffuse map tied to the submesh that samples it.
#[derive(Debug, Clone, PartialEq)]
pub struct Diffuse {
    pub mesh: String,
    pub texture: String,
    pub width: u32,
    pub height: u32,
    pub png: String,
}

pub fn diffuse(
    texture: &Texture,
    mesh: &str,
    nodes: &[Node],
    texconv: &Texconv,
    limit: usize,
) -> Result<Diffuse> {
    let (_, _, png) = texture.decode_png(nodes, texconv, Some(limit))?;
    Ok(Diffuse {
        mesh: mesh.to_string(),
        texture: texture.name.clone(),
        width: texture.width as u32,
        height: texture.height as u32,
        png: base64::encode(&png),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{Channel, VertexStreams};
    use crate::typetree::Value;

    fn texture_tree() -> Value {
        let image = [0x00u8, 0xF8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // 4x4 red
        let field = |name: &str, value: Value| (name.to_string(), value);
        Value::Struct(vec![
            field("m_Name", Value::Str("red".into())),
            field("m_Width", Value::I32(4)),
            field("m_Height", Value::I32(4)),
            field("m_CompleteImageSize", Value::I32(8)),
            field("m_TextureFormat", Value::I32(10)),
            field("m_MipCount", Value::I32(1)),
            field("image data", Value::Bytes(image.to_vec())),
            field(
                "m_StreamData",
                Value::Struct(vec![
                    field("offset", Value::U32(0)),
                    field("size", Value::U32(0)),
                    field("path", Value::Str(String::new())),
                ]),
            ),
        ])
    }

    #[test]
    fn thumbnails_decode_through_texconv() {
        let Some(tool) = Texconv::locate() else {
            eprintln!("skip: texconv not available");
            return;
        };
        let texture = Texture::from_value(&texture_tree()).unwrap();
        let thumb = thumb(&texture, &[], &tool, 2).unwrap();
        assert_eq!(thumb.name, "red");
        assert_eq!((thumb.width, thumb.height), (4, 4));
        assert!(!thumb.png.is_empty());
        // PNG magic base64: iVBORw0KGgo
        assert!(thumb.png.starts_with("iVBORw0KGgo"));
    }

    fn mesh() -> Mesh {
        let positions: Vec<f32> = vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 1.0, 1.0, 2.0,
        ];
        let normals: Vec<f32> = (0..5).flat_map(|_| [0.0, 0.0, 1.0]).collect();
        let uvs: Vec<f32> = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.5, 0.5];
        let mut attributes: Vec<Option<Vec<f32>>> = vec![None; 14];
        attributes[0] = Some(positions);
        attributes[1] = Some(normals);
        attributes[4] = Some(uvs);
        Mesh {
            name: "test_mesh".into(),
            vertex_count: 5,
            channels: [Channel::UNUSED; 14],
            streams: VertexStreams {
                attributes,
                raw: Vec::new(),
            },
            indices: vec![0, 1, 2, 3, 4, 0],
            index_format: 1,
            submesh_index_counts: vec![6],
            bindpose: Vec::new(),
            bone_hashes: Vec::new(),
            aabb_center: [0.0; 3],
            aabb_extent: [0.0; 3],
        }
    }

    #[test]
    fn matches_the_python_payload() {
        // Expected strings generated with numpy + base64 for the same vertices.
        let g = geometry(&mesh()).unwrap();
        assert_eq!(g.name, "test_mesh");
        assert_eq!(g.count, 5);
        assert_eq!(
            g.positions,
            "AAAAAAAAAAAAAACAAACAPwAAAAAAAACAAAAAAAAAgD8AAACAAAAAAAAAAAAAAADAAACAPwAAgD8AAADA"
        );
        assert_eq!(
            g.normals.as_deref(),
            Some(
                "AAAAAAAAAAAAAIC/AAAAAAAAAAAAAIC/AAAAAAAAAAAAAIC/AAAAAAAAAAAAAIC/AAAAAAAAAAAAAIC/"
            )
        );
        assert_eq!(
            g.uvs.as_deref(),
            Some("AAAAAAAAAAAAAIA/AAAAAAAAAAAAAIA/AACAPwAAgD8AAAA/AAAAPw==")
        );
        assert_eq!(g.indices, "AgAAAAEAAAAAAAAAAAAAAAQAAAADAAAA");
        assert_eq!(g.bounds_min, [0.0, 0.0, -2.0]);
        assert_eq!(g.bounds_max, [1.0, 1.0, -0.0]);
    }

    #[test]
    fn missing_positions_is_an_error() {
        let mut m = mesh();
        m.streams.attributes[0] = None;
        assert!(geometry(&m).is_err());
    }
}
