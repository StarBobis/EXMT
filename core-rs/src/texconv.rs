//! `texconv.exe` (Microsoft DirectXTex) as the image engine.
//!
//! All pixel work -- decoding block-compressed textures, resizing, writing PNG,
//! encoding replacement textures with mips -- is delegated to the DirectXTex
//! command-line tool that ships with the app. This crate only adapts Unity's raw
//! payloads to a DDS header ([`crate::dds`]) and drives the process.
//!
//! The executable path is explicit: the library never guesses where an external
//! binary lives. The shell resolves it (env var `EXMT_TEXCONV`, then the file
//! bundled next to the app).
//!
//! Every run happens in a fresh temporary directory that is removed afterwards,
//! and failures carry the tool's own message instead of a bare exit code.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::dds::{self, DdsFormat};
use crate::error::{Error, Result};

/// Wrapper around one `texconv.exe`.
#[derive(Debug, Clone)]
pub struct Texconv {
    exe: PathBuf,
}

impl Texconv {
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Texconv { exe: exe.into() }
    }

    /// Resolve the tool from `EXMT_TEXCONV`, then from the repo's bundled copy.
    ///
    /// Callers that ship the binary elsewhere should pass the path themselves;
    /// this helper exists for tools and tests.
    pub fn locate() -> Option<Self> {
        if let Ok(path) = std::env::var("EXMT_TEXCONV") {
            if Path::new(&path).is_file() {
                return Some(Texconv::new(path));
            }
        }
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/bin/texconv.exe");
        if bundled.is_file() {
            return Some(Texconv::new(bundled));
        }
        None
    }

    pub fn path(&self) -> &Path {
        &self.exe
    }

    pub fn is_available(&self) -> bool {
        self.exe.is_file()
    }

    /// Decode mip 0 of a Unity texture into upright RGBA8.
    ///
    /// This path goestexconv -> DDS (`R8G8B8A8_UNORM`) and reads the payload
    /// directly, which avoids needing a PNG decoder in tests and tools.
    pub fn decode_rgba(
        &self,
        unity_format: i32,
        width: u32,
        height: u32,
        payload: &[u8],
    ) -> Result<Vec<u8>> {
        let format = dds::from_unity(unity_format).ok_or(Error::Unsupported {
            what: "texconv input format",
        })?;
        let input = dds::build(width, height, format, payload)?;
        let dir = TempDir::new()?;
        let src = dir.path.join("in.dds");
        std::fs::write(&src, &input)?;

        self.run(&[
            "-nologo".into(),
            "-y".into(),
            "-m".into(),
            "1".into(),
            "-f".into(),
            "R8G8B8A8_UNORM".into(),
            "-ft".into(),
            "dds".into(),
            "-vflip".into(),
            "-o".into(),
            dir.out_arg(),
            src_arg(&src),
        ])?;

        let out = std::fs::read(dir.path.join("out").join("in.dds"))?;
        let parsed = dds::parse(&out)?;
        let rgba = parsed.payload(&out)?;
        let want = width as usize * height as usize * 4;
        if rgba.len() != want {
            return Err(Error::SizeMismatch {
                what: "texconv rgba",
                got: rgba.len(),
                want,
            });
        }
        Ok(rgba.to_vec())
    }

    /// Decode mip 0 and optionally resize, returning PNG bytes.
    ///
    /// `target` is an exact output size; callers that preserve aspect ratio must
    /// compute it first (texconv stretches when both dimensions are given).
    pub fn decode_png(
        &self,
        unity_format: i32,
        width: u32,
        height: u32,
        payload: &[u8],
        target: Option<(u32, u32)>,
    ) -> Result<Vec<u8>> {
        let format = dds::from_unity(unity_format).ok_or(Error::Unsupported {
            what: "texconv input format",
        })?;
        let input = dds::build(width, height, format, payload)?;
        let dir = TempDir::new()?;
        let src = dir.path.join("in.dds");
        std::fs::write(&src, &input)?;

        let mut args = vec![
            "-nologo".to_string(),
            "-y".to_string(),
            "-m".to_string(),
            "1".to_string(),
            "-f".to_string(),
            "R8G8B8A8_UNORM".to_string(),
            "-ft".to_string(),
            "png".to_string(),
            "-vflip".to_string(),
            "-if".to_string(),
            "FANT".to_string(),
        ];
        if let Some((w, h)) = target {
            args.push("-w".into());
            args.push(w.to_string());
            args.push("-h".into());
            args.push(h.to_string());
        }
        args.push("-o".into());
        args.push(dir.out_arg());
        args.push(src_arg(&src));
        self.run(&args)?;

        let png = std::fs::read(dir.path.join("out").join("in.png"))?;
        let (got_w, got_h) = png_size(&png).ok_or(Error::Invalid {
            what: "texconv png",
            detail: "missing IHDR".into(),
        })?;
        if let Some((w, h)) = target {
            if (got_w, got_h) != (w, h) {
                return Err(Error::SizeMismatch {
                    what: "texconv png size",
                    got: (got_w * got_h) as usize,
                    want: (w * h) as usize,
                });
            }
        } else if (got_w, got_h) != (width, height) {
            return Err(Error::SizeMismatch {
                what: "texconv png size",
                got: (got_w * got_h) as usize,
                want: (width * height) as usize,
            });
        }
        Ok(png)
    }

    /// Decode mip 0 and optionally resize, returning a JPEG preview.
    /// The original DDS payload is exported separately and remains lossless.
    pub fn decode_jpg(
        &self,
        unity_format: i32,
        width: u32,
        height: u32,
        payload: &[u8],
        target: Option<(u32, u32)>,
    ) -> Result<Vec<u8>> {
        let format = dds::from_unity(unity_format).ok_or(Error::Unsupported {
            what: "texconv input format",
        })?;
        let input = dds::build(width, height, format, payload)?;
        let dir = TempDir::new()?;
        let src = dir.path.join("in.dds");
        std::fs::write(&src, &input)?;

        let mut args = vec![
            "-nologo".to_string(),
            "-y".to_string(),
            "-m".to_string(),
            "1".to_string(),
            "-f".to_string(),
            "R8G8B8A8_UNORM".to_string(),
            "-ft".to_string(),
            "jpg".to_string(),
            "-vflip".to_string(),
            "-if".to_string(),
            "FANT".to_string(),
        ];
        if let Some((w, h)) = target {
            args.extend(["-w".into(), w.to_string(), "-h".into(), h.to_string()]);
        }
        args.extend(["-o".into(), dir.out_arg(), src_arg(&src)]);
        self.run(&args)?;

        let jpg = std::fs::read(dir.path.join("out").join("in.jpg"))?;
        let got = jpeg_size(&jpg).ok_or(Error::Invalid {
            what: "texconv jpg",
            detail: "missing SOF marker".into(),
        })?;
        let expected = target.unwrap_or((width, height));
        if got != expected {
            return Err(Error::SizeMismatch {
                what: "texconv jpg size",
                got: (got.0 * got.1) as usize,
                want: (expected.0 * expected.1) as usize,
            });
        }
        Ok(jpg)
    }

    /// Convert upright RGBA8 images to PNG, batching many images per process.
    ///
    /// A texconv start-up costs tens of milliseconds, which is most of a
    /// 160x160 thumbnail. One call carries a whole batch; a failed batch is
    /// retried image by image so one bad texture cannot fail the skin.
    ///
    /// Results are positionally aligned with `images`; a failure is reported on
    /// its own row instead of aborting the batch.
    pub fn rgba_to_png(&self, images: &[(u32, u32, Vec<u8>)]) -> Vec<Result<Vec<u8>>> {
        const BATCH: usize = 32;
        let mut rows: Vec<Option<Result<Vec<u8>>>> = (0..images.len()).map(|_| None).collect();
        let mut start = 0;
        while start < images.len() {
            let end = (start + BATCH).min(images.len());
            match self.rgba_batch(&images[start..end]) {
                Ok(pngs) => {
                    for (offset, png) in pngs.into_iter().enumerate() {
                        rows[start + offset] = Some(Ok(png));
                    }
                }
                Err(_) => {
                    for index in start..end {
                        rows[index] = Some(
                            self.rgba_batch(&images[index..=index])
                                .map(|mut pngs| pngs.remove(0)),
                        );
                    }
                }
            }
            start = end;
        }
        rows.into_iter()
            .map(|row| {
                row.unwrap_or_else(|| {
                    Err(Error::Invalid {
                        what: "rgba to png",
                        detail: "no result produced".into(),
                    })
                })
            })
            .collect()
    }

    fn rgba_batch(&self, images: &[(u32, u32, Vec<u8>)]) -> Result<Vec<Vec<u8>>> {
        let dir = TempDir::new()?;
        let mut args: Vec<String> = vec![
            "-nologo".into(),
            "-y".into(),
            "-f".into(),
            "R8G8B8A8_UNORM".into(),
            "-ft".into(),
            "png".into(),
            "-m".into(),
            "1".into(),
            "-o".into(),
            dir.out_arg(),
        ];
        let mut names = Vec::with_capacity(images.len());
        for (index, (width, height, rgba)) in images.iter().enumerate() {
            let dds = dds::build(*width, *height, DdsFormat::Rgba32, rgba)?;
            let stem = format!("in{index}");
            let src = dir.path.join(format!("{stem}.dds"));
            std::fs::write(&src, &dds)?;
            names.push((format!("{stem}.png"), *width, *height));
            args.push(src_arg(&src));
        }
        self.run(&args)?;
        let mut out = Vec::with_capacity(names.len());
        for (name, width, height) in names {
            let png = std::fs::read(dir.path.join("out").join(&name))?;
            let got = png_size(&png).ok_or(Error::Invalid {
                what: "texconv png",
                detail: "missing IHDR".into(),
            })?;
            if got != (width, height) {
                return Err(Error::SizeMismatch {
                    what: "texconv png size",
                    got: (got.0 * got.1) as usize,
                    want: (width * height) as usize,
                });
            }
            out.push(png);
        }
        Ok(out)
    }

    /// Encode a PNG into the Unity payload layout for `unity_format`.
    ///
    /// `size` optionally forces the output dimensions (texconv stretches to the
    /// exact pair) and `mips` asks DirectXTex to generate a mip chain.
    pub fn encode(
        &self,
        png: &[u8],
        unity_format: i32,
        size: Option<(u32, u32)>,
        mips: u32,
    ) -> Result<Vec<u8>> {
        let (dxgi, unity_expected) = match unity_format {
            10 => ("BC1_UNORM", 10),
            12 => ("BC3_UNORM", 12),
            26 => ("BC4_UNORM", 26),
            27 => ("BC5_UNORM", 27),
            25 => ("BC7_UNORM", 25),
            4 => ("R8G8B8A8_UNORM", 4),
            _ => {
                return Err(Error::Unsupported {
                    what: "texconv output format",
                })
            }
        };
        if mips == 0 || mips > 20 {
            return Err(Error::Invalid {
                what: "texture mips",
                detail: mips.to_string(),
            });
        }
        let dir = TempDir::new()?;
        let src = dir.path.join("in.png");
        std::fs::write(&src, png)?;

        let mut args = vec![
            "-nologo".to_string(),
            "-y".to_string(),
            "-f".to_string(),
            dxgi.to_string(),
            "-m".to_string(),
            mips.to_string(),
            // `-srgbi -srgbo` is an identity transform on the pixel values.
            // Without it texconv treats a Blender-written PNG as sRGB and
            // writes linear blocks (measured 31 -> 4, 219 -> 181, i.e.
            // everything much darker; the fixture test measures 128 -> 58).
            // The numbers in the PNG are exactly the numbers that must end up
            // in the asset.
            "-srgbi".to_string(),
            "-srgbo".to_string(),
            "-o".to_string(),
            dir.out_arg(),
        ];
        if let Some((w, h)) = size {
            args.push("-w".into());
            args.push(w.to_string());
            args.push("-h".into());
            args.push(h.to_string());
        }
        args.push(src_arg(&src));
        self.run(&args)?;

        let out = std::fs::read(dir.path.join("out").join("in.dds"))?;
        let parsed = dds::parse(&out)?;
        if parsed.unity_format() != Some(unity_expected) {
            return Err(Error::Invalid {
                what: "texconv output format",
                detail: format!("wanted {unity_expected}, got {:?}", parsed.unity_format()),
            });
        }
        if parsed.mip_count != mips {
            return Err(Error::Invalid {
                what: "texconv output mips",
                detail: format!("wanted {mips}, got {}", parsed.mip_count),
            });
        }
        Ok(parsed.payload(&out)?.to_vec())
    }

    fn run(&self, args: &[String]) -> Result<()> {
        let mut cmd = Command::new(&self.exe);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let output = cmd.output().map_err(|e| Error::Invalid {
            what: "texconv",
            detail: format!("failed to start {}: {e}", self.exe.display()),
        })?;
        if !output.status.success() {
            let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            let tail: String = text
                .chars()
                .rev()
                .take(600)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            return Err(Error::Invalid {
                what: "texconv",
                detail: format!("exit {:?}: {}", output.status.code(), tail.trim()),
            });
        }
        Ok(())
    }
}

fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    Some((w, h))
}

fn jpeg_size(jpg: &[u8]) -> Option<(u32, u32)> {
    if jpg.len() < 4 || jpg[..2] != [0xff, 0xd8] {
        return None;
    }
    let mut at = 2usize;
    while at + 4 <= jpg.len() {
        while at < jpg.len() && jpg[at] != 0xff {
            at += 1;
        }
        while at < jpg.len() && jpg[at] == 0xff {
            at += 1;
        }
        let marker = *jpg.get(at)?;
        at += 1;
        if marker == 0xd8 || marker == 0xd9 {
            continue;
        }
        let len = u16::from_be_bytes([*jpg.get(at)?, *jpg.get(at + 1)?]) as usize;
        if len < 2 || at + len > jpg.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if len < 7 {
                return None;
            }
            let height = u16::from_be_bytes([jpg[at + 3], jpg[at + 4]]) as u32;
            let width = u16::from_be_bytes([jpg[at + 5], jpg[at + 6]]) as u32;
            return Some((width, height));
        }
        at += len;
    }
    None
}

fn src_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Unique temporary directory, removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let name = format!(
            "exmt-texconv-{}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            nanos
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(path.join("out"))?;
        Ok(TempDir { path })
    }

    fn out_arg(&self) -> String {
        self.path.join("out").to_string_lossy().into_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 4x4 solid red and 8x8 smooth gradient, both written by Pillow.
    const RED_PNG: &str = "89504e470d0a1a0a0000000d4948445200000004000000040806000000a9f19e7e0000001549444154789c63fccfc0f09f01093021738813000083d1020604bc24470000000049454e44ae426082";
    const GRADIENT_PNG: &str = "89504e470d0a1a0a0000000d4948445200000008000000080806000000c40fbe8b0000001a49444154789c63646070f8afc0c0c0800bb380493c60782800009eba031d560b39030000000049454e44ae426082";
    /// 4x4 solid 128 grey with the `sRGB` + `gAMA` chunks Blender writes.
    const GRAY_SRGB_PNG: &str = "89504e470d0a1a0a0000000d4948445200000004000000040806000000a9f19e7e000000017352474200aece1ce90000000467414d410000b18f0bfc6105000000124944415478da63686868f88f8c1948170000428727f160b6f02e0000000049454e44ae426082";

    fn texconv() -> Option<Texconv> {
        let tool = Texconv::locate();
        if tool.is_none() {
            eprintln!("skip: texconv.exe not available (set EXMT_TEXCONV)");
        }
        tool
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_bc1_to_exact_pixels() {
        let Some(tool) = texconv() else { return };
        // c0 = red (0xF800), c1 = black, every index 0.
        let block = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
        let rgba = tool.decode_rgba(10, 4, 4, &block).unwrap();
        assert_eq!(rgba.len(), 64);
        for px in rgba.chunks_exact(4) {
            assert_eq!(px, &[255, 0, 0, 255]);
        }
    }

    #[test]
    fn decodes_bc7_transparent_block() {
        let Some(tool) = texconv() else { return };
        // Mode 6, every field zero: endpoints (0,0,0,0).
        let block = [0x40u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let rgba = tool.decode_rgba(25, 4, 4, &block).unwrap();
        for px in rgba.chunks_exact(4) {
            assert_eq!(px, &[0, 0, 0, 0]);
        }
    }

    #[test]
    fn decodes_and_resizes_to_png() {
        let Some(tool) = texconv() else { return };
        let block = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
        let png = tool.decode_png(10, 4, 4, &block, Some((2, 3))).unwrap();
        assert_eq!(png_size(&png), Some((2, 3)));
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn decodes_a_jpeg_preview() {
        let Some(tool) = texconv() else { return };
        let block = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
        let jpg = tool.decode_jpg(10, 4, 4, &block, None).unwrap();
        assert_eq!(&jpg[..2], [0xff, 0xd8]);
        assert_eq!(jpeg_size(&jpg), Some((4, 4)));
        let small = tool.decode_jpg(10, 4, 4, &block, Some((2, 2))).unwrap();
        assert_eq!(jpeg_size(&small), Some((2, 2)));
    }

    #[test]
    fn encodes_png_to_bc1() {
        let Some(tool) = texconv() else { return };
        let png = unhex(RED_PNG);
        let payload = tool.encode(&png, 10, Some((4, 4)), 1).unwrap();
        assert_eq!(payload.len(), 8, "one 4x4 BC1 block");
        let rgba = tool.decode_rgba(10, 4, 4, &payload).unwrap();
        for px in rgba.chunks_exact(4) {
            assert_eq!(px, &[255, 0, 0, 255]);
        }
    }

    #[test]
    fn encode_keeps_png_values_unconverted() {
        let Some(tool) = texconv() else { return };
        // Blender writes PNGs with `sRGB` + `gAMA`; texconv honours them unless
        // told the numbers are already the target values.  Without
        // `-srgbi -srgbo` this grey lands near 13 (measured on a real
        // replacement PNG: 64 -> 13), so the test pins the identity transform.
        let png = unhex(GRAY_SRGB_PNG);
        let payload = tool.encode(&png, 10, Some((4, 4)), 1).unwrap();
        let rgba = tool.decode_rgba(10, 4, 4, &payload).unwrap();
        for px in rgba.chunks_exact(4) {
            assert!(px[0].abs_diff(128) <= 4, "grey became {px:?}");
        }
    }

    #[test]
    fn encodes_a_mip_chain() {
        let Some(tool) = texconv() else { return };
        let png = unhex(GRADIENT_PNG);
        let payload = tool.encode(&png, 10, Some((8, 8)), 4).unwrap();
        // 8x8, 4x4, 2x2, 1x1 blocks -> 32 + 8 + 8 + 8 bytes.
        assert_eq!(payload.len(), 56);
        let back = tool.decode_rgba(10, 8, 8, &payload[..32]).unwrap();
        assert_eq!(back.len(), 8 * 8 * 4);
    }

    #[test]
    fn converts_rgba_images_in_one_batch() {
        let Some(tool) = texconv() else { return };
        let red = (4u32, 4u32, [255u8, 0, 0, 255].repeat(16));
        let green = (2u32, 3u32, [0u8, 255, 0, 255].repeat(6));
        let pngs = tool.rgba_to_png(&[red, green]);
        assert_eq!(pngs.len(), 2);
        assert_eq!(png_size(&pngs[0].as_ref().unwrap()), Some((4, 4)));
        assert_eq!(png_size(&pngs[1].as_ref().unwrap()), Some((2, 3)));
    }

    #[test]
    fn a_bad_image_does_not_sink_the_batch() {
        let Some(tool) = texconv() else { return };
        let good = (4u32, 4u32, vec![0u8, 0, 255, 255].repeat(16));
        let bad = (4u32, 4u32, vec![0u8; 12]); // wrong payload length
        let pngs = tool.rgba_to_png(&[good, bad.clone()]);
        assert_eq!(pngs.len(), 2);
        assert_eq!(png_size(&pngs[0].as_ref().unwrap()), Some((4, 4)));
        assert!(pngs[1].is_err(), "wrong-sized input must report its own error");
        let pngs = tool.rgba_to_png(&[bad]);
        assert!(pngs[0].is_err());
    }

    #[test]
    fn rejects_formats_texconv_cannot_wrap() {
        let Some(tool) = texconv() else { return };
        assert!(tool.decode_rgba(17, 4, 4, &[0u8; 64]).is_err());
    }
}
