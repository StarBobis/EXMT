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
| Texture2D semantics | **not started** | — |
| BC1/BC3/BC4/BC5/BC7 decode | **not started** | — |
| Tauri calls Rust instead of Python | **not started** | — |

`86 tests`, all green. See "Verification" below for what they actually prove.

## Build and test

```powershell
cd core-rs
cargo test            # 86 tests; skips sample-dependent ones if the file is absent
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

Tools for this live in `examples/`: `rebuild_bundle`, `dump_typetree`,
`dump_values`. They are the intended way to re-run the differential check.

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

## Next steps

1. **Texture2D** (`class 28`): `m_StreamData` (path/offset/size, usually a
   sibling `.resS`), `m_Width`/`m_Height`, `m_TextureFormat`, `m_MipCount`,
   `m_CompleteImageSize`, `m_ImageData`. Acceptance: image data round-trips
   byte-exact and dimensions/format match the Python oracle.
2. **BCn decode**: BC1/BC3/BC4/BC5/BC7 to RGBA8. Acceptance: compare against
   UnityPy's `image` output for the same texture, pixel for pixel.
3. **Wire into Tauri**: `src-tauri` depends on this crate by path; replace the
   `run_core` Python subprocess in `src/lib.rs` with direct calls.
4. **Retire the Python core** once Rust covers texture work too.

## Conventions

- No third-party crates. If something needs vendoring, write it.
- Errors carry the byte offset that made the input unacceptable; Unity files are
  untrusted input.
- Tests must state values from an independent source, never values this code
  produced. A test that only proves self-consistency has caught a real bug
  exactly once here — and that bug was in the other direction.
