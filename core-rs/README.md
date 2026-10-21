# exmt-core

Self-hosted Unity asset pipeline for EXMT Modbox. Parses and writes the formats
GF2's bundles use, using **only the Rust standard library** — no third-party
crates, no Python at runtime.

This exists because the Python core under `../core` depends on UnityPy, numpy,
Pillow, `lz4` and `texture2ddecoder`. The goal is a pipeline we can read, fix and
ship without upstream churn.

## Current state

| Layer | Status | Module |
|---|---|---|
| LZ4 block codec | done | `src/lz4.rs` |
| UnityFS container + NEP2 | done | `src/container.rs` |
| SerializedFile v17–v22 | done | `src/serialized.rs` |
| Type-tree read/write | done | `src/typetree.rs` |
| Mesh vertex/index layer | done | `src/mesh.rs` |
| Texture2D semantics | done | `src/texture.rs` |
| DDS adapter for texconv | done | `src/dds.rs` |
| texconv process wrapper | done | `src/texconv.rs` |
| Geometry payload (three.js) | done | `src/skin.rs` |
| SerializedFile rebuild (v17–v21) | done | `src/serialized.rs` |
| Bundle patch + rebuild | done | `src/asset.rs` |
| NPZ reader (ZIP + DEFLATE + `.npy`) | done | `src/inflate.rs`, `src/zip.rs`, `src/npy.rs` |
| `skinpack` command in Rust | done | `../src-tauri/src/skinpack.rs` |
| `modpack` texture path | done | `../src-tauri/src/modpack.rs` |
| `.exmtmesh` mesh patches | done (same cook; other cooks fall back) | `../src-tauri/src/exmtmesh.rs`, `src/mesh.rs` |
| preview `thumbs` / `skinfo` | done | `../src-tauri/src/preview.rs` |
| `analyze` (GameCache) | **not started** (still Python) | — |

`142 tests`, all green. See "Verification" below for what they actually prove.

Pixel work is **not** implemented here: decoding, resizing, PNG output and BC
encoding all go through the bundled DirectXTex `texconv.exe`
(`../src-tauri/bin/texconv.exe`, MIT, licence next to it). This crate owns the
DDS header, the process call and the asset logic around them.

## Build and test

```powershell
cd core-rs
cargo test            # 142 tests; skips sample-dependent ones if the file is absent
cargo test --lib      # unit tests only, no sample file needed
```

Requires a stable Rust toolchain (developed against 1.99.1). Nothing else.

The repo-wide gate is `scripts/check.ps1`. On a machine with the default
execution policy it must be launched through bypass, otherwise PowerShell
refuses to load it:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check.ps1
```

It runs frontend types, the Rust build, `cargo test` for this crate, the Python
core's syntax/smoke/tests, and the docs build, and ends with `all checks passed`.

## Sample data

Several integration tests read one real bundle:

```
C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig
```

Path is hard-coded in `tests/real_bundle.rs`, `tests/real_serialized.rs`,
`tests/real_typetree.rs`, `tests/real_mesh.rs`. **When it is missing those tests
print a skip line and return instead of failing**, so a fresh checkout still
goes green — you simply lose the real-data coverage until the file is back.

To restore it on another machine: it is a hot-update bundle cached by the game
(1,740,445 bytes, NEP2-encrypted, contains 4 character meshes + 1 AssetBundle).
Copy it from the old machine, or let the game download the Vepley SSR skin again
and take the file from its update cache. Do not commit it to the repo.

The texture layer has a second, stronger harness that needs no committed data:
set `EXMT_TEXTURE_ASSETS` to any Unity `SerializedFile` containing Texture2D
objects (a shipped `.assets` from any Unity 2017–2022 game works, its `.resS`
sibling must sit next to it). `cargo test --test real_texture -- --nocapture`
then checks every decodable texture for mip-chain length, mip-0 decode, and a
full store → serialize → re-read → decode round trip. Unset the variable and the
test prints a skip line instead of failing.

## Verification approach

Every layer is checked three ways, and the third is the one that matters:

1. **Unit tests** with hand-built fixtures (no sample file needed).
2. **Integration tests** against the real bundle, asserting values an
   independent implementation produced — not values this code produced.
3. **Cross-implementation diff**: dump a normalized text projection from Rust and
   from the Python core, then diff. Current results:
   - type-tree nodes: 296 nodes, line-for-line identical
   - decoded values: 211,904 leaves, zero differences (floats compared as raw
     bit patterns, strings as bytes)
   - rebuilt bundle: same byte size as the Python rebuild (1,832,463), original
     cipher head preserved, and UnityPy enumerates the same 5 objects
   - block decoders: 7,508 deliberately arbitrary BC1/BC3/BC4/BC5/BC7 blocks
     (every BC7 mode forced at least once) decode byte-identically to
     `texture2ddecoder` 1.0.6, the library the Python core calls
   - real textures: on a shipped Unity 2019.4 `.assets` with 283 decodable
     Texture2D objects (42 BC7, 154 DXT5, 3 DXT1, 84 RGBA32, streamed through
     `.resS`), the native decoder's upright RGBA hash matches UnityPy +
     `texture2ddecoder` for every one, and all 283 survive store → serialize →
     decode unchanged
   - texconv path: decoding the same 283 through DirectXTex is pixel-identical
     for all 42 BC7 textures and within ±1 for BC1/BC3 (different rounding in
     the reference decoders); every thumbnail dimension matches Pillow's
     `Image.thumbnail`. RGBA32 is the one deliberate divergence: the Python
     core swaps red and blue for it, which this path does not reproduce.
   - skinpack end to end: one spec run through both implementations on a
     synthetic UnityFS bundle built from that asset (BC1 + BC3 + BC7 + RGBA32 +
     one 2048² BC7). `meshes.json`, `geometry.json`, `groups.json` and `meta.json`
     are identical; 5/5 thumbnail dimensions match; BC thumbnails differ by at
     most 3 levels (one 2048² BC7 edge reaches 33 from FANT vs Lanczos); diffuse
     maps are exact, ±1, and mean 0.05 respectively. Wall clock: Rust 0.99 s,
     Python 3.21 s.
   - write path: rebuilding two real Unity 2019.4 SerializedFiles with no
     changes (48,528 B `sharedassets0.assets` and 19,353,096 B `resources.assets`)
     reproduces every byte; on the synthetic 43,685-object bundle a texture-only
     `modpack` built by Rust and by Python yields **identical object payloads for
     all 43,685 objects**, the patched texture decodes red in both, and an
     untouched BC7 texture hashes the same across source/Python/Rust.
   - archive layer: five raw-DEFLATE streams produced by Python's zlib (stored,
     fixed and dynamic Huffman blocks) inflate to the original bytes; an
     871-byte `np.savez_compressed` fixture (four entries) is read through the
     ZIP central directory, inflated, parsed as `.npy`, and yields the exact
     float32/uint32 arrays and UTF-32 meta string NumPy wrote.
   - mesh patch: a 73 MB bundle built from ZZZ's `globalgamemanagers.assets`
     (43,685 objects) carries `RainDropMesh` (6,520 vertices, 9,780 indices,
     streamed vertices inlined first). An `.exmtmesh` written by Python's
     `interchange.save` after shifting every position by +0.25 on X is packed by
     both implementations: **all 43,685 object payloads are byte-identical**,
     UnityPy reads both outputs with the shifted positions, `firstByte 0 /
     indexCount 9780 / firstVertex 0 / vertexCount 6520`, and the local AABB
     centre `(0.25, 0, 0)`. The core's vertex encoder was also checked against
     `mesh.encode_streams` on a second synthetic layout (192 bytes, exact).

DirectXTex (`texconv.exe`) is vendored under `../src-tauri/bin/` with its MIT
licence and bundled through `tauri.conf.json`. `EXMT_TEXCONV` overrides the
path; without the binary the `skinpack` command falls back to Python.

Tools for this live in `examples/`: `rebuild_bundle`, `dump_typetree`,
`dump_values`, `dump_textures`. They are the intended way to re-run the
differential check. `dump_textures` prints one `name|WxH|format|bytes|fnv64`
line per texture so the two implementations can be diffed with plain `Compare-Object`.

## Format facts worth not rediscovering

Each of these cost a debugging round; all are now pinned by tests.

- **NEP2 encrypts only the first 32 KiB**, and the keystream comes from the
  *ciphertext's* first four bytes. Plaintext therefore always begins with the
  literal `Unit` (the keystream cancels there). `container::encrypt` must be
  handed the cipher head you want, never the plaintext head — the latter yields
  `X = 0`, a near-zero keystream, and a file whose signature no longer reads as
  `UnityFS`. This is the single easiest way to corrupt a mod.
- **`container::build` returns plaintext.** Decrypting its output again is not a
  no-op: the near-zero keystream still flips bytes 8, 24, 40… and eats the block
  table.
- **Only the SerializedFile's fixed header is big-endian.** `unity_version`,
  `platform`, the type table, the object table and every payload follow the
  declared endianness. Reading the whole metadata block as big-endian parses
  fine and yields garbage.
- **v21+ appends `type_dependencies` after each type tree**; skipping it shifts
  every later type by four bytes.
- **Type-tree string refs use the top bit as a flag**: clear = offset into the
  type's own buffer, set = index into Unity's built-in name table
  (`src/commonstrings.rs`, 114 entries, keys are non-contiguous and must not be
  renumbered).
- **`byte_start` is relative to `data_offset`**, and object payloads are not
  byte-adjacent — a 4-byte gap is normal.
- **Vector alignment is applied once.** A vector inherits the align flag from
  its `Array` child; padding after the elements *and* again at the end shifts
  every later field.
- **The array element type is the Array child's *second* child.** `idx + 2` is
  the count node. Synthetic tests miss this when the count and element share a
  type, so the fixture uses a struct element.
- **`m_IndexBuffer` is `vector<UInt8>`**, not `TypelessData`; `ChannelInfo`
  members are `UInt8`; `m_VertexCount` is `unsigned int`. Read integers through
  `Value::as_i32`/`as_u32` (all widths) and write back with the declared type
  (`mesh::set_int`), or the writer rightly refuses.
- **Mesh stream strides are unpadded**, only stream *starts* are 16-byte
  aligned. Body: 40/20/32 with gaps, 2283 verts → exactly 210048 bytes.
- **`m_VertexData.m_DataSize` is `TypelessData`** (arrives as bytes) while
  `m_IndexBuffer` arrives as an array — accept both shapes and preserve which
  one the tree declares.
- **Texture pixels are stored bottom-up**, and the block decoders emit BGRA.
  Turning storage into an upright picture needs *both* a vertical flip and a
  red/blue swap; doing only one still produces a plausible-looking image on a
  symmetric test texture.
- **Texture2D's pixel field is spelled `image data`** (with a space). Writing
  `m_ImageData` on a type-tree dict silently adds a new key that serialization
  drops, which looks like "the replacement did nothing".
- **`m_StreamData.offset` is `unsigned int` in Unity 2019.4** and `unsigned long
  long` in other versions; read it through `as_u64`. Match the stream node by
  the path's last component: assets say `archive:/CAB-0123/CAB-0123.resS` while
  the node is `CAB-0123.resS`. A two-node bundle with a `.resS` name is the
  fallback.
- **Replacing a streamed texture inlines it** (`m_StreamData` cleared): the
  `.resS` payload is shared by every texture in the bundle, so growing it means
  relocating every offset, which is a rebuild concern.
- **`m_CompleteImageSize` covers the whole mip chain**, not mip 0: 2048² DXT1
  with 12 levels is 2,796,216 bytes, BC7 twice that. `m_MipCount` can be 0 or
  absent on odd assets; a missing count means 1.
- **Zero-sized textures exist** (render targets, disabled assets). They parse
  fine and must not abort a bundle walk; decoding one is an error.
- **The object table aligns to 4 bytes before each path ID.** When the table
  starts unaligned, skipping that byte shifts every entry and yields
  "object 256 has type 256 of 74".
- **`FileIdentifier` (external) is `string, GUID, type, path`**, not
  `path, name, GUID, type`. Reading two strings first desynchronises the tail of
  the metadata on any file with externals.
- **Class 114 (MonoBehaviour) type entries carry a 16-byte `script_id`** after
  `script_type_index`. Omitting it shifts the type table and only fails on files
  that contain scripts.
- **v20+ reference types are a real table**, each entry a full type plus class
  name, namespace and assembly strings — not a fixed class-id+hash pair. Files
  with zero ref types never notice the difference; others parse garbage.
- **texconv writes a full mip chain for DDS output unless told otherwise.**
  Decoding a single level must pass `-m 1`, or the payload carries every mip and
  a "mip 0 length" check reads the tail as pixels.
- **Unity texture rows are bottom-up; DDS is top-down.** Decode with `-vflip`
  or the whole image is upside down (symmetric test textures hide this).
- **`-w` and `-h` are independent and stretch.** Resizing with one dimension
  keeps the other at its original value, so a thumbnail needs both computed
  first (`texture::thumbnail_size`, a port of Pillow's algorithm including its
  floor/ceil-by-closest-aspect step).
- **RGBA32 is really RGBA.** The Python core treats every decoded format as
  BGRA and swaps red/blue for RGBA32 too; DirectXTex reads the DDS masks, so the
  Rust path is correct and intentionally differs there.
- **`-o` does not create its directory**, and the output keeps the input base
  name with a new extension (`in.dds` -> `in.png`). Run each conversion in a
  fresh temp directory and check the file exists.
- **DirectXTex has no Lanczos filter.** `-if FANT` is the closest windowed-sinc
  choice; pixels sit within a few levels of Pillow's LANCZOS, and the 160px
  thumbnails are visually indistinguishable.
- **`metadata_size` is the unpadded metadata length, and `data_offset` is that
  plus the 16-byte alignment** (v21: `data_offset = align16(20 + metadata_size)`).
  Writing the padded value into `metadata_size` makes an untouched rebuild differ
  by a few bytes even though it still loads.
- **The object table aligns against the file, not against the table start.**
  `objects_start + table.len()` must be 4-aligned; getting this wrong shifts the
  table by one byte and only shows up on files whose type table leaves the count
  unaligned.
- **Object payloads align to 8 bytes relative to the data section.** `byte_start`
  is relative to `data_offset`, so the padding belongs before each payload, not
  after.
- **Never re-encode the type tables.** Their string references mix offsets into
  the per-type buffer and indices into Unity's built-in name table; copying the
  parsed byte range preserves both. The same goes for external and reference-type
  tables: there is no reason to round-trip what this crate does not modify.
- **v22 header layout differs** (the modern header repeats sizes after the
  reserved bytes), and no v22 sample exists here, so `rebuild` refuses v22 and
  the shell falls back to Python for those files.
- **`np.savez_compressed` writes a data descriptor**: the local file header
  carries `0xffffffff` sizes, so entry sizes and offsets must come from the
  central directory. Reading local headers works only for archives written
  without streaming.
- **`.npy` strings are UTF-32LE** (`<U54` = 54 code points, 216 bytes), not
  UTF-8, and the header is a Python dict literal padded to a 64-byte boundary.
  Reading it as text from the first `{` to the newline is the reliable part.
- **DEFLATE has three block shapes** and all appear in real archives: stored,
  fixed Huffman, and dynamic Huffman (code-length tree, repeat codes 16/17/18).
  A decoder that only handles fixed blocks gets lucky on text and fails on mesh
  buffers.
- **`.exmtmesh` always stores indices as uint32**, even when the mesh's
  `index_format` is 0 (16-bit). Narrow after reading and reject values above
  65535; checking the dtype instead rejects every 16-bit mesh file.
- **Bone arrays arrive as uint32** in `.exmtmesh` while the vertex encoder takes
  floats. Convert after reading; values are small enough to be exact.
- **A mesh's submeshes are rebuilt, not just counted**: `firstByte`,
  `indexCount`, `firstVertex`, `vertexCount` and a per-submesh `localAABB` are
  derived from the new index buffer, and zero-count entries disappear. Unity
  culls with the per-submesh box, so it must cover only the referenced vertices.
- **Different cooks stay in Python**: if the file's `m_Channels` or
  `m_BoneNameHashes` differ from the target tree, `modpack` raises
  `NeedsPython` before writing anything. Bone remapping and channel fitting are
  implemented there and are not worth duplicating badly.

## Next steps

1. **`analyze` (GameCache) in Rust**: the multi-process library scan that
   produces `manifest.json`, `names.json`, `skins.json` and the skin packs. This
   is the last large Python-owned command.
2. **Retire the Python core** once `analyze` is ported; until then `run_core`
   falls back to it for `analyze`, `workspace`, `backup`/`restore` and mesh
   patches whose cook differs.
3. Keep `modpack`'s Python fallback as a safety net for exotic meshes
   (compressed meshes, bone remapping) even after `analyze` moves.

## Conventions

- No third-party crates. If something needs vendoring, write it.
- Errors carry the byte offset that made the input unacceptable; Unity files are
  untrusted input.
- Tests must state values from an independent source, never values this code
  produced. A test that only proves self-consistency has caught a real bug
  exactly once here — and that bug was in the other direction.
