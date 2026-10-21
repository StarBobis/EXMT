# Rust 资产核心（core-rs）

`core-rs/` 是把资产管线从 Python 迁到 Rust 的落地位置。目标：**只用 std**，不依赖任何第三方 crate，也不再 spawn Python 进程。

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
| BC1/3/4/5/7 解码 | `bc.rs` | 完成 |
| Tauri 接 Rust | — | 未开始 |

103 个测试。`cargo test` 在 `core-rs/` 下跑。

贴图验收用的 oracle 是 Python 侧的 `texture2ddecoder` 1.0.6（AssetStudio 系解码器）与
UnityPy，不是 D3D 规范：7,508 个任意 BC 块逐字节一致，283 个真实贴图的 RGBA 哈希一致。
**按规范"修正"BC1 三色模式的透明黑会直接让差分变红**——这里要的是与 Python 输出相同。

## 三条硬规矩

1. **样例缺失时测试必须 skip 而不是 fail。** 四个 `tests/real_*.rs` 硬编码
   `C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig`，
   该文件不存在时打印 skip 行并 return。换机器后测试仍应全绿。
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

以上每条都在 `core-rs/README.md` 有更长的解释和对应测试名。

## 真实贴图数据怎么验

`core-rs/tests/real_texture.rs` 由环境变量开闸：把 `EXMT_TEXTURE_ASSETS` 指向任何带
Texture2D 的 Unity `SerializedFile`（`.resS` 放在同目录），它会对每个可解码贴图检查
mip 链长度、mip 0 解码，以及「存回 → 序列化 → 再读 → 解码」逐像素不变。
不设变量就打印 skip，不失败。差分导出工具是 `examples/dump_textures`（输出
`name|WxH|format|bytes|fnv64`，可直接和 Python 侧输出比较）。

## 换机器后恢复真实数据

样例是一个 GF2 热更新缓存 bundle（1,740,445 字节，NEP2 加密，含 4 个角色网格 + 1 个 AssetBundle）。从旧机器复制，或让游戏重新下载 Vepley SSR 皮肤后从更新缓存取。**不要提交进仓库。**
