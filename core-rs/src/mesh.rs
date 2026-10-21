//! Mesh (class 43) vertex streams: decode `m_DataSize` into attributes and back.
//!
//! `m_VertexData.m_Channels` has 14 slots, one per vertex attribute, each
//! naming a stream index, a byte offset within that stream, a format and a
//! dimension. Attributes with dimension 0 are unused.
//!
//! Layout rules (each verified against a real mesh):
//!
//! * A stream's stride is the largest `offset + size*dim` any attribute needs
//!   in it. The stride is **not** padded.
//! * Streams start one after another, but each start is aligned up to 16 bytes.
//!   For GF2's character meshes that means strides 40 / 20 / 32 with two small
//!   padding gaps, giving exactly `m_DataSize` bytes for N vertices.
//! * Padding is skipped, not interpreted: 2283 * 92 = 210036, yet the blob is
//!   210048.
//!
//! Values are stored as `f32` after normalisation. Integer formats that are not
//! normalised (bone indices) keep their exact value, which is safe because a
//! mesh cannot have more than 2^24 bones.

use crate::error::{Error, Result};
use crate::typetree::Value;

/// Vertex attribute slots, in Unity's enum order.
pub const ATTRS: [(&str, u32); 14] = [
    ("position", 3),
    ("normal", 3),
    ("tangent", 4),
    ("color", 4),
    ("uv0", 2),
    ("uv1", 2),
    ("uv2", 2),
    ("uv3", 2),
    ("uv4", 2),
    ("uv5", 2),
    ("uv6", 2),
    ("uv7", 2),
    ("weights", 4),
    ("bones", 4),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fmt {
    F32,
    F16,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
}

/// VertexAttributeFormat index -> (format, byte width).
pub fn format_of(code: u32) -> Option<(Fmt, usize)> {
    Some(match code {
        0 => (Fmt::F32, 4),
        1 => (Fmt::F16, 2),
        2 => (Fmt::U8, 1),
        3 => (Fmt::I8, 1),
        4 => (Fmt::U16, 2),
        5 => (Fmt::I16, 2),
        6 => (Fmt::U8, 1),
        7 => (Fmt::I8, 1),
        8 => (Fmt::U16, 2),
        9 => (Fmt::I16, 2),
        10 => (Fmt::U32, 4),
        11 => (Fmt::I32, 4),
        _ => return None,
    })
}

/// Normalisation divisor for a format code; 1.0 means "store as-is".
pub fn normaliser(code: u32) -> f32 {
    match code {
        2 => 255.0,
        3 => 127.0,
        4 => 65535.0,
        5 => 32767.0,
        _ => 1.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    pub stream: u32,
    pub offset: u32,
    pub format: u32,
    pub dimension: u32,
}

impl Channel {
    pub const UNUSED: Channel = Channel { stream: 0, offset: 0, format: 0, dimension: 0 };

    pub fn is_used(&self) -> bool {
        self.dimension != 0
    }

    /// Bytes this attribute occupies inside its stream.
    pub fn width(&self) -> Result<usize> {
        let (_, size) = format_of(self.format).ok_or(Error::Invalid {
            what: "vertex format",
            detail: format!("unknown format code {}", self.format),
        })?;
        Ok(size * self.dimension as usize)
    }

    /// End offset inside the stream.
    pub fn end(&self) -> Result<usize> {
        Ok(self.offset as usize + self.width()?)
    }
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}

/// Per-stream strides, computed from the channel table.
///
/// An unused stream keeps stride 0 and consumes no space. The returned vector is
/// one longer than the highest used stream index.
pub fn strides(channels: &[Channel; 14]) -> Result<Vec<usize>> {
    let mut out: Vec<usize> = Vec::new();
    for ch in channels {
        if !ch.is_used() {
            continue;
        }
        let s = ch.stream as usize;
        let end = ch.end()?;
        if s >= out.len() {
            out.resize(s + 1, 0);
        }
        out[s] = out[s].max(end);
    }
    Ok(out)
}

/// Byte offset of each stream's first vertex within `m_DataSize`.
pub fn stream_starts(strides: &[usize], vertex_count: usize) -> Vec<usize> {
    let mut starts = Vec::with_capacity(strides.len());
    let mut cursor = 0usize;
    for &stride in strides {
        cursor = align16(cursor);
        starts.push(cursor);
        cursor += stride * vertex_count;
    }
    starts
}

/// Total bytes the layout implies, including inter-stream padding.
pub fn layout_size(strides: &[usize], vertex_count: usize) -> usize {
    let starts = stream_starts(strides, vertex_count);
    match (starts.last(), strides.last()) {
        (Some(&s), Some(&w)) => s + w * vertex_count,
        _ => 0,
    }
}

fn read_scalar(data: &[u8], pos: usize, fmt: Fmt) -> Result<f32> {
    let need = match fmt {
        Fmt::F32 | Fmt::U32 | Fmt::I32 => 4,
        Fmt::F16 | Fmt::U16 | Fmt::I16 => 2,
        Fmt::U8 | Fmt::I8 => 1,
    };
    if pos + need > data.len() {
        return Err(Error::Truncated {
            what: "vertex attribute",
            offset: pos,
            wanted: need,
            available: data.len().saturating_sub(pos),
        });
    }
    Ok(match fmt {
        Fmt::F32 => f32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]),
        Fmt::F16 => f16_to_f32(u16::from_le_bytes([data[pos], data[pos + 1]])),
        Fmt::U8 => data[pos] as f32,
        Fmt::I8 => data[pos] as i8 as f32,
        Fmt::U16 => u16::from_le_bytes([data[pos], data[pos + 1]]) as f32,
        Fmt::I16 => i16::from_le_bytes([data[pos], data[pos + 1]]) as f32,
        Fmt::U32 => u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as f32,
        Fmt::I32 => i32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as f32,
    })
}

fn write_scalar(out: &mut [u8], pos: usize, fmt: Fmt, value: f32, normaliser: f32) {
    let v = if normaliser != 1.0 {
        (value * normaliser).round().clamp(0.0, normaliser)
    } else {
        value
    };
    match fmt {
        Fmt::F32 => out[pos..pos + 4].copy_from_slice(&v.to_le_bytes()),
        Fmt::F16 => out[pos..pos + 2].copy_from_slice(&f32_to_f16(v).to_le_bytes()),
        Fmt::U8 => out[pos] = v as u8,
        Fmt::I8 => out[pos] = (v as i8) as u8,
        Fmt::U16 => out[pos..pos + 2].copy_from_slice(&(v as u16).to_le_bytes()),
        Fmt::I16 => out[pos..pos + 2].copy_from_slice(&(v as i16).to_le_bytes()),
        Fmt::U32 => out[pos..pos + 4].copy_from_slice(&(v as u32).to_le_bytes()),
        Fmt::I32 => out[pos..pos + 4].copy_from_slice(&(v as i32).to_le_bytes()),
    }
}

/// IEEE 754 binary16 to f32. Exact: every f16 value is representable in f32.
pub fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0f32 } else { 1.0 };
    let exp = ((bits >> 10) & 0x1F) as i32;
    let mantissa = (bits & 0x3FF) as f32;
    let value = match exp {
        0 => mantissa * 2f32.powi(-24),
        31 => {
            if mantissa == 0.0 {
                f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => (1.0 + mantissa / 1024.0) * 2f32.powi(exp - 15),
    };
    sign * value
}

/// f32 to IEEE 754 binary16, rounding to nearest and flushing subnormals.
pub fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32 - 127 + 15;
    let mantissa = bits & 0x7F_FFFF;

    if exp >= 31 {
        // Overflow or NaN/Inf.
        return sign | 0x7C00 | if mantissa != 0 { 0x200 } else { 0 };
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        // Subnormal: shift the implicit leading one into place.
        let m = mantissa | 0x80_0000;
        let shift = (14 - exp) as u32;
        let half = (m >> shift) as u16;
        let round = (m >> (shift - 1)) & 1;
        let sticky = (m & ((1 << (shift - 1)) - 1)) != 0;
        let mut out = half + if round == 1 && (sticky || half & 1 == 1) { 1 } else { 0 };
        if out >= 0x400 {
            return sign | 0x0400;
        }
        out &= 0x3FF;
        return sign | out;
    }

    let mut half_exp = exp as u16;
    let mut half_mant = (mantissa >> 13) as u16;
    let round = (mantissa >> 12) & 1;
    let sticky = (mantissa & 0xFFF) != 0;
    if round == 1 && (sticky || half_mant & 1 == 1) {
        half_mant += 1;
        if half_mant >= 0x400 {
            half_mant = 0;
            half_exp += 1;
            if half_exp >= 31 {
                return sign | 0x7C00;
            }
        }
    }
    sign | (half_exp << 10) | (half_mant & 0x3FF)
}

/// Decoded vertex attributes for one mesh.
#[derive(Debug, Clone, Default)]
pub struct VertexStreams {
    /// Indexed by attribute slot; `None` when the channel is unused.
    pub attributes: Vec<Option<Vec<f32>>>,
    /// Raw `m_DataSize` bytes, kept so a rebuild can preserve padding bytes
    /// that this decoder does not own.
    pub raw: Vec<u8>,
}

/// Decode `m_DataSize` into per-attribute value arrays.
pub fn decode_vertices(data: &[u8], vertex_count: usize, channels: &[Channel; 14]) -> Result<VertexStreams> {
    let strides = strides(channels)?;
    let starts = stream_starts(&strides, vertex_count);

    let mut attributes: Vec<Option<Vec<f32>>> = vec![None; 14];
    for (attr, ch) in channels.iter().enumerate() {
        if !ch.is_used() {
            continue;
        }
        let (fmt, size) = format_of(ch.format).ok_or(Error::Invalid {
            what: "vertex format",
            detail: format!("unknown format code {} at attribute {}", ch.format, attr),
        })?;
        let stream = ch.stream as usize;
        if stream >= starts.len() {
            return Err(Error::Invalid {
                what: "vertex stream",
                detail: format!("attribute {attr} names stream {stream}"),
            });
        }
        let stride = strides[stream];
        let base = starts[stream] + ch.offset as usize;
        let dim = ch.dimension as usize;
        let norm = normaliser(ch.format);

        if stride * vertex_count > data.len().saturating_sub(starts[stream]) {
            return Err(Error::Truncated {
                what: "vertex stream",
                offset: starts[stream],
                wanted: stride * vertex_count,
                available: data.len().saturating_sub(starts[stream]),
            });
        }

        let mut values = Vec::with_capacity(vertex_count * dim);
        for v in 0..vertex_count {
            let row = base + v * stride;
            for k in 0..dim {
                let raw = read_scalar(data, row + k * size, fmt)?;
                values.push(if norm != 1.0 { raw / norm } else { raw });
            }
        }
        attributes[attr] = Some(values);
    }

    Ok(VertexStreams { attributes, raw: data.to_vec() })
}

/// Re-pack attribute values into the layout the channels describe.
///
/// Bytes this decoder does not own (padding, and any stream region no channel
/// covers) are taken from `original` when it is long enough, so an unchanged
/// mesh round-trips exactly rather than being zero-filled.
pub fn encode_vertices(
    attributes: &[Option<Vec<f32>>],
    vertex_count: usize,
    channels: &[Channel; 14],
    original: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let strides = strides(channels)?;
    let starts = stream_starts(&strides, vertex_count);
    let total = layout_size(&strides, vertex_count);

    let mut out = vec![0u8; total];
    if let Some(orig) = original {
        let n = orig.len().min(total);
        out[..n].copy_from_slice(&orig[..n]);
    }

    for (attr, ch) in channels.iter().enumerate() {
        if !ch.is_used() {
            continue;
        }
        let values = match attributes.get(attr).and_then(|a| a.as_ref()) {
            Some(v) => v,
            None => continue,
        };
        let (fmt, size) = format_of(ch.format).ok_or(Error::Invalid {
            what: "vertex format",
            detail: format!("unknown format code {}", ch.format),
        })?;
        let dim = ch.dimension as usize;
        if values.len() != vertex_count * dim {
            return Err(Error::Invalid {
                what: "attribute length",
                detail: format!(
                    "attribute {attr} has {} values, expected {}",
                    values.len(),
                    vertex_count * dim
                ),
            });
        }
        let stream = ch.stream as usize;
        let stride = strides[stream];
        let base = starts[stream] + ch.offset as usize;
        let norm = normaliser(ch.format);
        for v in 0..vertex_count {
            let row = base + v * stride;
            for k in 0..dim {
                write_scalar(&mut out, row + k * size, fmt, values[v * dim + k], norm);
            }
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Bridge to the type-tree value layer
// ---------------------------------------------------------------------------

/// A mesh pulled out of its decoded type tree.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub vertex_count: usize,
    pub channels: [Channel; 14],
    pub streams: VertexStreams,
    pub indices: Vec<u32>,
    pub index_format: i32,
    pub submesh_index_counts: Vec<u32>,
    pub bindpose: Vec<f32>,
    pub bone_hashes: Vec<u32>,
    pub aabb_center: [f32; 3],
    pub aabb_extent: [f32; 3],
}

/// Store an integer back into a slot, keeping the integer type the tree declares.
///
/// Writing I32 into an `unsigned int` field is rejected by the writer, correctly
/// -- the type tree is the contract, not the caller's convenience.
fn set_int(slot: &mut Value, v: i64) {
    *slot = match slot {
        Value::U8(_) => Value::U8(v as u8),
        Value::I8(_) => Value::I8(v as i8),
        Value::U16(_) => Value::U16(v as u16),
        Value::I16(_) => Value::I16(v as i16),
        Value::U32(_) => Value::U32(v as u32),
        Value::U64(_) => Value::U64(v as u64),
        Value::I64(_) => Value::I64(v),
        _ => Value::I32(v as i32),
    };
}

fn get_i32(tree: &Value, name: &str) -> Result<i32> {
    tree.get(name).and_then(|v| v.as_i32()).ok_or(Error::Invalid {
        what: "mesh field",
        detail: format!("missing or non-integer {name}"),
    })
}

/// Read a byte blob that the type tree may declare either as `TypelessData`
/// (decoded to `Bytes`) or as a `vector<UInt8>` (decoded to an array).
fn byte_blob(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::Bytes(b) => Some(b.clone()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::U8(b) => out.push(*b),
                    Value::I8(b) => out.push(*b as u8),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Store a byte blob back into a slot, keeping whichever shape it had.
fn set_byte_blob(slot: &mut Value, bytes: &[u8]) -> bool {
    match slot {
        Value::Bytes(b) => {
            *b = bytes.to_vec();
            true
        }
        Value::Array(items) => {
            *items = bytes.iter().map(|b| Value::U8(*b)).collect();
            true
        }
        _ => false,
    }
}

fn get_bytes_owned(tree: &Value, name: &str) -> Result<Vec<u8>> {
    let value = tree.get(name).ok_or(Error::Invalid {
        what: "mesh field",
        detail: format!("missing {name}"),
    })?;
    byte_blob(value).ok_or(Error::Invalid {
        what: "mesh field",
        detail: format!("{name} is neither bytes nor a byte array"),
    })
}

fn vec3(tree: &Value, name: &str) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    if let Some(v) = tree.get(name) {
        for (i, axis) in ["x", "y", "z"].iter().enumerate() {
            out[i] = match v.get(axis) {
                Some(Value::F32(f)) => *f,
                _ => 0.0,
            };
        }
    }
    out
}

/// Extract a [`Mesh`] from a decoded Mesh object.
pub fn from_value(tree: &Value) -> Result<Mesh> {
    let vd = tree.get("m_VertexData").ok_or(Error::Invalid {
        what: "mesh",
        detail: "missing m_VertexData".into(),
    })?;
    let vertex_count = get_i32(vd, "m_VertexCount")? as usize;

    let mut channels = [Channel::UNUSED; 14];
    if let Some(list) = vd.get("m_Channels").and_then(|v| v.as_array()) {
        for (i, ch) in list.iter().enumerate().take(14) {
            channels[i] = Channel {
                stream: ch.get("stream").and_then(|v| v.as_i32()).unwrap_or(0) as u32,
                offset: ch.get("offset").and_then(|v| v.as_i32()).unwrap_or(0) as u32,
                format: ch.get("format").and_then(|v| v.as_i32()).unwrap_or(0) as u32,
                dimension: ch.get("dimension").and_then(|v| v.as_i32()).unwrap_or(0) as u32,
            };
        }
    }

    let data = get_bytes_owned(vd, "m_DataSize")?;
    let streams = decode_vertices(&data, vertex_count, &channels)?;

    let index_format = get_i32(tree, "m_IndexFormat")?;
    let ib = get_bytes_owned(tree, "m_IndexBuffer")?;
    let indices = if index_format == 0 {
        ib.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]) as u32)
            .collect()
    } else {
        ib.chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };

    let submesh_index_counts = tree
        .get("m_SubMeshes")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .map(|s| s.get("indexCount").and_then(|v| v.as_i32()).unwrap_or(0) as u32)
                .collect()
        })
        .unwrap_or_default();

    let mut bindpose = Vec::new();
    if let Some(list) = tree.get("m_BindPose").and_then(|v| v.as_array()) {
        for m in list {
            for r in 0..4 {
                for c in 0..4 {
                    let key = format!("e{r}{c}");
                    bindpose.push(match m.get(&key) {
                        Some(Value::F32(f)) => *f,
                        _ => 0.0,
                    });
                }
            }
        }
    }

    let bone_hashes = tree
        .get("m_BoneNameHashes")
        .and_then(|v| v.as_array())
        .map(|list| list.iter().filter_map(|v| v.as_i32()).map(|v| v as u32).collect())
        .unwrap_or_default();

    let aabb = tree.get("m_LocalAABB");
    let (aabb_center, aabb_extent) = match aabb {
        Some(a) => (
            vec3(a, "m_Center"),
            vec3(a, "m_Extent"),
        ),
        None => ([0.0; 3], [0.0; 3]),
    };

    let name = tree.get("m_Name").and_then(|v| v.as_str()).unwrap_or_default().to_string();

    Ok(Mesh {
        name,
        vertex_count,
        channels,
        streams,
        indices,
        index_format,
        submesh_index_counts,
        bindpose,
        bone_hashes,
        aabb_center,
        aabb_extent,
    })
}

/// Write a [`Mesh`] back into its type tree (mutates `tree`).
///
/// Only the fields the vertex/index work owns are touched, so unrelated fields
/// such as blend shapes or compression flags survive untouched.
pub fn to_value(tree: &mut Value, mesh: &Mesh) -> Result<()> {
    let channels = mesh.channels;
    let original = if mesh.streams.raw.len() == layout_size(&strides(&channels)?, mesh.vertex_count) {
        Some(mesh.streams.raw.as_slice())
    } else {
        None
    };
    let data = encode_vertices(&mesh.streams.attributes, mesh.vertex_count, &channels, original)?;

    let vd = tree.get_mut("m_VertexData").ok_or(Error::Invalid {
        what: "mesh",
        detail: "missing m_VertexData".into(),
    })?;
    {
        let slot = vd.get_mut("m_VertexCount").ok_or(Error::Invalid {
            what: "mesh",
            detail: "missing m_VertexCount".into(),
        })?;
        set_int(slot, mesh.vertex_count as i64);
    }
    {
        let slot = vd.get_mut("m_DataSize").ok_or(Error::Invalid {
            what: "mesh",
            detail: "missing m_DataSize".into(),
        })?;
        if !set_byte_blob(slot, &data) {
            return Err(Error::Invalid {
                what: "m_DataSize",
                detail: "neither bytes nor a byte array".into(),
            });
        }
    }

    let index_bytes: Vec<u8> = if mesh.index_format == 0 {
        mesh.indices.iter().flat_map(|v| (*v as u16).to_le_bytes()).collect()
    } else {
        mesh.indices.iter().flat_map(|v| v.to_le_bytes()).collect()
    };
    if let Some(slot) = tree.get_mut("m_IndexBuffer") {
        if !set_byte_blob(slot, &index_bytes) {
            return Err(Error::Invalid {
                what: "m_IndexBuffer",
                detail: "neither bytes nor a byte array".into(),
            });
        }
    }
    if let Some(slot) = tree.get_mut("m_IndexFormat") {
        set_int(slot, mesh.index_format as i64);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_round_trips_known_values() {
        for v in [0.0f32, 1.0, -1.0, 0.5, 2.0, 1.0 / 3.0, 65504.0, -0.0] {
            let half = f32_to_f16(v);
            let back = f16_to_f32(half);
            assert!(
                (back - v).abs() <= v.abs() * 0.001 || (v == 0.0 && back == 0.0),
                "{v} -> {half:#06x} -> {back}"
            );
        }
    }

    #[test]
    fn f16_exact_for_representable_values() {
        // These are exactly representable in binary16.
        for v in [0.0f32, 1.0, 0.5, 0.25, 2.0, -4.0, 0.125] {
            assert_eq!(f16_to_f32(f32_to_f16(v)), v);
        }
    }

    #[test]
    fn strides_match_real_layout() {
        // position f32x3 @0, normal f32x3 @12, tangent f32x4 @24 -> 40
        // color f32x4 @0, uv0 f16x2 @16 -> 20
        // weights f32x4 @0, bones u32x4 @16 -> 32
        let mut ch = [Channel::UNUSED; 14];
        ch[0] = Channel { stream: 0, offset: 0, format: 0, dimension: 3 };
        ch[1] = Channel { stream: 0, offset: 12, format: 0, dimension: 3 };
        ch[2] = Channel { stream: 0, offset: 24, format: 0, dimension: 4 };
        ch[3] = Channel { stream: 1, offset: 0, format: 0, dimension: 4 };
        ch[4] = Channel { stream: 1, offset: 16, format: 1, dimension: 2 };
        ch[12] = Channel { stream: 2, offset: 0, format: 0, dimension: 4 };
        ch[13] = Channel { stream: 2, offset: 16, format: 10, dimension: 4 };

        let s = strides(&ch).unwrap();
        assert_eq!(s, vec![40, 20, 32]);
        // 2283 verts: 91320 + pad 8 + 45660 + pad 4 + 73056 = 210048
        assert_eq!(layout_size(&s, 2283), 210_048);
    }

    #[test]
    fn stream_starts_are_sixteen_aligned() {
        let strides = vec![40, 20, 32];
        let starts = stream_starts(&strides, 2283);
        assert_eq!(starts[0], 0);
        assert_eq!(starts[1], 91328); // 91320 aligned up to 16
        assert_eq!(starts[2], 136992);
        for s in starts {
            assert_eq!(s % 16, 0);
        }
    }

    #[test]
    fn unused_channels_consume_nothing() {
        let ch = [Channel::UNUSED; 14];
        let s = strides(&ch).unwrap();
        assert!(s.is_empty());
        assert_eq!(layout_size(&s, 100), 0);
    }

    #[test]
    fn decode_then_encode_is_identical() {
        let mut ch = [Channel::UNUSED; 14];
        ch[0] = Channel { stream: 0, offset: 0, format: 0, dimension: 3 };
        ch[12] = Channel { stream: 1, offset: 0, format: 0, dimension: 4 };
        ch[13] = Channel { stream: 1, offset: 16, format: 10, dimension: 4 };

        let vertices = 3usize;
        let s = strides(&ch).unwrap();
        let total = layout_size(&s, vertices);
        let starts = stream_starts(&s, vertices);

        // Build plausible contents: finite floats and small bone indices, with
        // arbitrary bytes in the padding and in the gap between streams. Random
        // u32 bone indices would exceed 2^24 and legitimately lose precision in
        // the f32 value model.
        let mut data = vec![0xABu8; total];
        for v in 0..vertices {
            let base = starts[0] + v * s[0];
            for k in 0..3 {
                let x = (v * 3 + k) as f32 * 0.25 - 1.0;
                data[base + k * 4..base + k * 4 + 4].copy_from_slice(&x.to_le_bytes());
            }
            let base = starts[1] + v * s[1];
            for k in 0..4 {
                let w = (v + k) as f32 * 0.125;
                data[base + k * 4..base + k * 4 + 4].copy_from_slice(&w.to_le_bytes());
            }
            for k in 0..4 {
                let bone = (v * 10 + k) as u32;
                data[base + 16 + k * 4..base + 20 + k * 4].copy_from_slice(&bone.to_le_bytes());
            }
        }

        let streams = decode_vertices(&data, vertices, &ch).unwrap();
        assert_eq!(streams.attributes[0].as_ref().unwrap().len(), vertices * 3);
        // Stream 1 holds weights at 0 and bones at 16, so its stride is 32.
        assert_eq!(s[1], 32);
        // bones is [0,1,2,3, 10,11,12,13, 20,21,22,23].
        assert_eq!(streams.attributes[13].as_ref().unwrap()[4], 10.0);
        assert_eq!(streams.attributes[13].as_ref().unwrap()[11], 23.0);
        let back = encode_vertices(&streams.attributes, vertices, &ch, Some(&data)).unwrap();
        assert_eq!(back, data, "unowned padding must be preserved");
    }

    #[test]
    fn integer_attributes_keep_exact_values() {
        let mut ch = [Channel::UNUSED; 14];
        ch[13] = Channel { stream: 0, offset: 0, format: 10, dimension: 4 };
        let mut data = Vec::new();
        for v in 0u32..3 {
            for k in 0..4u32 {
                data.extend_from_slice(&(v * 100 + k).to_le_bytes());
            }
        }
        let streams = decode_vertices(&data, 3, &ch).unwrap();
        let bones = streams.attributes[13].as_ref().unwrap();
        assert_eq!(bones[0], 0.0);
        assert_eq!(bones[3], 3.0);
        assert_eq!(bones[4], 100.0);
        assert_eq!(bones[11], 203.0);
    }

    #[test]
    fn normalised_formats_scale_on_read() {
        let mut ch = [Channel::UNUSED; 14];
        ch[3] = Channel { stream: 0, offset: 0, format: 2, dimension: 4 }; // UNorm8
        let data = [0u8, 128, 255, 64];
        let streams = decode_vertices(&data, 1, &ch).unwrap();
        let colors = streams.attributes[3].as_ref().unwrap();
        assert_eq!(colors[0], 0.0);
        assert!((colors[1] - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(colors[2], 1.0);
    }

    #[test]
    fn short_stream_is_rejected() {
        let mut ch = [Channel::UNUSED; 14];
        ch[0] = Channel { stream: 0, offset: 0, format: 0, dimension: 3 };
        let data = vec![0u8; 11]; // one vertex needs 12
        assert!(decode_vertices(&data, 1, &ch).is_err());
    }
}
