//! Minimal NPZ writer (`numpy.savez` compatible, stored entries).
//!
//! The Blender interchange format is an NPZ archive. We only need to write it:
//! the arrays are already in memory, and NumPy reads stored (method 0) entries
//! just as happily as deflated ones. Keeping the writer stored-only avoids a
//! DEFLATE encoder without changing the format.
//!
//! Layout follows NumPy: a `.npy` header padded so the array data starts on a
//! 64-byte boundary, little-endian C-order payloads, and a ZIP central directory.

/// Build one `.npy` file.
pub fn npy(descr: &str, shape: &[usize], data: &[u8]) -> Vec<u8> {
    let shape_text = if shape.is_empty() {
        "()".to_string()
    } else if shape.len() == 1 {
        format!("({},)", shape[0])
    } else {
        format!(
            "({})",
            shape
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let header = format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': {shape_text}, }}");
    let mut header = header.into_bytes();
    // Magic (6) + version (2) + header length (2) + header + newline must be a
    // multiple of 64, which is what NumPy itself emits.
    let unpadded = 10 + header.len() + 1;
    let pad = (64 - (unpadded % 64)) % 64;
    header.extend(std::iter::repeat_n(b' ', pad));
    header.push(b'\n');

    let mut out = Vec::with_capacity(10 + header.len() + data.len());
    out.extend_from_slice(b"\x93NUMPY\x01\x00");
    out.extend_from_slice(&(header.len() as u16).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(data);
    out
}

/// An NPZ archive under construction.
#[derive(Default)]
pub struct Npz {
    entries: Vec<(String, Vec<u8>)>,
}

impl Npz {
    pub fn new() -> Self {
        Npz::default()
    }

    pub fn add_f32(&mut self, name: &str, shape: &[usize], values: &[f32]) {
        let mut data = Vec::with_capacity(values.len() * 4);
        for value in values {
            data.extend_from_slice(&value.to_le_bytes());
        }
        self.add(name, npy("<f4", shape, &data));
    }

    pub fn add_u32(&mut self, name: &str, shape: &[usize], values: &[u32]) {
        let mut data = Vec::with_capacity(values.len() * 4);
        for value in values {
            data.extend_from_slice(&value.to_le_bytes());
        }
        self.add(name, npy("<u4", shape, &data));
    }

    pub fn add_u8(&mut self, name: &str, values: &[u8]) {
        self.add(name, npy("|u1", &[values.len()], values));
    }

    /// A 0-d UTF-32LE string array, the shape NumPy uses for `np.array("…")`.
    pub fn add_string(&mut self, name: &str, text: &str) {
        let chars: Vec<u32> = text.chars().map(|c| c as u32).collect();
        let width = chars.len().max(1);
        let mut data = Vec::with_capacity(width * 4);
        for code in &chars {
            data.extend_from_slice(&code.to_le_bytes());
        }
        data.resize(width * 4, 0);
        self.add(name, npy(&format!("<U{width}"), &[], &data));
    }

    pub fn add(&mut self, name: &str, npy: Vec<u8>) {
        self.entries.push((format!("{name}.npy"), npy));
    }

    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in &self.entries {
            let offset = out.len() as u32;
            let crc = crate::crc32::crc32(data);
            write_local(&mut out, name, data, crc);
            write_central(&mut central, name, data, crc, offset);
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        let cd_size = central.len() as u32;
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk
        out.extend_from_slice(&0u16.to_le_bytes()); // central disk
        out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment length
        out
    }
}

fn write_local(out: &mut Vec<u8>, name: &str, data: &[u8], crc: u32) {
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method 0
    out.extend_from_slice(&0u16.to_le_bytes()); // time
    out.extend_from_slice(&0u16.to_le_bytes()); // date
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra length
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(data);
}

fn write_central(out: &mut Vec<u8>, name: &str, data: &[u8], crc: u32, offset: u32) {
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes()); // made by
    out.extend_from_slice(&20u16.to_le_bytes()); // needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method
    out.extend_from_slice(&0u16.to_le_bytes()); // time
    out.extend_from_slice(&0u16.to_le_bytes()); // date
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra
    out.extend_from_slice(&0u16.to_le_bytes()); // comment
    out.extend_from_slice(&0u16.to_le_bytes()); // disk
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::npy::Npy;
    use crate::zip::Archive;

    #[test]
    fn npy_data_starts_on_a_64_byte_boundary() {
        let file = npy("<f4", &[3], &[0u8; 12]);
        assert_eq!(&file[..6], b"\x93NUMPY");
        let header_len = u16::from_le_bytes([file[8], file[9]]) as usize;
        assert_eq!((10 + header_len) % 64, 0);
        let parsed = Npy::parse(&file).unwrap();
        assert_eq!(parsed.shape, vec![3]);
        assert_eq!(parsed.as_f32().unwrap(), vec![0.0; 3]);
    }

    #[test]
    fn npz_round_trips_through_the_reader() {
        let mut npz = Npz::new();
        npz.add_f32("positions", &[2, 3], &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        npz.add_u32("indices", &[2], &[4, 5]);
        npz.add_u8("raw", &[1, 2, 3]);
        npz.add_string("meta", "{\"name\":\"tri\"}");
        let bytes = npz.write();
        let archive = Archive::new(&bytes).expect("parse npz");
        let names: Vec<&str> = archive.names().collect();
        assert_eq!(
            names,
            ["positions.npy", "indices.npy", "raw.npy", "meta.npy"]
        );
        assert!(archive.entries().iter().all(|e| e.method == 0));
        let positions = Npy::parse(&archive.read("positions.npy", 1 << 20).unwrap()).unwrap();
        assert_eq!(positions.shape, vec![2, 3]);
        assert_eq!(
            positions.as_f32().unwrap(),
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]
        );
        let indices = Npy::parse(&archive.read("indices.npy", 1 << 20).unwrap()).unwrap();
        assert_eq!(indices.as_u32().unwrap(), vec![4, 5]);
        let meta = Npy::parse(&archive.read("meta.npy", 1 << 20).unwrap()).unwrap();
        assert_eq!(meta.as_string().unwrap(), "{\"name\":\"tri\"}");
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crate::crc32::crc32(b"123456789"), 0xCBF4_3926);
    }
}


