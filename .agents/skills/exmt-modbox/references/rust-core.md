# Rust 资产核心（core-rs）

`core-rs/` 是把资产管线从 Python 迁到 Rust 的落地位置。目标：**只用 std**，不依赖任何第三方 crate，也不再 spawn Python 进程。**图像处理不写 Rust**：解码、缩放、PNG、BC 编码全部交给内置的 DirectXTex `texconv.exe`。

完整的事实清单、进度表和下一步在 `core-rs/README.md`，这里只记 agent 动手前必须知道的判断。

## 何时看这里

- 要改 `core-rs/` 下任何模块
- 要把 Tauri 的 `run_core`（当前 spawn Python）换成直接调用
- 要判断某层是否已可信、能否删对应 Python 实现

## 现在的状态

| 层 | 模块 | 状态 |
| --- | --- | --- |
| LZ4 block | `lz4.rs` | 完成 |
| UnityFS 容器 + NEP2 | `container.rs` | 完成 |
| SerializedFile v17–v22 | `serialized.rs` | 完成 |
| TypeTree 读写 | `typetree.rs` | 完成 |
| Mesh 顶点/索引 | `mesh.rs` | 完成 |
| Texture2D | `texture.rs` | 完成 |
| BC1/3/4/5/7 解码（原生） | `bc.rs` | 完成（差分/工具用，预览走 texconv） |
| DDS 封装/解析 | `dds.rs` | 完成 |
| texconv 调用 | `texconv.rs` | 完成 |
| geometry（three.js） | `skin.rs` | 完成 |
| SerializedFile 重建（v17–v21） | `serialized.rs` | 完成 |
| bundle 补丁重打包 | `asset.rs` | 完成 |
| skinpack 命令 | `src-tauri/src/skinpack.rs` | 完成，Tauri 已直接调用 |
| modpack 贴图路径 | `src-tauri/src/modpack.rs` | 完成，Tauri 已直接调用 |
| NPZ 读取（ZIP + DEFLATE + `.npy`） | `inflate.rs` / `zip.rs` / `npy.rs` | 完成 |
| modpack 网格补丁（`.exmtmesh`） | `mesh.rs` + `src-tauri/src/exmtmesh.rs` | 完成（同 cook）；异 cook 回退 Python |
| analyze（GameCache） | — | 未开始（仍 Python） |

142 个测试。`cargo test` 在 `core-rs/` 下跑；`src-tauri` 另有 9 个单测和 3 个可选
端到端测试（skinpack、modpack、exmtmesh）。`texconv.exe`（966 KB，MIT，许可证同目录）随仓库放在
`src-tauri/bin/`，并由 `tauri.conf.json` 的 `bundle.resources` 打进安装包；`EXMT_TEXCONV`
可覆盖路径，找不到时 `skinpack` 回退 Python。

贴图验收用的 oracle 分两条：
- **原生解码**（`bc.rs`，工具/差分用）对 `texture2ddecoder` 1.0.6：7,508 个任意 BC 块
  逐字节一致，283 个真实贴图的 RGBA 哈希一致。**按规范"修正"BC1 三色模式的透明黑
  会直接让差分变红**——这里要的是与 Python 输出相同。
- **texconv 路径**（预览/skinpack 用）对 Pillow：BC7 42/42 完全一致，BC1/BC3 全部 ±1；
  RGBA32 有意不同（Python 把它当 BGRA 交换了红蓝，Unity 实际就是 RGBA）。
  缩略图尺寸 283/283 与 Pillow `thumbnail` 一致；像素差异来自 FANT vs Lanczos。

## 三条硬规矩

1. **样例缺失时测试必须 skip 而不是 fail。** 四个 `tests/real_*.rs` 硬编码
   `C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig`，
   该文件不存在时打印 skip 行并 return。换机器后测试仍应全绿。texconv 相关测试
   找不到 `texconv.exe` 时同样 skip；`skinpack_real` 由 `EXMT_SKINPACK_SPEC` 开闸。
2. **验收值必须来自独立实现。** Python 核心是对照 oracle，不是被测对象。
   导出工具：`examples/dump_typetree`、`examples/dump_values`、
   `examples/rebuild_bundle`。做法是两边导同一份文本投影再 diff，
   不要拿自己产出的值当期望值。
3. **不要为了通过测试放宽 writer 的类型检查。** 类型树声明的宽度就是契约；
   写错类型应当报错，修的是调用方（用 `mesh::set_int` 保持声明类型）。

## 最容易踩的格式陷阱

- NEP2 明文头必然是字面 `Unit`；`encrypt` 要传**你想要的密文头**，传明文头会得到零密钥。
- `container::build` 返回明文，别再 `decrypt` 一次。
- SerializedFile 只有固定头部是大端，其余按声明的 endian。
- 数组元素是 `Array` 子节点的**第二个**子节点；`idx + 2` 是计数。
- vector 的 16 字节对齐只施加一次（读取尾部统一做）。
- `m_IndexBuffer` 是 `vector<UInt8>`（数组），`m_DataSize` 是 `TypelessData`（bytes）。
- ChannelInfo 字段是 `UInt8`，`m_VertexCount` 是 `unsigned int`。
- Mesh 的 stream stride 不对齐，只有 stream **起点**对齐到 16 字节。
- 贴图行序是**自下而上**，解码器输出的是 **BGRA**：翻正 = 垂直翻转 + 红蓝交换，缺一不可。
- 像素字段名是 `image data`（带空格）；写 `m_ImageData` 会多出一个键并在序列化时丢掉。
- `.resS` 路径按**最后一段**匹配节点（`archive:/CAB-x/CAB-x.resS` vs `CAB-x.resS`）；
  `m_StreamData.offset` 在 2019.4 是 u32，别的版本是 u64，统一走 `as_u64`。
- 替换流式贴图要内联并清空 `m_StreamData`，因为 `.resS` 是同包所有贴图共用的。
- 对象表每条 path ID 前有 4 字节对齐；外部引用是 `string, GUID, type, path`；
  class 114 类型条目多 16 字节 `script_id`；v20+ 的引用类型是真表（含三条字符串）。
  这四条只有在非 GF2 样例的真实资产上才会暴露。
- texconv 输出 DDS 时**默认生成整条 mip 链**；只要 mip0 时必须传 `-m 1`，否则
  "mip0 长度"检查会把 mip 尾巴当成像素。
- DDS 是自上而下、Unity 贴图是自下而上：解码必须 `-vflip`，否则图整个倒过来；
  对称测试图会掩盖这个错误。
- `-w`/`-h` **各自独立、会拉伸**：只给一个，另一个保持原尺寸。缩略图要先按 Pillow
  的 `thumbnail` 算法算好两个尺寸（`texture::thumbnail_size`，含 floor/ceil 取最近比例）。
- `-o` 指定的目录**不会自动创建**；输出名沿用输入主名换扩展名（`in.dds` → `in.png`）。
- DirectXTex 没有 Lanczos；最接近的是 `-if FANT`，与 Pillow LANCZOS 相差几个灰阶。
- `metadata_size` 是**不含填充**的元数据长度，`data_offset = align16(20 + metadata_size)`
  （v21）；把填充后的值写进去，未修改重建也会差几字节。
- object 表按**文件绝对位置**做 4 字节对齐（`objects_start + table.len()`），不是按表起点；
  对象 payload 按 data 段起点做 8 字节对齐，`byte_start` 相对 `data_offset`。
- 类型表**不要重新编码**：字符串引用混用 type 内偏移和 Unity 内置名表索引，直接原样拷贝
  解析到的字节区间才是安全的；外部表、引用类型表同理。
- v22 头布局与 v17–v21 不同，手头没有 v22 样例，`rebuild` 直接拒绝 v22 并回退 Python。
- `np.savez_compressed` 写的是 data descriptor：本地头里的 size 是 `0xffffffff`，
  必须从**中央目录**读真实大小与偏移。
- `.npy` 字符串是 **UTF-32LE**（`<U54` = 54 个码点），不是 UTF-8；头是 Python dict
  字面量并填充到 64 字节边界。
- DEFLATE 三种块都会在真实归档里出现（stored / fixed / dynamic），只实现 fixed 的
  解码器会在网格缓冲区上失败。
- `.exmtmesh` 的 indices **永远是 uint32**（即便 `index_format == 0` 是 16 位索引），
  读入后再窄化并拒绝 >65535；按 dtype 检查会拒掉所有 16 位网格。
- `.exmtmesh` 的 bones 是 uint32，但顶点编码器吃 f32；读入后转换即可。
- 子网格是**重建**的：firstByte/indexCount/firstVertex/vertexCount/localAABB 全部由新
  索引缓冲推导，零长度子网格会被丢掉；Unity 用每个子网格自己的 localAABB 做剔除。
- 通道或骨骼表不一致就是不同 cook，必须 `NeedsPython` 回退；骨骼重映射和通道裁剪在
  Python 侧，不要半吊子重写。

以上每条都在 `core-rs/README.md` 有更长的解释和对应测试名。

## 真实贴图数据怎么验

`core-rs/tests/real_texture.rs` 由环境变量开闸：把 `EXMT_TEXTURE_ASSETS` 指向任何带
Texture2D 的 Unity `SerializedFile`（`.resS` 放在同目录），它会对每个可解码贴图检查
mip 链长度、mip 0 解码，以及「存回 → 序列化 → 再读 → 解码」逐像素不变。
不设变量就打印 skip，不失败。差分导出工具是 `examples/dump_textures`（输出
`name|WxH|format|bytes|fnv64`，可直接和 Python 侧输出比较）。

skinpack 的端到端验法（无 GF2 数据也能做）：
1. 取任意 Unity 2019/2020 的 `.assets` + `.resS`（本机用 ZZZ 的 `globalgamemanagers`，
   含 BC1/BC3/BC7/RGBA32 各一张），用 `container::build(nodes, template, false)` 造一个
   UnityFS 包，再 `container::encrypt(plain, 任意四字节)` 包成 NEP2——Python 核心能直接
   `open_bundle` 读它。
2. 写一份 spec（`meshes: []`，`textures`/`maps` 指向包内贴图），分别跑
   `python -m exmt_core skinpack spec.json` 与 `src-tauri/tests/skinpack_real.rs`。
3. 对比：`meshes/geometry/groups/meta` 必须相等；缩略图尺寸必须相等；像素允许
   FANT/Lanczos 的几灰阶差。2026-10-21 实测 Rust 0.99 s、Python 3.21 s。

写路径的验法：`EXMT_WRITE_BUNDLE=<bundle>` 跑 `tests/real_bundle_write.rs`，它会检查
「无修改重建后每个对象 payload 逐字节相同」和「只改一张贴图、其他对象不动、再解码像素不变」。
2026-10-21 用一个 43,685 对象的合成包实测：Rust 与 Python 各自 modpack 出的包，**43,685 个
对象 payload 全部逐字节相同**；`resources.assets` 与 `sharedassets0.assets` 未修改重建也逐字节相同。

NPZ 层不依赖本机游戏数据：`tests/npz.rs` 内嵌一个由 `numpy.savez_compressed` 生成的
871 字节 fixture，DEFLATE 向量来自 Python `zlib` 的 `-15` 原始流；改这两个模块前先跑它。

网格路径（2026-10-21 实测）：ZZZ 的 `globalgamemanagers.assets` 里 `RainDropMesh` 顶点流
在 `.resS`（offset 11,400,020 / 182,560 字节），先内联再包成 UnityFS 夹具；用 Python
`interchange.save` 把所有位置 +0.25 生成 `.exmtmesh`，同一 spec 分别跑 Rust/Python
`modpack`，**43,685 个对象 payload 逐字节相同**，UnityPy 读两边顶点、子网格和 AABB 一致。
注意 UnityPy 不能直接读 NEP2 加密包，要用 `exmt_core.sf.open_bundle`。

## 换机器后恢复真实数据

样例是一个 GF2 热更新缓存 bundle（1,740,445 字节，NEP2 加密，含 4 个角色网格 + 1 个 AssetBundle）。从旧机器复制，或让游戏重新下载 Vepley SSR 皮肤后从更新缓存取。**不要提交进仓库。**
