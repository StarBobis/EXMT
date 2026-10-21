//! Decode and re-encode a SerializedFile object through its type tree.
//!
//! This is the value layer on top of `serialized`: it turns an object's raw
//! bytes into a [`Value`] tree and writes one back. The algorithm is entirely
//! type-tree driven, in this order:
//!
//! 1. A primitive type name reads a fixed-width value.
//! 2. `string` reads an int32 length, that many bytes, then aligns to 4.
//! 3. `TypelessData` reads an int32 length and that many raw bytes.
//! 4. `pair` reads its two children as a pair.
//! 5. A node whose **first child** is `Array` is a variable-length container:
//!    the int32 count lives in that child, and the element type is the Array's
//!    *second* child. Getting this wrong reads the count as data.
//! 6. Anything else is a struct: one value per child, in declaration order.
//!
//! Alignment follows `meta_flag & 0x4000` and vectors inherit it from their
//! Array child. It is absolute (4-byte) from the start of the object payload.
//!
//! The contract the writer must satisfy: reading a payload and writing it back
//! unchanged reproduces the original bytes exactly.

use crate::error::{Error, Result};
use crate::serialized::{Endian, ObjectInfo, Reader, SerializedFile, SerializedType, TypeNode};

/// A decoded object tree.
///
/// Structs keep child order rather than a map: the writer must emit fields in
/// type-tree order, and a hash map would lose that.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Str(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Struct(Vec<(String, Value)>),
    Pair(Box<Value>, Box<Value>),
}

impl Value {
    /// Look up a struct field by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Struct(fields) => fields.iter().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Mutable struct field lookup.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        match self {
            Value::Struct(fields) => fields.iter_mut().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Value::I32(v) => Some(*v),
            Value::U32(v) => Some(*v as i32),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_struct(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Struct(f) => Some(f),
            _ => None,
        }
    }
}

/// Primitive type name -> (kind tag, byte width).
fn primitive_width(name: &str) -> Option<usize> {
    Some(match name {
        "SInt8" | "UInt8" | "char" => 1,
        "short" | "SInt16" | "unsigned short" | "UInt16" => 2,
        "int" | "SInt32" | "unsigned int" | "UInt32" | "Type*" | "float" => 4,
        "long long" | "SInt64" | "unsigned long long" | "UInt64" | "FileSize" | "double" => 8,
        "bool" => 1,
        _ => return None,
    })
}

struct ValueReader<'a> {
    nodes: &'a [TypeNode],
    r: Reader<'a>,
}

impl<'a> ValueReader<'a> {
    fn read_primitive(&mut self, name: &str) -> Result<Value> {
        Ok(match name {
            "SInt8" => Value::I8(self.r.take(1, "SInt8")?[0] as i8),
            "UInt8" | "char" => Value::U8(self.r.take(1, "UInt8")?[0]),
            "short" | "SInt16" => Value::I16(self.r.u16("SInt16")? as i16),
            "unsigned short" | "UInt16" => Value::U16(self.r.u16("UInt16")?),
            "int" | "SInt32" => Value::I32(self.r.u32("SInt32")? as i32),
            "unsigned int" | "UInt32" | "Type*" => Value::U32(self.r.u32("UInt32")?),
            "long long" | "SInt64" => Value::I64(self.r.u64("SInt64")? as i64),
            "unsigned long long" | "UInt64" | "FileSize" => Value::U64(self.r.u64("UInt64")?),
            "float" => Value::F32(f32::from_bits(self.r.u32("float")?)),
            "double" => Value::F64(f64::from_bits(self.r.u64("double")?)),
            "bool" => Value::Bool(self.r.u8("bool")? != 0),
            _ => return Err(Error::Unsupported { what: "primitive type" }),
        })
    }

    fn read_string(&mut self) -> Result<String> {
        let len = self.r.i32("string length")?;
        if len < 0 || len as usize > self.r.remaining() {
            return Err(Error::Invalid {
                what: "string length",
                detail: format!("{len} with {} bytes left", self.r.remaining()),
            });
        }
        let out = if len == 0 {
            String::new()
        } else {
            let raw = self.r.take(len as usize, "string bytes")?;
            String::from_utf8_lossy(raw).into_owned()
        };
        self.r.align(4);
        Ok(out)
    }

    fn read_typeless(&mut self) -> Result<Vec<u8>> {
        let len = self.r.i32("TypelessData length")?;
        if len < 0 || len as usize > self.r.remaining() {
            return Err(Error::Invalid {
                what: "TypelessData length",
                detail: format!("{len} with {} bytes left", self.r.remaining()),
            });
        }
        Ok(self.r.take(len as usize, "TypelessData")?.to_vec())
    }

    fn value(&mut self, idx: usize) -> Result<Value> {
        let node = &self.nodes[idx];
        // A vector inherits alignment from its Array child, and the padding is
        // applied once, after the whole value. Aligning inside the array branch
        // as well would double the padding and shift every later field.
        let mut align = node.is_align();
        let subtree_end = node.subtree_end;

        let value = if primitive_width(&node.type_name).is_some() {
            self.read_primitive(&node.type_name)?
        } else if node.type_name == "string" {
            Value::Str(self.read_string()?)
        } else if node.type_name == "TypelessData" {
            Value::Bytes(self.read_typeless()?)
        } else if node.type_name == "pair" {
            let first = idx + 1;
            if first >= subtree_end {
                return Err(Error::Invalid {
                    what: "pair",
                    detail: format!("node {idx} has no children"),
                });
            }
            let a = self.value(first)?;
            let second = self.nodes[first].subtree_end;
            if second >= subtree_end {
                return Err(Error::Invalid {
                    what: "pair",
                    detail: format!("node {idx} has only one child"),
                });
            }
            let b = self.value(second)?;
            Value::Pair(Box::new(a), Box::new(b))
        } else if idx + 1 < subtree_end && self.nodes[idx + 1].type_name == "Array" {
            // Variable-length container. The Array child holds two children:
            // the int32 count and the element type. The element is the *second*
            // one, so it must be reached by skipping the count's subtree --
            // using idx + 2 reads the count node itself as if it were the
            // element, which only looks correct when both happen to be ints.
            let arr = &self.nodes[idx + 1];
            align |= arr.is_align();
            let size_idx = idx + 2;
            if size_idx >= arr.subtree_end {
                return Err(Error::Invalid {
                    what: "array",
                    detail: format!("node {} has no count child", node.name),
                });
            }
            let element = self.nodes[size_idx].subtree_end;
            if element >= arr.subtree_end {
                return Err(Error::Invalid {
                    what: "array",
                    detail: format!("node {} has no element type", node.name),
                });
            }
            let count = self.r.i32("array length")?;
            if count < 0 {
                return Err(Error::Invalid {
                    what: "array length",
                    detail: format!("negative {count} at {}", node.name),
                });
            }
            let remain = self.r.remaining();
            if count as usize > remain {
                return Err(Error::Invalid {
                    what: "array length",
                    detail: format!(
                        "{} wants {count} elements with {remain} bytes left",
                        node.name
                    ),
                });
            }
            let mut items = Vec::new();
            for _ in 0..count {
                items.push(self.value(element)?);
            }
            Value::Array(items)
        } else {
            let mut fields = Vec::new();
            let mut i = idx + 1;
            while i < subtree_end {
                let child = &self.nodes[i];
                let v = self.value(i)?;
                fields.push((child.name.clone(), v));
                i = self.nodes[i].subtree_end.max(i + 1);
            }
            Value::Struct(fields)
        };

        if align {
            self.r.align(4);
        }
        Ok(value)
    }
}

/// Decode one object into a [`Value`] tree.
pub fn read_value(sf: &SerializedFile, obj: &ObjectInfo) -> Result<Value> {
    let ty = sf.type_of(obj).ok_or(Error::Invalid {
        what: "object type",
        detail: format!("type id {} out of range", obj.type_id),
    })?;
    decode_with_type(sf, ty, sf.object_bytes(obj))
}

/// Decode arbitrary payload bytes against an already-resolved type tree.
///
/// Useful for re-reading bytes that were just written, where there is no
/// `ObjectInfo` describing them yet.
pub fn decode_with_type(
    sf: &SerializedFile,
    ty: &SerializedType,
    payload: &[u8],
) -> Result<Value> {
    if ty.nodes.is_empty() {
        return Err(Error::Invalid {
            what: "type tree",
            detail: format!("class {} has no nodes", ty.class_id),
        });
    }
    let mut reader = ValueReader {
        nodes: &ty.nodes,
        r: Reader::new(payload, sf.endian),
    };
    reader.value(0)
}

struct ValueWriter<'a> {
    nodes: &'a [TypeNode],
    endian: Endian,
    buf: Vec<u8>,
}

impl<'a> ValueWriter<'a> {
    fn align(&mut self) {
        let pad = (4 - self.buf.len() % 4) % 4;
        if pad > 0 {
            self.buf.extend(std::iter::repeat(0u8).take(pad));
        }
    }

    fn put_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&match self.endian {
            Endian::Little => v.to_le_bytes(),
            Endian::Big => v.to_be_bytes(),
        });
    }

    fn put_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&match self.endian {
            Endian::Little => v.to_le_bytes(),
            Endian::Big => v.to_be_bytes(),
        });
    }

    fn put_i32(&mut self, v: i32) {
        self.put_u32(v as u32);
    }

    fn put_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&match self.endian {
            Endian::Little => v.to_le_bytes(),
            Endian::Big => v.to_be_bytes(),
        });
    }

    fn write_primitive(&mut self, name: &str, value: &Value) -> Result<()> {
        match (name, value) {
            ("SInt8", Value::I8(v)) => self.buf.push(*v as u8),
            ("UInt8" | "char", Value::U8(v)) => self.buf.push(*v),
            ("short" | "SInt16", Value::I16(v)) => self.put_u16(*v as u16),
            ("unsigned short" | "UInt16", Value::U16(v)) => self.put_u16(*v),
            ("int" | "SInt32", Value::I32(v)) => self.put_i32(*v),
            ("unsigned int" | "UInt32" | "Type*", Value::U32(v)) => self.put_u32(*v),
            ("long long" | "SInt64", Value::I64(v)) => self.put_u64(*v as u64),
            ("unsigned long long" | "UInt64" | "FileSize", Value::U64(v)) => self.put_u64(*v),
            ("float", Value::F32(v)) => self.put_u32(v.to_bits()),
            ("double", Value::F64(v)) => self.put_u64(v.to_bits()),
            // A bool may round-trip as either representation depending on how
            // the caller built it.
            ("bool", Value::Bool(v)) => self.buf.push(*v as u8),
            ("bool", Value::U8(v)) => self.buf.push(*v),
            (_, other) => {
                return Err(Error::Invalid {
                    what: "primitive write",
                    detail: format!("cannot write {other:?} as {name}"),
                });
            }
        }
        Ok(())
    }

    fn value(&mut self, idx: usize, value: &Value) -> Result<()> {
        let node = &self.nodes[idx];
        // Mirror the reader: one alignment, applied after the value, with the
        // vector's Array flag folded in.
        let mut align = node.is_align();
        let subtree_end = node.subtree_end;

        if primitive_width(&node.type_name).is_some() {
            self.write_primitive(&node.type_name, value)?;
        } else if node.type_name == "string" {
            let text = value.as_str().ok_or(Error::Invalid {
                what: "string write",
                detail: format!("expected string for {}", node.name),
            })?;
            let raw = text.as_bytes();
            self.put_i32(raw.len() as i32);
            self.buf.extend_from_slice(raw);
            self.align();
        } else if node.type_name == "TypelessData" {
            let raw = value.as_bytes().ok_or(Error::Invalid {
                what: "TypelessData write",
                detail: format!("expected bytes for {}", node.name),
            })?;
            self.put_i32(raw.len() as i32);
            self.buf.extend_from_slice(raw);
        } else if node.type_name == "pair" {
            let (a, b) = match value {
                Value::Pair(a, b) => (a.as_ref(), b.as_ref()),
                other => {
                    return Err(Error::Invalid {
                        what: "pair write",
                        detail: format!("expected pair, got {other:?}"),
                    })
                }
            };
            let first = idx + 1;
            let second = self.nodes[first].subtree_end;
            self.value(first, a)?;
            self.value(second, b)?;
        } else if idx + 1 < subtree_end && self.nodes[idx + 1].type_name == "Array" {
            let arr = &self.nodes[idx + 1];
            align |= arr.is_align();
            let size_idx = idx + 2;
            let element = self
                .nodes
                .get(size_idx)
                .map(|n| n.subtree_end)
                .ok_or(Error::Invalid {
                    what: "array write",
                    detail: format!("node {} has no count child", node.name),
                })?;
            let items = value.as_array().ok_or(Error::Invalid {
                what: "array write",
                detail: format!("expected array for {}", node.name),
            })?;
            self.put_i32(items.len() as i32);
            for item in items {
                self.value(element, item)?;
            }
        } else {
            let fields = value.as_struct().ok_or(Error::Invalid {
                what: "struct write",
                detail: format!("expected struct for {}", node.name),
            })?;
            let mut i = idx + 1;
            while i < subtree_end {
                let child = &self.nodes[i];
                let field = fields
                    .iter()
                    .find(|(k, _)| *k == child.name)
                    .map(|(_, v)| v)
                    .ok_or_else(|| Error::Invalid {
                        what: "struct write",
                        detail: format!("missing field {} for {}", child.name, node.name),
                    })?;
                self.value(i, field)?;
                i = self.nodes[i].subtree_end.max(i + 1);
            }
        }

        if align {
            self.align();
        }
        Ok(())
    }
}

/// Encode a [`Value`] tree into the object's payload bytes.
pub fn write_value(sf: &SerializedFile, obj: &ObjectInfo, value: &Value) -> Result<Vec<u8>> {
    let ty = sf.type_of(obj).ok_or(Error::Invalid {
        what: "object type",
        detail: format!("type id {} out of range", obj.type_id),
    })?;
    if ty.nodes.is_empty() {
        return Err(Error::Invalid {
            what: "type tree",
            detail: format!("class {} has no nodes", ty.class_id),
        });
    }
    let mut w = ValueWriter { nodes: &ty.nodes, endian: sf.endian, buf: Vec::new() };
    w.value(0, value)?;
    Ok(w.buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(level: u8, type_name: &str, name: &str, meta_flag: i32) -> TypeNode {
        TypeNode {
            version: 1,
            level,
            type_flags: 0,
            type_name: type_name.to_string(),
            name: name.to_string(),
            byte_size: -1,
            index: 0,
            meta_flag,
            ref_type_hash: 0,
            subtree_end: 0,
        }
    }

    fn finish(mut nodes: Vec<TypeNode>) -> Vec<TypeNode> {
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
        nodes
    }

    fn read_nodes(nodes: &[TypeNode], data: &[u8], endian: Endian) -> Result<Value> {
        let mut r = ValueReader { nodes, r: Reader::new(data, endian) };
        r.value(0)
    }

    fn write_nodes(nodes: &[TypeNode], value: &Value, endian: Endian) -> Result<Vec<u8>> {
        let mut w = ValueWriter { nodes, endian, buf: Vec::new() };
        w.value(0, value)?;
        Ok(w.buf)
    }

    #[test]
    fn primitives_round_trip() {
        let nodes = finish(vec![
            node(0, "Root", "Base", 0),
            node(1, "int", "i", 0),
            node(1, "float", "f", 0),
            node(1, "bool", "b", 0),
            node(1, "UInt64", "u", 0),
        ]);
        let mut data = Vec::new();
        data.extend_from_slice(&(-7i32).to_le_bytes());
        data.extend_from_slice(&1.5f32.to_le_bytes());
        data.push(1);
        data.extend_from_slice(&(2u64 ^ (1 << 63)).to_le_bytes());
        let v = read_nodes(&nodes, &data, Endian::Little).unwrap();
        assert_eq!(v.get("i"), Some(&Value::I32(-7)));
        assert_eq!(v.get("b"), Some(&Value::Bool(true)));
        assert_eq!(write_nodes(&nodes, &v, Endian::Little).unwrap(), data);
    }

    #[test]
    fn string_alignment_round_trips() {
        let nodes = finish(vec![
            node(0, "Root", "Base", 0),
            node(1, "string", "s", 0),
            node(1, "int", "after", 0),
        ]);
        for text in ["", "a", "abcd", "abcde", "x".repeat(40).as_str()] {
            let payload = Value::Struct(vec![
                ("s".into(), Value::Str(text.into())),
                ("after".into(), Value::I32(9)),
            ]);
            let bytes = write_nodes(&nodes, &payload, Endian::Little).unwrap();
            assert_eq!(bytes.len() % 4, 0, "string output must stay aligned");
            let back = read_nodes(&nodes, &bytes, Endian::Little).unwrap();
            assert_eq!(back.get("s").and_then(|v| v.as_str()), Some(text));
            assert_eq!(back.get("after"), Some(&Value::I32(9)));
        }
    }

    #[test]
    fn array_round_trips() {
        // The element type deliberately differs from the count type. When both
        // were `int`, reading the count node as if it were the element produced
        // the same bytes and hid an off-by-one in the element lookup.
        let nodes = finish(vec![
            node(0, "Root", "Base", 0),
            node(1, "vector", "items", 0),
            node(2, "Array", "Array", 0),
            node(3, "int", "size", 4),
            node(3, "Entry", "data", 0),
            node(4, "int", "x", 4),
            node(4, "float", "y", 0),
        ]);

        let payload = Value::Struct(vec![(
            "items".into(),
            Value::Array(vec![Value::Struct(vec![
                ("x".into(), Value::I32(7)),
                ("y".into(), Value::F32(0.5)),
            ])]),
        )]);
        let bytes = write_nodes(&nodes, &payload, Endian::Little).unwrap();
        // 4-byte count + one 8-byte record.
        assert_eq!(bytes.len(), 12);
        let back = read_nodes(&nodes, &bytes, Endian::Little).unwrap();
        let items = back.get("items").and_then(|v| v.as_array()).expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].get("x"), Some(&Value::I32(7)));
        assert_eq!(items[0].get("y"), Some(&Value::F32(0.5)));

        // Empty and multi-element cases too.
        for n in [0usize, 3] {
            let entries: Vec<Value> = (0..n)
                .map(|i| {
                    Value::Struct(vec![
                        ("x".into(), Value::I32(i as i32)),
                        ("y".into(), Value::F32(i as f32)),
                    ])
                })
                .collect();
            let p = Value::Struct(vec![("items".into(), Value::Array(entries))]);
            let b = write_nodes(&nodes, &p, Endian::Little).unwrap();
            assert_eq!(b.len(), 4 + n * 8);
            let r = read_nodes(&nodes, &b, Endian::Little).unwrap();
            assert_eq!(r.get("items").and_then(|v| v.as_array()).unwrap().len(), n);
        }
    }

    #[test]
    fn align_flag_pads_after_node() {
        let nodes = finish(vec![
            node(0, "Root", "Base", 0),
            node(1, "SInt8", "one", TypeNode::FLAG_ALIGN),
            node(1, "int", "after", 0),
        ]);
        let mut data = vec![7u8, 0, 0, 0];
        data.extend_from_slice(&42i32.to_le_bytes());
        let back = read_nodes(&nodes, &data, Endian::Little).unwrap();
        assert_eq!(back.get("one"), Some(&Value::I8(7)));
        assert_eq!(back.get("after"), Some(&Value::I32(42)));
    }

    #[test]
    fn missing_struct_field_is_an_error() {
        let nodes = finish(vec![
            node(0, "Root", "Base", 0),
            node(1, "int", "a", 0),
            node(1, "int", "b", 0),
        ]);
        let payload = Value::Struct(vec![("a".into(), Value::I32(1))]);
        assert!(write_nodes(&nodes, &payload, Endian::Little).is_err());
    }

    #[test]
    fn negative_array_length_rejected() {
        let nodes = finish(vec![
            node(0, "Root", "Base", 0),
            node(1, "vector", "items", 0),
            node(2, "Array", "Array", 0),
            node(3, "int", "size", 4),
            node(3, "int", "data", 0),
        ]);
        let data = (-1i32).to_le_bytes();
        assert!(read_nodes(&nodes, &data, Endian::Little).is_err());
    }
}
