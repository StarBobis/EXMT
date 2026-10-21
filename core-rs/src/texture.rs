//! Texture2D (class 28): metadata, streaming payloads, pixel decode and replace.
//!
//! The semantics are ported from the Python core (`exmt_core/ddsinfo.py` and
//! `exmt_core/textures.py`), including two conventions that are easy to get
//! wrong and are pinned by tests:
//!
//! * **Rows are stored bottom-up.** Decoding flips them, and replacing a
//!   texture with a top-down encoder output (texconv) flips them back, block
//!   row by block row, for every mip level.
//! * **The block decoders emit BGRA** (the `texture2ddecoder` convention), so
//!   an upright RGBA image needs a channel swap as well as the flip.
//!
//! A texture's pixels live in one of two places: inline in the object as
//! `image data` (the field really has a space in it), or in a sibling stream
//! file (`.resS`) described by `m_StreamData` (path/offset/size). Bundle nodes
//! are matched by the path's final component because Unity writes paths like
//! `archive:/CAB-0123/CAB-0123.resS` while the node is called `CAB-0123.resS`.

use std::borrow::Cow;

use crate::bc::{self, Bc};
use crate::container::Node;
use crate::error::{Error, Result};
use crate::typetree::Value;

/// Where the pixels sit and how many bytes one unit occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// 4x4 block compression: bytes per block.
    Block(usize),
    /// Uncompressed: bytes per pixel.
    Pixel(usize),
}

/// A Unity `TextureFormat` this crate understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub name: &'static str,
    pub layout: Layout,
    /// Set when the format is block-compressed *and* this crate can decode it.
    pub bc: Option<Bc>,
}

/// Unity TextureFormat ids, ported from `ddsinfo.FORMATS`.
pub fn format(code: i32) -> Option<Format> {
    let (name, layout, bc) = match code {
        1 => ("Alpha8", Layout::Pixel(1), None),
        3 => ("RGB24", Layout::Pixel(4), None),
        4 => ("RGBA32", Layout::Pixel(4), None),
        5 => ("ARGB32", Layout::Pixel(4), None),
        10 => ("DXT1", Layout::Block(8), Some(Bc::Bc1)),
        12 => ("DXT5", Layout::Block(16), Some(Bc::Bc3)),
        17 => ("RGBAHalf", Layout::Pixel(8), None),
        20 => ("RGBAFloat", Layout::Pixel(16), None),
        24 => ("BC6H", Layout::Block(16), None),
        25 => ("BC7", Layout::Block(16), Some(Bc::Bc7)),
        26 => ("BC4", Layout::Block(8), Some(Bc::Bc4)),
        27 => ("BC5", Layout::Block(16), Some(Bc::Bc5)),
        63 => ("R8", Layout::Pixel(1), None),
        _ => return None,
    };
    Some(Format { name, layout, bc })
}

/// Mip dimensions, level 0 first, each dimension floored at 1.
pub fn mip_sizes(width: usize, height: usize, count: usize) -> Vec<(usize, usize)> {
    (0..count.max(1))
        .map(|i| ((width >> i).max(1), (height >> i).max(1)))
        .collect()
}

/// Bytes a full mip chain of this format occupies.
pub fn expected_bytes(code: i32, width: usize, height: usize, mips: usize) -> Result<usize> {
    let format = format(code).ok_or(Error::Unsupported {
        what: "texture format",
    })?;
    // A zero-sized texture owns no pixels. Floors of 1 in `mip_sizes` are for
    // the tail of a real chain, not for an empty asset.
    if width == 0 || height == 0 {
        return Ok(0);
    }
    let mut total = 0usize;
    for (w, h) in mip_sizes(width, height, mips) {
        total += match format.layout {
            Layout::Pixel(bytes) => w * h * bytes,
            Layout::Block(bytes) => w.div_ceil(4) * h.div_ceil(4) * bytes,
        };
    }
    Ok(total)
}

/// The out-of-line pixel stream a Texture2D points into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamData {
    pub path: String,
    pub offset: u64,
    pub size: u32,
}

/// A Texture2D pulled out of its decoded type tree.
#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub format: i32,
    pub mip_count: usize,
    /// `m_CompleteImageSize` as declared by the asset (not trusted for slicing).
    pub image_size: usize,
    pub stream: Option<StreamData>,
    inline: Vec<u8>,
}

fn get_i32(tree: &Value, name: &str) -> Result<i32> {
    tree.get(name)
        .and_then(|v| v.as_i32())
        .ok_or(Error::Invalid {
            what: "texture field",
            detail: format!("missing or non-integer {name}"),
        })
}

/// The pixel blob field is `image data` in Unity's type trees. The other two
/// spellings show up in third-party serializer dumps; accept them on read but
/// always write back to the field that exists.
fn image_field(tree: &Value) -> Option<&Value> {
    ["image data", "m_ImageData", "image_data"]
        .iter()
        .find_map(|name| tree.get(name))
}

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

impl Texture {
    pub fn from_value(tree: &Value) -> Result<Texture> {
        let name = tree
            .get("m_Name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let width = get_i32(tree, "m_Width")?;
        let height = get_i32(tree, "m_Height")?;
        // Zero-sized textures exist in shipped files (render targets, disabled
        // assets). They parse, but decoding them is an error rather than an
        // empty image.
        if width < 0 || height < 0 {
            return Err(Error::Invalid {
                what: "texture size",
                detail: format!("{width}x{height}"),
            });
        }
        let format = get_i32(tree, "m_TextureFormat")?;
        let mip_count = tree
            .get("m_MipCount")
            .and_then(|v| v.as_i32())
            .filter(|v| *v > 0)
            .unwrap_or(1) as usize;
        let image_size = tree
            .get("m_CompleteImageSize")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let mut stream = None;
        if let Some(sd) = tree.get("m_StreamData") {
            let path = sd
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !path.is_empty() {
                let offset = sd
                    .get("offset")
                    .and_then(|v| v.as_u64())
                    .ok_or(Error::Invalid {
                        what: "m_StreamData.offset",
                        detail: "missing or non-integer".into(),
                    })?;
                let size = sd.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                if size > u32::MAX as u64 {
                    return Err(Error::Implausible {
                        what: "m_StreamData.size",
                        value: size,
                    });
                }
                stream = Some(StreamData {
                    path,
                    offset,
                    size: size as u32,
                });
            }
        }

        let inline = image_field(tree).and_then(byte_blob).unwrap_or_default();

        Ok(Texture {
            name,
            width: width as usize,
            height: height as usize,
            format,
            mip_count,
            image_size,
            stream,
            inline,
        })
    }

    /// Bytes the asset declares for the whole chain.
    pub fn expected_bytes(&self) -> Result<usize> {
        expected_bytes(self.format, self.width, self.height, self.mip_count)
    }

    /// The compressed pixel payload, either from a bundle node or inline.
    ///
    /// The returned slice is borrowed from `nodes` when streamed.
    pub fn payload<'a>(&'a self, nodes: &'a [Node]) -> Result<Cow<'a, [u8]>> {
        if let Some(stream) = &self.stream {
            let data = find_stream(nodes, &stream.path).ok_or_else(|| Error::Invalid {
                what: "texture stream",
                detail: format!("no bundle node for {}", stream.path),
            })?;
            let start = stream.offset as usize;
            let end = start.saturating_add(stream.size as usize);
            if end > data.len() {
                return Err(Error::Truncated {
                    what: "texture stream",
                    offset: start,
                    wanted: stream.size as usize,
                    available: data.len().saturating_sub(start),
                });
            }
            Ok(Cow::Borrowed(&data[start..end]))
        } else {
            Ok(Cow::Borrowed(&self.inline))
        }
    }

    /// Decode mip 0 to upright RGBA8 (`width * height * 4`).
    pub fn decode_rgba(&self, nodes: &[Node]) -> Result<Vec<u8>> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::Invalid {
                what: "texture size",
                detail: format!("{}x{} has no pixels", self.width, self.height),
            });
        }
        let payload = self.payload(nodes)?;
        let format = format(self.format).ok_or(Error::Unsupported {
            what: "texture format",
        })?;
        let mut raw = match format.bc {
            Some(bc) => {
                let need = bc::image_bytes(bc, self.width, self.height);
                if payload.len() < need {
                    return Err(Error::Truncated {
                        what: "texture mip 0",
                        offset: 0,
                        wanted: need,
                        available: payload.len(),
                    });
                }
                bc::decode(bc, &payload[..need], self.width, self.height)?
            }
            None if self.format == 4 => {
                // RGBA32: the Python core treats these bytes as BGRA too, and
                // this port follows it rather than the format's name.
                let need = self.width * self.height * 4;
                if payload.len() < need {
                    return Err(Error::Truncated {
                        what: "texture mip 0",
                        offset: 0,
                        wanted: need,
                        available: payload.len(),
                    });
                }
                payload[..need].to_vec()
            }
            None => {
                return Err(Error::Unsupported {
                    what: "texture decoder",
                })
            }
        };
        upright(&mut raw, self.width, self.height);
        Ok(raw)
    }
}

/// Rewrite BGRA rows stored bottom-up into upright RGBA.
pub fn upright(data: &mut [u8], width: usize, height: usize) {
    let row = width * 4;
    debug_assert_eq!(data.len(), row * height);
    for y in 0..height / 2 {
        let top = y * row;
        let bottom = (height - 1 - y) * row;
        for i in 0..row {
            data.swap(top + i, bottom + i);
        }
    }
    // Channel swap in place: BGRA -> RGBA.
    for px in data.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

/// Flip every mip level vertically, block row by block row.
///
/// This is what turns a top-down encoder result (`texconv` writes DDS that way)
/// into Unity's bottom-up storage.
pub fn flip_vertical(
    payload: &[u8],
    code: i32,
    width: usize,
    height: usize,
    mips: usize,
) -> Result<Vec<u8>> {
    let format = format(code).ok_or(Error::Unsupported {
        what: "texture format",
    })?;
    let mut out = Vec::with_capacity(payload.len());
    let mut at = 0usize;
    for (w, h) in mip_sizes(width, height, mips) {
        let (row_bytes, rows) = match format.layout {
            Layout::Pixel(bytes) => (w * bytes, h),
            Layout::Block(bytes) => (w.div_ceil(4) * bytes, h.div_ceil(4)),
        };
        let level = row_bytes * rows;
        let slice = payload.get(at..at + level).ok_or(Error::Truncated {
            what: "texture mip chain",
            offset: at,
            wanted: level,
            available: payload.len().saturating_sub(at),
        })?;
        for y in (0..rows).rev() {
            out.extend_from_slice(&slice[y * row_bytes..(y + 1) * row_bytes]);
        }
        at += level;
    }
    if at != payload.len() {
        return Err(Error::SizeMismatch {
            what: "texture mip chain",
            got: payload.len(),
            want: at,
        });
    }
    Ok(out)
}

fn set_int(slot: &mut Value, v: u64) {
    *slot = match slot {
        Value::U8(_) => Value::U8(v as u8),
        Value::I8(_) => Value::I8(v as i8),
        Value::U16(_) => Value::U16(v as u16),
        Value::I16(_) => Value::I16(v as i16),
        Value::U32(_) => Value::U32(v as u32),
        Value::U64(_) => Value::U64(v),
        Value::I64(_) => Value::I64(v as i64),
        _ => Value::I32(v as i32),
    };
}

/// Replace a texture's pixels with top-down encoded blocks.
///
/// The asset is inlined: `m_StreamData` is cleared, because the bundle stream
/// layout is shared by every texture in the node and resizing it is a rebuild
/// concern, not a texture concern. Returns the updated description.
pub fn store_image(tree: &mut Value, top_down: &[u8]) -> Result<Texture> {
    let texture = Texture::from_value(tree)?;
    let want = texture.expected_bytes()?;
    if top_down.len() != want {
        return Err(Error::SizeMismatch {
            what: "texture payload",
            got: top_down.len(),
            want,
        });
    }
    let stored = flip_vertical(
        top_down,
        texture.format,
        texture.width,
        texture.height,
        texture.mip_count,
    )?;

    let key = ["image data", "m_ImageData", "image_data"]
        .into_iter()
        .find(|k| tree.get(k).is_some())
        .ok_or(Error::Invalid {
            what: "texture",
            detail: "type tree has no `image data` field".into(),
        })?;
    match tree.get_mut(key) {
        Some(Value::Bytes(b)) => *b = stored.clone(),
        Some(Value::Array(items)) => *items = stored.iter().copied().map(Value::U8).collect(),
        _ => {
            return Err(Error::Invalid {
                what: "image data",
                detail: "neither bytes nor a byte array".into(),
            })
        }
    }

    if let Some(slot) = tree.get_mut("m_CompleteImageSize") {
        set_int(slot, top_down.len() as u64);
    }
    if let Some(slot) = tree.get_mut("m_MipCount") {
        set_int(slot, texture.mip_count as u64);
    }
    if let Some(sd) = tree.get_mut("m_StreamData") {
        if let Some(Value::Str(path)) = sd.get_mut("path") {
            path.clear();
        }
        if let Some(slot) = sd.get_mut("offset") {
            set_int(slot, 0);
        }
        if let Some(slot) = sd.get_mut("size") {
            set_int(slot, 0);
        }
    }

    let mut updated = texture;
    updated.stream = None;
    updated.inline = stored;
    updated.image_size = top_down.len();
    Ok(updated)
}

/// Last path component, so `archive:/CAB-1/CAB-1.resS` matches `CAB-1.resS`.
fn base(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Find the bundle node that holds a texture stream.
pub fn find_stream<'a>(nodes: &'a [Node], stream_path: &str) -> Option<&'a [u8]> {
    let want = base(stream_path);
    nodes
        .iter()
        .find(|node| base(&node.path) == want)
        .map(|node| node.data.as_slice())
        .or_else(|| {
            // Unity player bundles name the stream after their only cabin, but
            // stored paths occasionally carry an editor prefix that does not
            // survive the pack. A two-node bundle has exactly one stream node.
            if nodes.len() == 2 && want.ends_with(".resS") {
                Some(nodes[1].data.as_slice())
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 8] = [0x00, 0xF8, 0x00, 0xF8, 0x00, 0x00, 0x00, 0x00];
    const BLUE: [u8; 8] = [0x1F, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x00, 0x00];

    fn s(name: &str, value: Value) -> (String, Value) {
        (name.to_string(), value)
    }

    fn tree(
        width: i32,
        height: i32,
        format: i32,
        mips: i32,
        image: &[u8],
        stream: Option<(&str, u64, u32)>,
    ) -> Value {
        let sd = match stream {
            Some((path, offset, size)) => Value::Struct(vec![
                s("offset", Value::U64(offset)),
                s("size", Value::U32(size)),
                s("path", Value::Str(path.to_string())),
            ]),
            None => Value::Struct(vec![
                s("offset", Value::U32(0)),
                s("size", Value::U32(0)),
                s("path", Value::Str(String::new())),
            ]),
        };
        Value::Struct(vec![
            s("m_Name", Value::Str("test_tex".into())),
            s("m_Width", Value::I32(width)),
            s("m_Height", Value::I32(height)),
            s("m_CompleteImageSize", Value::I32(image.len() as i32)),
            s("m_TextureFormat", Value::I32(format)),
            s("m_MipCount", Value::I32(mips)),
            s("image data", Value::Bytes(image.to_vec())),
            s("m_StreamData", sd),
        ])
    }

    fn node(path: &str, data: Vec<u8>) -> Node {
        Node {
            path: path.to_string(),
            data,
            flags: 0,
        }
    }

    #[test]
    fn mip_chain_matches_the_python_core() {
        // Real GF2 diffuse: 2048² DXT1 with 12 levels, 2,796,216 bytes.
        assert_eq!(expected_bytes(10, 2048, 2048, 12).unwrap(), 2_796_216);
        // Same chain in BC7 is exactly twice as large.
        assert_eq!(expected_bytes(25, 2048, 2048, 12).unwrap(), 5_592_432);
        assert_eq!(expected_bytes(10, 4, 4, 3).unwrap(), 24);
        assert_eq!(expected_bytes(4, 4, 4, 3).unwrap(), 84);
        assert_eq!(mip_sizes(2048, 2048, 12).last(), Some(&(1, 1)));
        assert!(expected_bytes(999, 4, 4, 1).is_err());
    }

    #[test]
    fn parses_inline_texture_and_decodes_upright() {
        // Two block rows: DXT1 red first. Unity stores row 0 at the bottom, so
        // the decoded top row must be the second block (blue).
        let image = [&RED[..], &BLUE[..]].concat();
        let tree = tree(4, 8, 10, 1, &image, None);
        let tex = Texture::from_value(&tree).unwrap();
        assert_eq!(
            (tex.width, tex.height, tex.format, tex.mip_count),
            (4, 8, 10, 1)
        );
        assert!(tex.stream.is_none());
        let rgba = tex.decode_rgba(&[]).unwrap();
        assert_eq!(&rgba[0..4], &[0, 0, 255, 255], "top row should be blue");
        assert_eq!(&rgba[7 * 4 * 4..7 * 4 * 4 + 4], &[255, 0, 0, 255]);
    }

    #[test]
    fn resolves_a_streamed_texture_by_node_basename() {
        let mut res = vec![0xAA; 5];
        res.extend_from_slice(&RED);
        let nodes = vec![node("CAB-abc", vec![0; 32]), node("CAB-abc.resS", res)];
        let tree = tree(
            4,
            4,
            10,
            1,
            &[],
            Some(("archive:/CAB-abc/CAB-abc.resS", 5, 8)),
        );
        let tex = Texture::from_value(&tree).unwrap();
        let stream = tex.stream.as_ref().unwrap();
        assert_eq!(stream.offset, 5);
        assert_eq!(tex.payload(&nodes).unwrap().as_ref(), &RED);
        let rgba = tex.decode_rgba(&nodes).unwrap();
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);

        // Missing stream node is an error, not an empty texture.
        assert!(tex.decode_rgba(&nodes[..1]).is_err());
        let short = vec![node("CAB-abc", vec![]), node("CAB-abc.resS", vec![0; 4])];
        assert!(tex.payload(&short).is_err());
    }

    #[test]
    fn store_image_flips_back_and_clears_the_stream() {
        // Top-down encoder output: red block on top, blue below.
        let top_down = [&RED[..], &BLUE[..]].concat();
        let mut tree = tree(4, 8, 10, 1, &[], Some(("CAB-abc.resS", 5, 8)));
        let tex = store_image(&mut tree, &top_down).unwrap();
        assert!(tex.stream.is_none());
        assert_eq!(tex.image_size, 16);

        // Storage must now hold blue first (bottom row of the picture).
        let stored = image_field(&tree).and_then(byte_blob).unwrap();
        assert_eq!(&stored[0..8], &BLUE);
        assert_eq!(&stored[8..16], &RED);

        let sd = tree.get("m_StreamData").unwrap();
        assert_eq!(sd.get("path").and_then(|v| v.as_str()), Some(""));
        assert_eq!(sd.get("size").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(
            tree.get("m_CompleteImageSize").and_then(|v| v.as_i32()),
            Some(16)
        );

        // Decoding the rewritten asset reproduces the top-down input.
        let again = Texture::from_value(&tree).unwrap();
        let rgba = again.decode_rgba(&[]).unwrap();
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[7 * 4 * 4..7 * 4 * 4 + 4], &[0, 0, 255, 255]);
    }

    #[test]
    fn store_image_rejects_a_wrong_length() {
        let mut tree = tree(4, 4, 10, 1, &RED, None);
        assert!(store_image(&mut tree, &[0u8; 7]).is_err());
        assert!(store_image(&mut tree, &[0u8; 16]).is_err());
    }

    #[test]
    fn flip_vertical_handles_each_mip_and_both_layouts() {
        // RGBA32: 4x2 top level, 2x1 mip.
        let mut payload = Vec::new();
        for v in [10u8, 11, 12, 13] {
            payload.extend_from_slice(&[v, 0, 0, 255]);
        }
        for v in [20u8, 21, 22, 23] {
            payload.extend_from_slice(&[v, 0, 0, 255]);
        }
        for v in [30u8, 31] {
            payload.extend_from_slice(&[v, 0, 0, 255]);
        }
        let flipped = flip_vertical(&payload, 4, 4, 2, 2).unwrap();
        assert_eq!(&flipped[0..4], &[20, 0, 0, 255]);
        assert_eq!(&flipped[16..20], &[10, 0, 0, 255]);
        assert_eq!(&flipped[32..36], &[30, 0, 0, 255]);
        assert_eq!(&flipped[36..40], &[31, 0, 0, 255]);

        // DXT1: 8x8 top level (2x2 blocks), 4x4 mip (1 block).
        let mut chain = Vec::new();
        for i in 0..4u8 {
            chain.extend_from_slice(&[i, 0, i, 0, 0, 0, 0, 0]);
        }
        chain.extend_from_slice(&[0xFF, 0xFF, 0, 0, 0, 0, 0, 0]);
        let flipped = flip_vertical(&chain, 10, 8, 8, 2).unwrap();
        // Block row 0 (blocks 0,1) and row 1 (2,3) swap.
        assert_eq!(&flipped[0..2], &[2, 0]);
        assert_eq!(&flipped[8..10], &[3, 0]);
        assert_eq!(&flipped[16..18], &[0, 0]);
        assert_eq!(&flipped[24..26], &[1, 0]);
        assert_eq!(&flipped[32..33], &[0xFF]);
        assert!(flip_vertical(&chain, 10, 8, 8, 2).unwrap().len() == chain.len());
        assert!(flip_vertical(&chain[..31], 10, 8, 8, 2).is_err());
    }

    #[test]
    fn zero_sized_textures_parse_but_do_not_decode() {
        let tree = tree(0, 0, 10, 1, &[], None);
        let tex = Texture::from_value(&tree).unwrap();
        assert_eq!(tex.expected_bytes().unwrap(), 0);
        assert!(tex.decode_rgba(&[]).is_err());
    }

    #[test]
    fn unsupported_formats_are_reported_by_name() {
        let tree = tree(4, 4, 17, 1, &[0u8; 128], None);
        let tex = Texture::from_value(&tree).unwrap();
        assert!(tex.decode_rgba(&[]).is_err());
        assert_eq!(format(17).unwrap().name, "RGBAHalf");
        assert_eq!(format(10).unwrap().bc, Some(Bc::Bc1));
        assert!(format(1234).is_none());
    }

    #[test]
    fn upright_swaps_channels_and_flips_rows() {
        let mut data = vec![
            1, 2, 3, 4, 5, 6, 7, 8, // bottom row
            9, 10, 11, 12, 13, 14, 15, 16, // top row
        ];
        upright(&mut data, 2, 2);
        assert_eq!(&data[0..4], &[11, 10, 9, 12]);
        assert_eq!(&data[4..8], &[15, 14, 13, 16]);
        assert_eq!(&data[8..12], &[3, 2, 1, 4]);
    }
}
