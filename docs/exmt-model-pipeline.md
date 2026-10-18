# EXMT Modbox · 模型流水线（GF2）

> 目标：把游戏里的角色网格导出 → 在 Blender 里改 → 打包回去，且不改骨架。
> 当前状态：**全链路跑通**，已用「维普蕾 SSR 作战服 → 宿舍装」在游戏大厅验证。

## 1. 链路总览

```
GF2 bundle ──exmt_core──▶ .exmtmesh ──Blender 插件──▶ 编辑 ──▶ .exmtmesh
                                  ▲                                │
                                  └──────── exmt_core pack ◀────────┘
                                                  │
                                          替换后的 bundle
```

- 容器读写：`core/exmt_core/bundle.py`（UnityFS；前 32KB XOR、LZ4 块、blocksInfo）
- 对象读写：`core/exmt_core/sf.py`（UnityPy 打开 SerializedFile；`o.read_typetree()` / `o.save_typetree()` / `env.file.save()`）
- 网格编解码：`core/exmt_core/mesh.py`（按 `m_Channels` 还原 3 条顶点流）
- 交换格式：`core/exmt_core/interchange.py`（`.exmtmesh` = npz）
- 重定向：`core/exmt_core/retarget.py`（骨骼名哈希优先 + 最近绑定位置兜底）
- Blender 侧：`C:\Dev\EXMTBlender`（`mesh_io.py` + 面板/操作符）
- 应用侧：`src/views/ModelsView.vue` + Rust 命令 `run_core`

## 2. 关键事实（实测）

| 项 | 值 |
| --- | --- |
| 网格 bundle | `872f1c4f93f07d6ba65b103ae5d1a337`（4 个 Mesh：body / cloth / plush / hair） |
| 贴图 bundle | `287dc73c9302244853e6d72689dd51bb`（cloth01/cloth02 d/n/rmo + plush） |
| cloth 顶点 | 18,194 顶点 / 68,121 索引 / 4 个子网格 / 126 骨 |
| body | 2,283 / 11,007 / 59 骨 |
| 顶点流 | s0: pos f32x3, normal f32x3, tangent f32x4（stride 40）<br>s1: color f32x4, uv0 f16x2（stride 20）<br>s2: weights f32x4, bones u32x4（stride 32）|
| 流对齐 | **每条流的起始按 16 字节对齐**，stride 本身不补齐（18,194 顶点 = 727,760 + 8 pad + 363,880 + 0 + 582,208 = 1,673,856 ✔︎）|
| 索引 | `m_IndexFormat=0`（u16）；`m_IndexBuffer` 是 **TypelessData（字节）**；`SubMesh.firstByte` 是**字节**偏移 |
| 类型树 | 该包内嵌 2 个类型（Mesh/AssetBundle），所以 typetree 读写可用 |

## 3. 改一个 Mesh 的完整步骤

1. `o = bundle.mesh(name)`；`tree = o.read_typetree()`
2. `md = mesh.from_tree(tree)` → numpy 数组
3. 改数组（顶点数可变，但**通道布局保持不变**，否则 shader/材质绑定会错）
4. `mesh.to_tree(tree, md)` → `o.save_typetree(tree)`（UnityPy 会按类型树重新序列化，16 字节对齐由它处理）
5. `env.file.save()` → 新 SerializedFile（对象表/偏移自动重算）
6. 把新 SF 塞回 node[0]，`bundle.build(nodes, info)` → 只加密前 32KB → 写盘

**自校验**：`read_typetree` → `save_typetree` 对 1.8 MB 的 cloth Mesh **逐字节一致**；
SF 重建后仅 metadata 多 4 字节（对象表对齐），游戏可以正常加载。

## 4. 骨骼重定向（换衣服的关键）

两套骨架的骨骼 **名字哈希（CRC32）有 24/43 重合**，所以：

1. 先按 `m_BoneNameHashes` 精确配对（最可靠）
2. 剩下的骨骼按绑定矩阵求逆后的**世界位置最近**配对
3. 顶点权重映射后合并同名、取前 4、归一化

只做"最近位置"不做哈希时，袖子/手部会被绑到相邻的错误骨骼，表现为**拉长的飘带**
（已验证：加哈希匹配后明显收敛）。

## 5. 贴图替换（同格式同尺寸 → 直接搬字节）

`c_VepleyDorm_slg_cloth_d` 与目标的 `cloth01_d/cloth02_d` 都是 **DXT1 2048² 12 级 mip = 2,796,216 字节**，
所以不需要 DXT 编码器：直接从源包 `.resS` 读出整段、覆盖目标包 `.resS` 对应偏移，重建块即可。

`exmt_core copy-texture <dst.bundle> <dst_tex> <src.bundle> <src_tex> <out>` 就是这个操作。
尺寸/格式不一致时会报错（那就要接 BC1/BC7 编码器）。

## 6. Blender 插件

- 路径 `C:\Dev\EXMTBlender`，在 Blender 里作为普通插件安装（`Edit > Preferences > Add-ons > Install`）
- 面板：`3D View > 侧栏(N) > EXMT`
  - 导入 `.exmtmesh` → 生成网格 + 126 根骨骼的 Armature + 顶点组 `b0…b125`
  - 导出 `.exmtmesh` → 读当前物体的顶点/UV/顶点色/权重（按顶点组名解析骨骼索引）
  - 从 Bundle 提取并导入 / 一键导出并打包（调用核心 CLI）
- 插件偏好设置里可改「核心目录」与「Python 解释器」
- 无 GUI 也能跑：`blender --background --python you.py`，直接 `import mesh_io`

## 7. 验收（2026-10-18）

- 模型：`c_VepleySSR01_slg_cloth_lod0` ← `c_VepleyDorm_slg_cloth_lod0`（5,345 顶点，重定向到 126 骨）
- 贴图：`cloth01_d` / `cloth02_d` ← `c_VepleyDorm_slg_cloth_d`（DXT1 2048²）
- 结果：大厅里维普蕾穿着**宿舍装**（灰黑+金色），站姿与蒙皮正常，游戏日志 0 报错
- 截图：`C:\DevCache\tauri-logs\shot_162326.png`

### 已知问题

1. 右手附近有少量**悬浮碎片**：宿舍装里有些部件（手套/挂件）的权重对应的骨骼在 SSR 骨架上没有
   合适对应，被绑到了邻近骨骼，拉伸成片。改进方向：给"没有同名且距离 > 阈值"的骨骼加惩罚项，
   或把这些顶点按最近顶点从身体网格继承权重。
2. 法线/切线在 Blender 往返时被重算（位置误差 ~2e-7，法线可能差 1.5），对新衣服无影响；
   若要"只改颜色不改形状"的原网格往返，需要把法线写回自定义拆分法线。

## 8. 未决问题

1. **外部模型导入**：本机 Blender MCP 的资产库（PolyHaven/Sketchfab/PolyPizza）与生成器全部关闭。
   已实测可用的外部来源只有 poly.pizza 的直链（页面里能抓到 `https://static.poly.pizza/<uuid>.glb`，
   无需登录）。但搜到的"衣服"要么是**平铺的 T 恤纸片**（`T-Shirt` 3,716 顶点，厚 0.17，无贴图），
   要么是**整只角色**（`Hoodie Character` / `Jumper`），没有一件是可直接穿上身的衣服。
   管线本身对任意网格通用：Blender 里对齐 + `传递权重并绑定骨架` 即可。
2. **BC7 贴图**：目前只做 DXT1 的整段替换；法线/RMO 是 BC7，需要编码器才能改。
3. **多材质**：替换后的网格只有 1 个子网格（1 个材质槽），原衣服的 4 个槽位未使用；
   若要保留多材质，需要在 Blender 里分材质并在导出时写回多个 submesh（格式已支持）。

## 9. glTF 导出（新增）

`python -m exmt_core glb <bundle> <out.glb> [mesh ...]`

- 一次导出包内**全部网格 + 各自骨架**：4 个 skinned mesh、4 套 skin、243 个节点，1.47 MB
- 骨架用 `inverseBindMatrices` = 游戏里的 bindpose；关节节点平铺（glTF 允许，因为每个关节节点自带世界静止矩阵）
- Blender 5.2 直接 `import_scene.gltf` 验证：4 个网格顶点数 2,283 / 5,345 / 358 / 5,844，
  骨骼数 59 / 126 / 18 / 32，顶点组与 Armature 修改器全部正确
- 应用里"导出模型"按扩展名分流：`.glb` 走 glb 命令，`.exmtmesh` 走 extract

## 10. 外部网格绑定（新增）

Blender 插件新增 `传递权重并绑定骨架`：

1. 先按来源网格的顶点组名在目标网格上建同名组（**DataTransfer 只写入已存在的组**，这是最容易漏的一步）
2. `DATA_TRANSFER` 修改器，`data_types_verts={'VGROUP_WEIGHTS'}`、`vert_mapping='POLYINTERP_NEAREST'`、`layers_vgroup_select_src='ALL'`
3. 应用修改器（`modifier_apply`）
4. 把目标网格 parent 到来源网格的 Armature，并加 `ARMATURE` 修改器

实测：96 顶点圆柱 → 126 个顶点组、96/96 顶点有权重、父级为骨架。

## 11. ✅ 验收：网上下载的夹克替换维普蕾衣服（2026-10-18）

### 11.1 素材

| 项 | 值 |
| --- | --- |
| 模型 | poly.pizza **Jacket**（CC0，Polygonal Mind）`https://poly.pizza/m/eeKk3ObCbf` |
| 直链 | `https://static.poly.pizza/829bbd95-9d98-4093-97bd-14fd9717241c.glb`（370 KB，无需登录） |
| 网格 | 458 顶点 / 592 三角面，单网格，自带 1024² 贴图（深蓝 + 金色三角纹章） |
| 贴图 | 从 GLB 的 bufferView 取出 PNG（1024×1024） |

### 11.2 处理链

1. Blender 导入 glb → `mesh_io.export_mesh` 导出 `.exmtmesh`（GLB 的 RootNode 带 **100 倍缩放**，
   导出走 `matrix_world` 自动烘焙）
2. numpy 里做 Unity 空间拟合：Blender Z-up → Unity Y-up、袖轴从 Z 转到 X、
   **按袖展缩放到 0.95**、肩线高度放在 y=1.20、袖口绕肩线**平滑衰减下垂 55°**
3. 权重：对每个夹克顶点取布料网格最近 4 个顶点，按距离反比加权后合并到 4 根骨
4. `exmt_core pack` 打进 `872f1c4f…`（458 顶点，1 个子网格）
5. 贴图：`exmt_core set-texture` 把 PNG 缩放到 2048²、BC1 编码 + 12 级 mip（正好 2,796,216 字节），
   写入 `287dc73c…` 的 `cloth01_d` / `cloth02_d`
6. 装包 → 启动 → 大厅截图，日志 `Failed to load` / `not built with the right version` 均为 **0**

### 11.3 两个关键坑

| 坑 | 症状 | 解法 |
| --- | --- | --- |
| **glTF UV 是左上原点，Unity 是左下** | 轮廓完全正确，但颜色错位、出现大片"皮肤色碎片" | 导入外部 glTF 网格时 `v = 1 - v` |
| **贴图槽是定长**（DXT1 2048² + 12 mip = 2,796,216 B） | 尺寸对不上无法整段替换 | 自写 BC1 编码器（min/max 端点 + 4 色模式）+ 逐级 2×2 降采样生成 mip，长度必须精确相等 |

### 11.4 结果

- 截图：`docs/shots/vepley-jacket-lobby.png`
- 仍是 126 骨的原骨架（`m_BindPose` / `m_BoneNameHashes` 原样保留），只换了网格与权重索引
- 遗留：右袖末端有几片悬浮碎片（袖口顶点被绑到相邻骨骼），属于权重质量而非管线问题；
  另有 1 个子网格 → 只用材质槽 0

### 11.5 复用命令

```powershell
python -m exmt_core glb       <bundle> out.glb                # 导出全部网格+骨架
python -m exmt_core extract   <bundle> <mesh> out.exmtmesh    # 单个网格（含骨骼/权重）
python -m exmt_core pack      <bundle> <mesh> in.exmtmesh out.bundle
python -m exmt_core set-texture <bundle> <tex> png out.bundle # BC1 + mip 精确填槽
python -m exmt_core copy-texture <dst.bundle> <dst_tex> <src.bundle> <src_tex> out.bundle
python -m exmt_core retarget  <src.exmtmesh> <template.exmtmesh> out.exmtmesh
```


