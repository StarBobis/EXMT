//! SerializedFile reader -- the layer a UnityFS node with flag 4 carries.
//!
//! A SerializedFile is a big-endian header followed by a byte-ordered body:
//! a table of per-class type trees, a table of objects with their offset and
//! length, script types, externals, and (v20+) a reference-type table. Object
//! payloads live at `data_offset`.
//!
//! Byte order is the trap here: **only the fixed header fields are big-endian**.
//! `unity_version`, `platform`, the type table, the object table and every
//! payload follow the endianness the header declares. Reading the whole
//! metadata block as big-endian still parses, it just yields nonsense -- a real
//! file produced `type_count = 0x02000000` instead of 2.
//!
//! Other layout facts that are easy to get wrong, each found the hard way:
//!
//! * v21+ appends a `type_dependencies` int array **after** each type tree.
//!   Skipping it shifts every later type by four bytes.
//! * Type-tree string references use the top bit as a flag: clear means an
//!   offset into the type's own buffer, set means an index into Unity's
//!   built-in name table (see [`crate::commonstrings`]). Masking instead of
//!   branching reads built-in names as whatever bytes sit at that low offset.
//! * `byte_start` in the object table is **relative to `data_offset`**.
//! * Object payloads are not byte-adjacent; a 4-byte gap is normal.

use crate::commonstrings;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

impl Endian {
    pub fn as_str(self) -> &'static str {
        match self {
            Endian::Little => "little",
            Endian::Big => "big",
        }
    }
}

/// Cursor over a byte range with a switchable byte order.
pub(crate) struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub endian: Endian,
    pub limit: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], endian: Endian) -> Self {
        Self { data, pos: 0, endian, limit: data.len() }
    }

    pub fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.pos)
    }

    pub fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated {
            what,
            offset: self.pos,
            wanted: n,
            available: self.remaining(),
        })?;
        if end > self.limit {
            return Err(Error::Truncated {
                what,
                offset: self.pos,
                wanted: n,
                available: self.remaining(),
            });
        }
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn int<const N: usize>(&mut self, what: &'static str) -> Result<[u8; N]> {
        let b = self.take(N, what)?;
        let mut out = [0u8; N];
        out.copy_from_slice(b);
        Ok(out)
    }

    pub fn u8(&mut self, what: &'static str) -> Result<u8> {
        Ok(self.take(1, what)?[0])
    }

    pub fn u16(&mut self, what: &'static str) -> Result<u16> {
        let b = self.int::<2>(what)?;
        Ok(match self.endian {
            Endian::Little => u16::from_le_bytes(b),
            Endian::Big => u16::from_be_bytes(b),
        })
    }

    pub fn i16(&mut self, what: &'static str) -> Result<i16> {
        Ok(self.u16(what)? as i16)
    }

    pub fn u32(&mut self, what: &'static str) -> Result<u32> {
        let b = self.int::<4>(what)?;
        Ok(match self.endian {
            Endian::Little => u32::from_le_bytes(b),
            Endian::Big => u32::from_be_bytes(b),
        })
    }

    pub fn i32(&mut self, what: &'static str) -> Result<i32> {
        Ok(self.u32(what)? as i32)
    }

    pub fn u64(&mut self, what: &'static str) -> Result<u64> {
        let b = self.int::<8>(what)?;
        Ok(match self.endian {
            Endian::Little => u64::from_le_bytes(b),
            Endian::Big => u64::from_be_bytes(b),
        })
    }

    pub fn i64(&mut self, what: &'static str) -> Result<i64> {
        Ok(self.u64(what)? as i64)
    }

    /// Advance to the next multiple of `n`, measured from the buffer start.
    pub fn align(&mut self, n: usize) {
        let pad = (n - self.pos % n) % n;
        self.pos = (self.pos + pad).min(self.limit);
    }

    pub fn cstr(&mut self, what: &'static str) -> Result<String> {
        let rest = &self.data[self.pos..self.limit];
        let end = rest.iter().position(|&b| b == 0).ok_or(Error::Invalid {
            what,
            detail: format!("unterminated string at {}", self.pos),
        })?;
        let text = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Ok(text)
    }
}

#[derive(Debug, Clone)]
pub struct TypeNode {
    pub version: u16,
    pub level: u8,
    pub type_flags: u8,
    pub type_name: String,
    pub name: String,
    pub byte_size: i32,
    pub index: i32,
    pub meta_flag: i32,
    pub ref_type_hash: u64,
    /// Exclusive end of this node's subtree in the flat pre-order list.
    pub subtree_end: usize,
}

impl TypeNode {
    pub const FLAG_ALIGN: i32 = 0x4000;

    pub fn is_array(&self) -> bool {
        self.type_flags & 0x1 != 0
    }

    pub fn is_align(&self) -> bool {
        self.meta_flag & Self::FLAG_ALIGN != 0
    }
}

/// The managed type name a reference-type entry describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefTypeName {
    pub class_name: String,
    pub namespace: String,
    pub assembly: String,
}

#[derive(Debug, Clone)]
pub struct SerializedType {
    pub class_id: i32,
    pub is_stripped: bool,
    /// True for entries of the file's reference-type table (v20+).
    pub is_ref_type: bool,
    pub script_type_index: i16,
    /// Present for class 114 (MonoBehaviour) and for ref types that point at a script.
    pub script_id: Option<[u8; 16]>,
    pub old_type_hash: [u8; 16],
    pub nodes: Vec<TypeNode>,
    pub type_dependencies: Vec<i32>,
    /// Only set for reference types: the managed class this tree describes.
    pub ref_name: Option<RefTypeName>,
}

impl SerializedType {
    pub fn root(&self) -> Option<&TypeNode> {
        self.nodes.first()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectInfo {
    /// Relative to the owning file's `data_offset`.
    pub byte_start: u64,
    pub byte_size: u32,
    pub type_id: i32,
}

impl ObjectInfo {
    pub fn abs_start(&self, data_offset: u64) -> u64 {
        data_offset + self.byte_start
    }
}

#[derive(Debug, Clone)]
pub struct External {
    /// Leading string Unity calls `tempEmpty`; normally empty.
    pub temp_empty: String,
    /// The referenced file, historically called `pathName` in Unity's code.
    pub path: String,
    pub guid: [u8; 16],
    pub kind: i32,
}

#[derive(Debug, Clone)]
pub struct SerializedFile {
    pub data: Vec<u8>,
    pub metadata_size: u32,
    pub file_size: u32,
    pub version: u32,
    pub data_offset: u64,
    pub endian: Endian,
    pub unity_version: String,
    pub platform: i32,
    pub has_type_tree: bool,
    pub types: Vec<SerializedType>,
    /// Reference types (v20+): trees for managed classes, looked up by name.
    pub ref_types: Vec<SerializedType>,
    /// Object table in file order; path IDs are signed and genuinely negative.
    pub objects: Vec<(i64, ObjectInfo)>,
    pub externals: Vec<External>,
}

impl SerializedFile {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data, Endian::Big);
        let metadata_size = r.u32("metadata_size")?;
        let file_size = r.u32("file_size")?;
        let version = r.u32("version")?;
        if !(17..=22).contains(&version) {
            return Err(Error::Unsupported {
                what: "SerializedFile version outside 17..=22",
            });
        }

        // The header stays big-endian through the endianness field itself.
        let data_offset: u64;
        let raw_endian: u32;
        if version >= 22 {
            data_offset = r.i64("data_offset")? as u64;
            raw_endian = r.u32("endianness")?;
            r.take(3, "reserved")?;
        } else {
            data_offset = r.u32("data_offset")? as u64;
            raw_endian = r.u32("endianness")?;
        }
        // Everything past this point follows the declared order.
        let endian = if raw_endian == 0 { Endian::Little } else { Endian::Big };
        r.endian = endian;

        let unity_version = r.cstr("unity_version")?;
        let platform = r.i32("platform")?;
        let has_type_tree = r.u8("has_type_tree")? != 0;

        // v7..v14 carry a big-ID flag before the type table.
        if (7..14).contains(&version) {
            let _ = r.i32("big_id_enabled")?;
        }

        let type_count = r.u32("type_count")?;
        if type_count > 100_000 {
            return Err(Error::Implausible {
                what: "type count",
                value: type_count as u64,
            });
        }
        let mut types = Vec::with_capacity(type_count as usize);
        for _ in 0..type_count {
            types.push(read_type(&mut r, version, has_type_tree, false)?);
        }

        let object_count = r.u32("object_count")?;
        if object_count > 5_000_000 {
            return Err(Error::Implausible {
                what: "object count",
                value: object_count as u64,
            });
        }
        let mut objects = Vec::with_capacity(object_count.min(4096) as usize);
        for _ in 0..object_count {
            // Unity aligns every object entry to 4 bytes before the path ID.
            // When the table starts unaligned this byte decides the whole layout.
            r.align(4);
            let path_id = r.i64("object path_id")?;
            let byte_start = if version >= 22 {
                r.i64("object byte_start")? as u64
            } else {
                r.u32("object byte_start")? as u64
            };
            let byte_size = r.u32("object byte_size")?;
            let type_id = r.i32("object type_id")?;
            objects.push((path_id, ObjectInfo { byte_start, byte_size, type_id }));
        }

        if version >= 11 {
            let script_count = r.u32("script type count")?;
            if script_count > 1_000_000 {
                return Err(Error::Implausible {
                    what: "script type count",
                    value: script_count as u64,
                });
            }
            for _ in 0..script_count {
                let _ = r.i32("script file_id")?;
                if version >= 14 {
                    r.align(4);
                    let _ = r.i64("script identifier")?;
                } else {
                    let _ = r.i32("script identifier")?;
                }
            }
        }

        let mut externals = Vec::new();
        if version >= 6 {
            let ext_count = r.u32("external count")?;
            if ext_count > 1_000_000 {
                return Err(Error::Implausible {
                    what: "external count",
                    value: ext_count as u64,
                });
            }
            for _ in 0..ext_count {
                // Layout follows Unity's FileIdentifier: leading empty string,
                // 16-byte GUID, type, then the referenced path last.
                let temp_empty = r.cstr("external temp_empty")?;
                let mut guid = [0u8; 16];
                guid.copy_from_slice(r.take(16, "external guid")?);
                let kind = r.i32("external type")?;
                let path = r.cstr("external path")?;
                externals.push(External { temp_empty, path, guid, kind });
            }
        }

        // v20+ reference types carry a full tree per managed class. They sit at
        // the end of the metadata and do not move payload offsets, but skipping
        // them with a guessed size leaves the reader misaligned on the next
        // table, which is how a MonoBehaviour-only field becomes a parse error.
        let mut ref_types = Vec::new();
        if version >= 20 {
            let ref_count = r.u32("ref type count")?;
            if ref_count > 1_000_000 {
                return Err(Error::Implausible {
                    what: "ref type count",
                    value: ref_count as u64,
                });
            }
            for _ in 0..ref_count {
                ref_types.push(read_type(&mut r, version, has_type_tree, true)?);
            }
        }

        let sf = SerializedFile {
            data: data.to_vec(),
            metadata_size,
            file_size,
            version,
            data_offset,
            endian,
            unity_version,
            platform,
            has_type_tree,
            types,
            ref_types,
            objects,
            externals,
        };
        sf.validate()?;
        Ok(sf)
    }

    fn validate(&self) -> Result<()> {
        if self.file_size as usize != self.data.len() {
            return Err(Error::SizeMismatch {
                what: "SerializedFile",
                got: self.data.len(),
                want: self.file_size as usize,
            });
        }
        if self.data_offset == 0 || self.data_offset > self.data.len() as u64 {
            return Err(Error::Invalid {
                what: "data_offset",
                detail: format!("{} outside {} byte file", self.data_offset, self.data.len()),
            });
        }
        for (path_id, obj) in &self.objects {
            if obj.type_id < 0 || obj.type_id as usize >= self.types.len() {
                return Err(Error::Invalid {
                    what: "object type_id",
                    detail: format!("object {path_id} has type {} of {}", obj.type_id, self.types.len()),
                });
            }
            let end = self.data_offset + obj.byte_start + obj.byte_size as u64;
            if end > self.data.len() as u64 {
                return Err(Error::Invalid {
                    what: "object range",
                    detail: format!("object {path_id} runs past end of file"),
                });
            }
        }
        Ok(())
    }

    pub fn object(&self, path_id: i64) -> Option<&ObjectInfo> {
        self.objects
            .iter()
            .find(|(id, _)| *id == path_id)
            .map(|(_, o)| o)
    }

    pub fn type_of(&self, obj: &ObjectInfo) -> Option<&SerializedType> {
        self.types.get(obj.type_id as usize)
    }

    pub fn object_bytes(&self, obj: &ObjectInfo) -> &[u8] {
        let start = (self.data_offset + obj.byte_start) as usize;
        let end = start + obj.byte_size as usize;
        &self.data[start..end]
    }
}

fn read_type(
    r: &mut Reader<'_>,
    version: u32,
    has_type_tree: bool,
    is_ref_type: bool,
) -> Result<SerializedType> {
    let class_id = r.i32("type class_id")?;
    let is_stripped = if version >= 16 { r.u8("type is_stripped")? != 0 } else { false };
    let script_type_index = if version >= 17 { r.i16("script_type_index")? } else { -1 };

    // A MonoBehaviour entry (class 114), and a ref type bound to a script, carry
    // a 16-byte script identifier before the hash. Omitting it shifts the whole
    // type table by 16 bytes and only shows up on files that contain scripts.
    let script_id = if class_id == 114 || (is_ref_type && script_type_index >= 0) {
        let mut id = [0u8; 16];
        id.copy_from_slice(r.take(16, "type script_id")?);
        Some(id)
    } else {
        None
    };

    let mut old_type_hash = [0u8; 16];
    old_type_hash.copy_from_slice(r.take(16, "old_type_hash")?);

    let mut nodes = Vec::new();
    if has_type_tree {
        nodes = read_type_tree(r, version)?;
    }

    // v21+ trailing metadata. Reference types name their managed class; ordinary
    // types list the type indices their tree references.
    let mut type_dependencies = Vec::new();
    let mut ref_name = None;
    if version >= 21 {
        if is_ref_type {
            ref_name = Some(RefTypeName {
                class_name: r.cstr("ref type class_name")?,
                namespace: r.cstr("ref type namespace")?,
                assembly: r.cstr("ref type assembly")?,
            });
        } else {
            let count = r.i32("type dependency count")?;
            if !(0..=100_000).contains(&count) {
                return Err(Error::Implausible {
                    what: "type dependency count",
                    value: count.max(0) as u64,
                });
            }
            for _ in 0..count {
                type_dependencies.push(r.i32("type dependency")?);
            }
        }
    }

    Ok(SerializedType {
        class_id,
        is_stripped,
        is_ref_type,
        script_type_index,
        script_id,
        old_type_hash,
        nodes,
        type_dependencies,
        ref_name,
    })
}

fn read_type_tree(r: &mut Reader<'_>, version: u32) -> Result<Vec<TypeNode>> {
    let node_count = r.u32("type tree node count")? as usize;
    let string_size = if version >= 19 { r.u32("type tree string size")? as usize } else { 0 };
    if node_count > 100_000 || string_size > 16_000_000 {
        return Err(Error::Implausible {
            what: "type tree size",
            value: node_count.max(string_size) as u64,
        });
    }
    let node_bytes = if version >= 19 { 32 } else { 24 };
    let nodes_start = r.pos;
    if nodes_start + node_count * node_bytes + string_size > r.limit {
        return Err(Error::Truncated {
            what: "type tree",
            offset: nodes_start,
            wanted: node_count * node_bytes + string_size,
            available: r.remaining(),
        });
    }
    r.pos = nodes_start + node_count * node_bytes;
    let strings = r.take(string_size, "type tree strings")?;

    let mut nodes = Vec::with_capacity(node_count);
    let mut nr = Reader {
        data: r.data,
        pos: nodes_start,
        endian: r.endian,
        limit: nodes_start + node_count * node_bytes,
    };
    for _ in 0..node_count {
        let node_version = nr.u16("node version")?;
        let level = nr.u8("node level")?;
        let type_flags = nr.u8("node type_flags")?;
        let type_off = nr.u32("node type_str_offset")?;
        let name_off = nr.u32("node name_str_offset")?;
        let byte_size = nr.i32("node byte_size")?;
        let index = nr.i32("node index")?;
        let meta_flag = nr.i32("node meta_flag")?;
        let ref_type_hash = if version >= 19 { nr.u64("node ref_type_hash")? } else { 0 };
        nodes.push(TypeNode {
            version: node_version,
            level,
            type_flags,
            type_name: resolve_string(strings, type_off)?,
            name: resolve_string(strings, name_off)?,
            byte_size,
            index,
            meta_flag,
            ref_type_hash,
            subtree_end: 0,
        });
    }

    // v22 appends a content hash and a serialized size after the strings.
    if version >= 22 {
        let _ = r.take(16, "type tree content hash")?;
        let _ = r.u32("type tree serialized size")?;
    }

    compute_subtree_ends(&mut nodes);
    Ok(nodes)
}

/// Resolve a type-tree string reference.
///
/// Flag clear: byte offset into the type's own buffer. Flag set: index into
/// Unity's built-in name table.
fn resolve_string(strings: &[u8], value: u32) -> Result<String> {
    if value & 0x8000_0000 == 0 {
        let offset = value as usize;
        if offset >= strings.len() {
            return Err(Error::Invalid {
                what: "type string offset",
                detail: format!("{offset} outside {} byte buffer", strings.len()),
            });
        }
        let rest = &strings[offset..];
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    } else {
        let index = value & 0x7FFF_FFFF;
        Ok(match commonstrings::lookup(index) {
            Some(name) => name.to_string(),
            None => index.to_string(),
        })
    }
}

fn compute_subtree_ends(nodes: &mut [TypeNode]) {
    let n = nodes.len();
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..n {
        while let Some(&top) = stack.last() {
            if nodes[top].level >= nodes[i].level {
                nodes[top].subtree_end = i;
                stack.pop();
            } else {
                break;
            }
        }
        stack.push(i);
    }
    for top in stack {
        nodes[top].subtree_end = n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_respects_endian() {
        let data = [0x00, 0x00, 0x00, 0x02];
        let mut be = Reader::new(&data, Endian::Big);
        assert_eq!(be.u32("x").unwrap(), 2);
        let mut le = Reader::new(&data, Endian::Little);
        assert_eq!(le.u32("x").unwrap(), 0x0200_0000);
    }

    #[test]
    fn reader_rejects_past_limit() {
        let data = [0u8; 2];
        let mut r = Reader::new(&data, Endian::Little);
        r.u8("a").unwrap();
        assert!(r.u32("b").is_err());
    }

    #[test]
    fn reader_rejects_unterminated_cstr() {
        let data = b"abc";
        let mut r = Reader::new(data, Endian::Little);
        assert!(r.cstr("s").is_err());
    }

    #[test]
    fn align_moves_to_next_multiple() {
        let data = [0u8; 16];
        let mut r = Reader::new(&data, Endian::Little);
        r.pos = 5;
        r.align(4);
        assert_eq!(r.pos, 8);
        r.align(4);
        assert_eq!(r.pos, 8, "already aligned must not move");
    }

    #[test]
    fn builtin_common_strings_resolve() {
        // Top-bit set means the built-in table, low bits are the index.
        assert_eq!(resolve_string(b"", 0x8000_0000 | 55).unwrap(), "Base");
        assert_eq!(resolve_string(b"", 0x8000_0000 | 76).unwrap(), "bool");
    }

    #[test]
    fn buffer_strings_resolve() {
        let strings = b"<nul>Mesh\0m_Name\0";
        assert_eq!(resolve_string(strings, 5).unwrap(), "Mesh");
        assert_eq!(resolve_string(strings, 10).unwrap(), "m_Name");
    }

    #[test]
    fn out_of_range_buffer_offset_is_an_error() {
        assert!(resolve_string(b"abc", 99).is_err());
    }

    #[test]
    fn subtree_ends_are_preorder_correct() {
        let mk = |level: u8| TypeNode {
            version: 1,
            level,
            type_flags: 0,
            type_name: String::new(),
            name: String::new(),
            byte_size: -1,
            index: 0,
            meta_flag: 0,
            ref_type_hash: 0,
            subtree_end: 0,
        };
        // Levels 0,1,2,1,1 -- a single root with a nested child and two siblings.
        let mut nodes = vec![mk(0), mk(1), mk(2), mk(1), mk(1)];
        compute_subtree_ends(&mut nodes);
        assert_eq!(nodes[0].subtree_end, 5, "root covers everything");
        assert_eq!(nodes[1].subtree_end, 3, "child covers itself and its child");
        assert_eq!(nodes[2].subtree_end, 3, "leaf ends after itself");
        assert_eq!(nodes[3].subtree_end, 4, "leaf sibling");
        assert_eq!(nodes[4].subtree_end, 5, "last leaf reaches the end");
    }

    #[test]
    fn subtree_ends_handle_multiple_roots() {
        // Malformed for a type tree, but the computation must stay sane:
        // the first root's subtree stops where the second root begins.
        let mk = |level: u8| TypeNode {
            version: 1,
            level,
            type_flags: 0,
            type_name: String::new(),
            name: String::new(),
            byte_size: -1,
            index: 0,
            meta_flag: 0,
            ref_type_hash: 0,
            subtree_end: 0,
        };
        let mut nodes = vec![mk(0), mk(1), mk(0)];
        compute_subtree_ends(&mut nodes);
        assert_eq!(nodes[0].subtree_end, 2);
        assert_eq!(nodes[1].subtree_end, 2);
        assert_eq!(nodes[2].subtree_end, 3);
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut data = vec![0u8; 64];
        data[4..8].copy_from_slice(&64u32.to_be_bytes());
        data[8..12].copy_from_slice(&99u32.to_be_bytes());
        assert!(matches!(
            SerializedFile::parse(&data),
            Err(Error::Unsupported { .. })
        ));
    }
}
