# GF2 资产流水线判据（模型 / 贴图 / 动画 / 骨骼 / Mod）

> 覆盖 UnityFS 容器 → 网格 / 骨骼 / 贴图 / 动画 → Mod 打包 → Blender 插件 → 工作空间。
> 只写链路、字段、命令、阈值、坑与修法，可脱离本项目复用。
>
> **2026-10 起 Python core 已删除**：本页的 `python -m exmt_core …` 是历史命令，
> 运行时全部在 Rust（`core-rs/` + `src-tauri/src/`）。字段、阈值与坑仍然有效。

## 1. 链路总览

主链：`bundle ─extract→ .exmtmesh ─Blender→ .exmtmesh ─pack→ bundle`；补丁链：`modpack → patches/*.exmtmesh → applyMods → 游戏包`。

| 层 | 实现 | 职责 |
| --- | --- | --- |
| 容器 + 对象 | `bundle.py` / `sf.py` | UnityFS v7 读写（前 32768 B XOR、LZ4 块、blocksInfo）；UnityPy 打开 SerializedFile |
| 网格 | `mesh.py` | 按 `m_Channels` 还原 3 条顶点流 |
| 骨骼 | `skeleton.py` | 跨网格骨骼表并集 + 哈希重映射 |
| 贴图 | `texpatch.py` + `ddsinfo.py` | 只搬字节，不碰图像编码 |
| 动画 | `anim.py` + `animidx.py` | 通用 Transform 曲线解码、全库索引 |
| 打包 + 交换 | `modpack.py` / `interchange.py` | 补丁集合，一包只写一次；`.exmtmesh` = npz（数组 + typetree） |
| Blender / 应用 | `C:\Dev\EXMTBlender\{__init__,mesh_io,blueprint}.py` / `ModelsView.vue` + Rust `run_core` / `pack_blender` | 导入导出、蓝图、导出 job；贴图编码、打包与安装 |

## 2. 容器层：UnityFS

| 项 | 值 / 判据 |
| --- | --- |
| 引擎 | Unity 2019.4.40f1 / IL2CPP / Addressables |
| 加密范围 | **只加密前 32768 字节**；整文件 XOR 会 `Failed to load CAB-` |
| 密钥 | `head4 ^ "Unit"` + PAD + 位翻转 + PAD（16 字节循环） |
| SF 重建 | metadata 多 4 字节（对象表对齐），游戏可正常加载 |
| 自校验 | `read_typetree` → `save_typetree` 对 1.8 MB cloth 网格**逐字节一致** |
| typetree 可用前提 | 包内嵌该类型（抽样包 2 个：Mesh / AssetBundle）；批量列类型只解 SF 元数据前缀（60 MB 包取 86 KB，9613 包 14 s） |

改一个 Mesh 的固定步骤：`o.read_typetree()` → `mesh.from_tree()` → 改数组（顶点数可变，**通道布局必须不变**）→ `mesh.to_tree()` + `o.save_typetree()` → `env.file.save()`（对象表/偏移自动重算）→ 新 SF 塞回 `node[0]` + `bundle.build(nodes, info)` → 只加密前 32 KB 写盘。

## 3. 网格字段与顶点流

| 流 | stride | 内容 |
| --- | --- | --- |
| s0 | 40 B | position f32×3(12) + normal f32×3(12) + tangent f32×4(16) |
| s1 | 20 B | colour f32×4(16) + uv0 f16×2(4) |
| s2 | 32 B | weights f32×4(16) + bones u32×4(16) |

- `len(m_DataSize) / vertex_count` = **92**（三流之和）；顶点在 **s0 内**步长才是 **40**，顶点 i 位置在 `i*40` 而非 `i*92`。
- 每条流起点按 **16 字节对齐**，stride 不补齐。例：18194 顶点 = 727760 + 8 pad + 363880 + 0 + 582208 = 1673856 ✔
- `m_IndexBuffer` 是 **TypelessData（字节数组）**；`SubMesh.firstByte` 是**字节**偏移；`m_IndexFormat=0`（u16）。
- 量级参考：cloth 18194 顶点 / 68121 索引 / 4 子网格 / 126 骨；body 2283 / 11007 / 59 骨。
- 每子网格 `localAABB` 必须按**被引用的顶点**算，不能用 `pos[min:max+1]`（会把未引用顶点算进盒体）。
- 真 stride 判据：把**解码后的位置**去 `raw.find()`，连续偏移的差值才是 stride。

## 4. 骨骼表：哈希、并集、重映射

| 事实 | 判据 / 做法 |
| --- | --- |
| 网格只带 CRC32 哈希 | `m_BindPose` + `m_BoneNameHashes`，**无名字、无父子关系** |
| 哈希定义 | `crc32(完整 transform 路径)`，不是叶子名（`Avatar.m_TOS` 30/30 全中） |
| 每网格只带自己用到的骨 | Alva 12 个网格 3…70 根，**并集 226 根** |
| 名字来源 | `Avatar.m_TOS` 给「哈希 → 真名 + 父链」；prefab Transform 树给同一路径 |
| 名字覆盖率 | 本地 53 包带 `Avatar`、418 带 `SkinnedMeshRenderer`；Alva 自己的 prefab 不在本地 → **104/226** |
| 未知骨 | 命名 `bone_<8 位哈希>`，**不猜父级**（宁可平铺也不乱接链） |
| 名字索引 | `<cache>/bonenames.json`（50005 条，约 13 s），路径写进 `EXMTConfig.json` 的 `bone_names` |
| 导出实现 | 工作空间按哈希求并集出 `skeleton.json`，每个 `.exmtmesh` 带「局部索引 → 骨架索引」映射；Blender 只建**一副**骨架，顶点组名逐字等于骨名 |

**重映射顺序：先哈希、后短名兜底**（短名按 `"/"` 取末段）。

| 策略 | 保留权重 | 无驱动顶点 |
| --- | --- | --- |
| 只按哈希 | 69.6% | 1954 / 12734 |
| 哈希 + 短名兜底 | **85.8%** | **0** |
| 只用短名（无哈希优先） | 60.0% | — |

⚠ 60.0% 那行**不能**当作「短名无效」的证据，它是没做哈希优先的结果；顺序必须先哈希。

**代际差异**：同一网格的多份拷贝可能来自游戏两个代际——旧表 `root/Root_M/Hip_M/Hip_R/…`，新表删掉 `Hip_M`、`Hip_R`/`Hip_L` 直挂 `Root_M`。126 根里 123 根短名相同，12 个短名各对应两个哈希（全在腿链）。

**假根判据**：同一哈希在不同裁剪深度被登记多次时，「有父优先 + 取最浅裁剪」；自证 = 父子世界距离中位 0.0000 m、p90 0.0039 m。抢答会让 5 根骨变假根、200+ 根整体位移约 1 m。

## 5. 权重：四个数判据

判「权重有没有真的进文件」，任一为 0 即废数据：

1. `weights.sum(axis=1)` 全部 ≈ 1.0（行和）
2. `bones.max() < len(bone_hashes)`（索引不越界）
3. `bone_hashes` 非空、`bindpose` 存在
4. 参与加权的**不同骨骼槽数** > 0

- 往返逐位一致的两个条件：等权重的影响槽按**索引升序**排；权重和已 ≈1 时**不再归一化**（否则差 1 ULP）。
- `dropped_influences` > 0 属正常：外部模型常带 4 根以上影响，游戏只吃 4 根。
- 零权重挂件（脸 3 组、武器 21 组全 0）是**刚性挂在某根骨上**的，播动作时留在原地；判据 `used=0` → 别当蒙皮网格。
- 顶点组 126 组名与目标完全一致却 `influenced=0` → 查「有没有传 `bone_table`」，不是权重算错。

## 6. 贴图与 DDS/BC

- 后缀用途：`_d`/`_da`/`_dt`/`_dif` 漫反射（`_da`/`_dt` 带透明）、`_n`/`_nt` 法线、`_rmo` 粗糙金属遮挡、`_bt`/`_e` 自发光。
- **同格式同尺寸 → 直接搬字节**：从源包 `.resS` 读整段覆盖目标 `.resS` 对应偏移后重建块（`copy-texture`）；尺寸/格式不一致必须重新编码。
- 需要编码时用内置 `texconv.exe`（DirectXTex）：`-f <目标格式> -w -h -m <目标 mips>`，输出长度必须与 `m_CompleteImageSize` 精确相等。

| 项 | 值 |
| --- | --- |
| DXT1/BC1 2048² + 12 mips | **2796216 B** |
| BC7 2048² + 12 mips | **5592432 B** |
| 游戏贴图 | 全部正方形；尺寸/格式/mip 数与槽位**定长** |

- **导出的 PNG 不是源格式**：texconv 解 mip0 得到 8-bit RGBA（IHDR color type 6），PNG 容器无损，但像素来自 BC1/BC7 的有损解码；回打包是二次有损重编码（BC1 42.17 dB / BC7 54.94 dB），只有 `copy-texture` 的字节搬运零损失。要留原始数据就存 DDS（头 + 裸 BC payload + 整条 mip 链）。
- **源格式看 `m_TextureFormat`**：维普蕾 SSR01 的 10 个包实测 48 条贴图（11 条 `10=DXT1`，37 条 `25=BC7`）：不透明 `_d` 走 DXT1，`_n`/`_rmo`/`_spc`/`_ramp`/`_da`/脸眼/plane 走 BC7；2048² 12 mips、1024² 11 mips、512² 10 mips，字节数与上表一致。核对命令：`cargo run --example dump_textures -- <bundle>`（输出 `name|WxH|format|payload|hash`）。
- **PNG 只含 mip0**：原 12 级 mip 链不随 PNG 导出；重打包必须按目标 mips 重新生成整条链。

- **typetree 键名是 `image data`（带空格）**；写 `m_ImageData`/`image_data` 会被当新键静默丢弃（症状：包只涨 140 字节、像素没变）。必须写 `tree["image data"]` 并清 `m_StreamData`。
- **行序**：Unity Texture2D 自下而上，texconv 对 PNG 自上而下 → 每个 mip 的块行整体翻一次（不翻 10.1 dB，翻后 59.2 dB）。
- **色彩**：texconv 把 PNG 当 sRGB，输出 UNORM 会转线性（31→4、219→181）→ 固定加 `-srgbi -srgbo`（恒等）：端到端 BC1 42.17 dB、BC7 54.94 dB（最大通道差 2–4）。
- 批量读贴图用 `peek_name()`，不要 `read_typetree()`（会连 image data 一起解析：240 张 15 min → 41 s）。
- 贴图索引：只解 node0、**≤512 KiB 才可能是贴图包**、不读 `.resS` → 31869 个贴图名 / 5027 个包 / 9.4 s（全量 50 GB 不必读）。
- 重名：3824 个贴图名里 **72 个有多份拷贝**（`RampMap_Linear_RGBAHalf` 542 份）。
- **alpha 判据**：漫反射是 BC3/BC7 时 alpha 常全 0（游戏不用）→ three.js 里不要开 `alphaTest`（0.35 会 discard 掉整片表面）。
- UV 原点：glTF 左上、Unity 左下 → 导入外部 glTF 时 `v = 1 - v`。

## 7. 材质槽：决定 Unity 画几个子网格

- 导出的子网格编号 = 源模型的**材质槽号**；渲染器材质列表属于**被替换的游戏网格**（GF2 只有 4 个槽）。
- 索引 ≥ 槽位数的子网格 **Unity 直接不画**（Blender 里一切正常，极具迷惑性）。手臂/手套消失、手环浮空即此类。
- 修法：导出时把源槽**合并到目标槽位数**（`matinfo <bundle> <mesh>` 读出目标子网格数），按面数凑组；基准 8 槽 → 3 子网格、14568 面一个不少。
- 代价：合并后部位会用错材质（贴图/颜色不对）；GF2 每网格只给固定槽位数，要保真只能改渲染器。

## 8. 换装与绑定空间重定向

- `body` 槽只有头/手/脚（2283 顶点），躯干在 `cloth`（18194）→ 只换 cloth 必然缺身体，只隐藏 cloth 只剩头+手。
- 换装本质是**绑定空间重定向**：`v' = Σ w_i (GameBind_i⁻¹ · SourceBind_i) v`。最近顶点 / 最近骨骼 envelope / 拉普拉斯平滑三种都错（会炸模）。
- 骨架配对：先按 `m_BoneNameHashes` 精确配对（最可靠），剩余按绑定矩阵求逆后的世界位置最近配对，再合并同名、取前 4、归一化。只做「最近位置」袖子会变拉长的飘带（两套骨架哈希 24/43 重合）。
- 外部网格绑骨四步：① 按来源网格顶点组名在目标上**建同名组**（DataTransfer 只写已存在的组，最易漏）；② `DATA_TRANSFER` + `data_types_verts={'VGROUP_WEIGHTS'}`、`vert_mapping='POLYINTERP_NEAREST'`、`layers_vgroup_select_src='ALL'`；③ 应用修改器；④ parent 到骨架 + `ARMATURE` 修改器。
- `hide-mesh`：顶点全塌到 `(0,-10000,0)`，索引/骨骼不变 → 不再渲染（是「隐藏」不是「删除」，仍在管线里，只是没有面积）。
- `base-body`：内置素体 7682 顶点，按目标 bbox 等比缩放、脚落地、居中、切头（`y > 0.855*height`）、最近 4 顶点 kNN 继承权重；只适合做起点，关节偏硬，且只能贴 cloth 槽（126 骨全骨架）。

## 9. 动画（AnimationClip）

| 事实 | 值 / 判据 |
| --- | --- |
| 命名 | 跟随皮肤前缀：`c_<皮肤>_slg_<动作>[_N]` |
| 全库规模 | 9619 包 → **39974 clip / 2675 动画包**，索引 63 s（3.5 MB 落在 `<cache>\animindex.json`） |
| 曲线类型 | **通用 generic Transform**，不是 humanoid 肌肉；`path = crc32(变换路径)` |
| `attribute` | 1=position(3 条曲线)、2=rotation(4)、3=scale(3)、4=euler(3)，其余绑定各 1 条 |
| 索引空间 | `[0, streamedCurveCount)` 走 streamed（稀疏三次键 `((a·dt+b)·dt+c)·dt+d`），接着 `denseCurveCount` 走 dense（帧优先），剩下 constant |
| 自证 | `Σ 绑定曲线数 = 2455 = streamed 106 + dense 623 + constant 1726`（一分不差） |
| 旋转表示 | 同一根骨**只给四元数或只给欧拉**（180/78/0 双给）；欧拉是度、顺序 Z→X→Y |
| 解码校验 | 258 骨 × 61 帧，四元数模长与 1 最大差 3.2e-4 |
| 导出量级 | 30 Hz + 5 位小数：1 秒 258 骨 ≈ 0.5 MB JSON |
| 层级来源 | 同包 prefab 的 Transform 树（634 Transform + 634 GameObject）；只登记完整路径覆盖率 **0%**，按后缀哈希 258 根里 257 根拿到父 |
| 与打包 | 动作**不参与打包**（Mod 只换网格/贴图），只用于看蒙皮 |

**蒙皮必须按游戏的 deform 算，不能按 Blender 的 rest 算**：

```
pose  = (骨世界 · 游戏rest⁻¹) · matrix_local
basis = (父pose · 父matrix_local⁻¹ · 本matrix_local)⁻¹ · pose
```

- 根因一：Blender 的 `bone.matrix_local`（head+Y+roll 反推）**不等于**游戏绑定姿态，32 根里 18 根差 10–179°（纯 roll）。判据：`bone.matrix_local⁻¹ @ 游戏rest` 的旋转角 ≈0。
- 根因二：骨架骨数远少于动作驱动数（只勾身体时 32 根 vs 动作 213 根，71% 位移无处可去）。**带动作时骨架按该皮肤全部网格的并集建**（只多解骨骼表、不多导出网格）：32 → 156 根。
- 覆盖率闸门：动作条目记 `bones`/`matched`；<50% 跳过，<95% 建 Action 但警告。
- 两条链必须配套：`world` 沿 clip 父子链合成；算 basis 时父的世界矩阵取 **Blender 父**在 `world` 里的值（没有就取 rest）。
- **验收用边缘拉伸的绝对毫米数**，不是「顶点有没有动」：1.7 m 人体上中位 0.14–0.19 mm、p95 2.2–2.7 mm 才对；按 Blender rest 算 basis 退化成中位 2.5 mm / p95 18 mm（中位差 13×、p95 差 7×）。首帧≈绑定姿势会把误差全抵消，不能用首帧验收。
- Blender 5.2 没有 `action.fcurves`：`slots.new` + `layers.new` + `strips.new(type="KEYFRAME")` + `channelbag(slot).fcurves.new(...)`，并设 `action_slot`。

## 10. Mod 打包与安装

| 项 | 判据 |
| --- | --- |
| `modpack <spec.json>` | 按 bundle 分组，**一个包只写一次**；输出 `<outdir>/<bundle hash>.bundle` + `mod.json` |
| `mod.json` | `format: exmt-mod-1`，含每包哈希/字节数/网格名/来源路径 + `patches[]`（mesh / file / bundles） |
| Mod 形态 | **补丁集合**，不是打好的包：Blender 侧 `GeneratedMod/{patches/*.exmtmesh,textures/*.png,job.json}`；EXMT 打包后才有 `<hash>.bundle` + `mod.json` |
| 运行时状态 | 原始包 + **所有启用 Mod 的补丁**重算一遍（`applyMods`），状态写 `<cache>/applied.json` |
| 打包起点 | 必须是**原始字节**：`<cache>/game/bundles` → 旧备份 → 游戏目录；否则连点两次「生成 Mod」把上次改动烤进本次 |
| 目标集合 | `targets` = `byBundle ∪ texByBundle`，只被贴图碰过的包也要算，否则整包被漏 |
| 安装前 | 必须重新读一遍 `mod.json`（用户可能刚在 Blender 又生成过） |
| 冲突 | 一个包只能有一份文件，后装覆盖先装；643 个备份包里 **68 个装着多个角色的网格** |
| 多拷贝 | `mesh_bundles` / `texture_bundles` 记每个名字的所有包，每个拷贝各打一行（2544 个网格名里 240 个有多份，9.4%） |
| 卸下 | 覆盖回备份，校验 SHA256 与备份逐位相等 |

**跨包不能照抄整棵树**：同网格不同拷贝可能差 UV 格式（f16 vs f32）、`m_IsReadable`、`m_BindPose` 末位，甚至**骨骼表差 15 根**。规则：只有「通道布局 + 可读标志 + 骨骼哈希表」全同才用文件自带 tree；否则**以目标包的 tree 为底**，只写几何，骨骼索引按哈希重映射（报告 `dropped_bones`）。

**身份放在两端都稳的地方**：本体导出时给每个网格分配 `index` + `alias` 写进 `workspace.json`，Blender 端用蓝图连线决定「哪个物体接到哪个 Submesh」，不要靠物体自定义属性；贴图节点是例外——「选了图就替换」。

⚠ `basebody` 类 Mod 只存指针、不存数据，被其他包覆盖后**无法重建**：要么也录成补丁，要么安装前检查该包是否已被占用并拒绝。

## 11. 工作空间与配置

| 产物 | 位置 / 字段 |
| --- | --- |
| 工作空间目录 | `<cache>/workspace/<皮肤>__<YYYYMMDD-HHmmss>/`，每次新建不覆盖；旧版按皮肤命名的目录作为 legacy 列出 |
| `workspace.json` | `bundles`、`mesh_bundles`、`texture_bundles`、`meshes[{index, alias, file}]`、`textures[{file}]`、`animations/` |
| `spec.json` | 核心写给 Blender 侧，Blender **不依赖 App 在跑** |
| `EXMTConfig.json` | `%APPDATA%\com.exmt.modbox\EXMTConfig.json`，格式 `exmt-config-1`：`workspace_root`、`updated_at`、`workspace{id,label,dir,entry,created_at}`、`bone_names`；串行写，避免快速切换时后到的 `null` 覆盖新选择 |
| 写工作空间 | 每次写入前先删 `meshes/`、`textures/`，保证目录内容严格等于本次勾选 |
| 皮肤缓存 | `<cache>/skins/<skinKey>/{meshes,geometry,thumbs,groups,diffuse,meta}.json`；`groups` = 核心配对结果（mesh → 贴图名），旧包没有它时前端才退回 TS 规则 |
| GameCache | `{manifest,names,skins,clips,bones}.json` + `skins/<key>/` + `progress.json`；**manifest 最后写**，它的存在即「缓存完整」 |
| 代际标记 | `cache_built_at` **必须等于** manifest 的 `built_at`；整次 analyze 只取一次时间戳，别让各产物各取当前时间 |

### 11.1 子网格 ↔ 贴图：按家族，不按前缀

- **旧缺陷**：只保留 `<skinKey>_` 开头的贴图 → 家庭级 `c_Balthilde_slg_face/hair`、武器 `cw_<char>_WL`、共享 `B_body_*` 全被过滤，body/hair 显示「0 贴图」。
- **规则唯一来源** `core/exmt_core/pairings.py`（TS 孪生 `src/lib/pairings.ts` 只给旧包/按需路径用）：
  - 前缀 `c_`/`cw_` 后第一个 token 是角色；变体词 `Dorm/Fight/General/Flat/Bend/trans/vfx/uber/mat` 不进语义；`P1/P2`、`cloth1/cloth01` 保留。
  - 同 token 链 > 同 head > 直接前缀（`Suitcase_slg_missle`←`Suitcase_slg`、`_1`←`_1_plane`）> 数字兄弟（`slg_cloth`←`cloth01`）> head 组（`hair` 网格兼收 `face/eye/eyeblend`，前提是没有独立 face 网格）> 字符家族（`BastiSSR0101` 用 `BastiSSR01_*`）。
  - `cw_*` 里出现 `WL` 一律归到 `wl` 头：整把枪的 `cw_X_WL_*` 图集由 WL/Silencer/Sight/Stool 共享。
  - 没有自有贴图的 `body`/`body1` → 共享 `B_body_*`（UV 岛对齐；`face_d` 会把胸口落在黑缝上）。
  - 数字肢体兄弟（`hand2` 映射 `hand1`）同 P 组才继承；最终仍有缺口的道具按**精确 token** 匹配共享资产（`cw_MP443_WL`、`SabrinaSSR01_Pizza`、`cw_Tongyong_SmokeGrenade`），但 `body/hair/face/hip/cloth/head...` 这类通用头禁止跨角色借。
- **实测**：249 个 roster 皮肤 2551/2603 个子网格有 diffuse（98.0%）；218 个皮肤全网格覆盖，31 个只差道具/未下载资产。审计把缺口分成三类（`pairs` 的 JSON）：`missing_bug` **必须为 0**（有 diffuse 家族却没配上 = 解析 bug）；`missing_no_diffuse` = 只有 n/rmo/spc、缓存里没有 base color（当前 4 个：OTs14 机械臂）；`no_texture_in_cache` = 任何合理命名下都没有贴图（当前 48 个：NPC/未下载皮肤包）。2026-10-21 实测：bug 0 / no_diffuse 4 / no_texture 48。审计命令：`python -m exmt_core pairs <cacheDir>`。
- 材质不可用：全部 18467 个 `SkinnedMeshRenderer.m_Materials` 指向 `<external>`，`cab-*` 材质包未下载 → 只能靠命名 + UV 证据，别试图解析材质。

- 部件键 = lod 前的茎 + **lod 数字之后的全部后缀**（`_Dorm`/`_Fight`/`_trans`）；丢了后缀会让 `slg_cloth` 与 `slg_cloth_Dorm` 撞键，默认战斗服被宿舍服顶掉。
- lod 正则写成 `_lod(m?)(\d+)`（`lodm0` 是移动端抽稀），同 lod 优先非移动版（`rank=(index, mobile)`）；`emitter` 只要名字里含就跳过。
- 皮肤范围 = 名字前缀 `<skinKey>_`；`skinKey` **已经带 `c_`**，拼接时不要再补 `c_`（否则 `c_c_…` 恒为空）。贴图归组**不要**用这个前缀判断，见 §11.1。
- 「开始制作」只新建、不更新，没有删除/重命名 UI；安装只备份一次；无补丁文件的旧记录显示 `0 个补丁` 并置灰。

## 12. Blender 插件

| 项 | 值 |
| --- | --- |
| 面板 | `3D View > 侧栏(N) > EXMT` |
| 节点树 | `EXMT_BlueprintTree`：`物体`（选物体 + 选 Submesh，下拉 `0. body_lod0`）+ `生成 Mod`（N 输入槽）+ `分组`（N→1）+ `贴图`（`2048x2048 · BC7 · 12 mips` + 图片选择器） |
| 槽位 | **不带含义**，只是输入口；打包集合 = 从生成节点出发（可穿 `分组`）能走到的所有**物体节点**，目标 Submesh 取物体节点自己的 `submesh` |
| 没连线 | = 不打包（「只导入做参考」的入口）；`分组` 让 12 个物体只占生成节点 1 根线 |
| 集合 / Frame | 建 `<EXMT · <label>>` 集合；物体节点 parent 到 Frame `<label>_original`，贴图节点到 `<label>_textures`；`NodeFrame.width/height` 可写，但 Python 建出来永远 160×100，必须按子节点包围盒自算 |
| 显示自定义 NodeTree（5.2） | `space.tree_type = <bl_idname>` 然后 `space.node_tree = <tree>`；4.x 才用 `ui_type`/`edit_tree`。判据：读 `space.bl_rna.properties[...].is_readonly` |
| 法线/切线保真 | 导入把游戏法线设成 **custom split normals**，切线存网格属性 `exmt_tangent` + `exmt_tangent_w`，导出优先读回（顶点数变了才回退重算）→ p90 法线 0.01°、切线 0.00° |
| 位置残差 | 1–2e-7（`matrix_world @ v.co` 的 float32 末位）；权重/骨骼索引/UV/颜色/索引表逐位相等 |
| 生成 Mod（导出） | Blender 只写 `<workspace>/GeneratedMod/`：`patches/*.exmtmesh` + `textures/*.png` + `job.json`。`Generation.start()` 同步导出，没有 modal、没有子进程、不再读 `core_dir`（旧的 `python -m exmt_core` + `cwd` 缺失目录正是 `[WinError 267] 目录名称无效` 的来源） |
| 打包（EXMT 侧） | App 的 `pack_blender` 读 `job.json`：按目标 bundle 分组 → texconv 编码 PNG（必须 `-srgbi -srgbo`，否则 64→13 变暗）→ 复用 `modpack::run` 写 `<hash>.bundle` + `mod.json`。点「加载到游戏」时自动打包；`job.json` 不比 `mod.json` 新就跳过 |
| 编码缓存 | `<GeneratedMod>/.encode/<name>.<bundle>.bin` + 同名 `.json`，键 = PNG mtime+size + 目标 format/width/height/mips + texconv mtime；实测 14 包 + 1 张贴图冷 23.7 s、热 19.2 s |
| `exmtmesh` 版本 | 插件导出 `exmt-mesh-2`（Rust `exmtmesh.rs` 只接受 2；`physical` 缺失时按 logical `dimension` 兜底） |
| 工作空间元数据 | 新导出的 `workspace.json`：mesh 行带 `materials`（目标子网格数）与 `bone_table`（目标骨骼 hash/name，外部模型按名匹配权重用），贴图行带 `format/format_name/mips`；旧工作空间重导一次即可 |
| 重新导入 | **同名蓝图必须复用、不许 remove**（内含用户连线与选图）；`<label>_original` Frame 内物体节点按 `submesh` 重指向新网格，Frame 外节点不动 |
| 清理旧导入 | 导入**前**删上一批网格**连 mesh 数据块一起删**（否则名字被占只能叫 `.001`），回收无孩子的旧骨架；骨架按「骨骼名集合相同」复用（姿势也保住） |
| 视口说真话 | `reuse_rig()`：场景已有含这些骨骼的骨架就复用，否则按骨骼表新建；导出不读修改器，改它只为视口正确 |
| headless 测试 UI | `FakeLayout`（`label/prop/row/column/box/operator/template_ID/template_preview`）+ `FakeOp`；每个 `operator()` 的 idname 必须能对上已注册操作符 |

插件侧高频坑：`node.parent is frame` 会对自己所属的 frame 返回 False（用 `==`）；`foreach_get` 写非连续视图（`out[:, :3]`）读回全 0（用连续临时数组）；枚举 label/description 含非 ASCII 会整格乱码（全 ASCII，中文放 description）；`bpy` 存 uint32 抛 `OverflowError`（转 JSON 字符串）。

## 13. 命令速查

```powershell
# 读 / 导出
python -m exmt_core glb         <bundle> out.glb               # 全部网格 + 各自骨架
python -m exmt_core extract     <bundle> <mesh> out.exmtmesh   # 单网格（含骨骼/权重）
python -m exmt_core geoset | maps | skinfo | thumbs <spec.json>
python -m exmt_core pairs       <cacheDir>                     # 审计子网格↔贴图覆盖率
python -m exmt_core workspace   <spec.json>
# 写回
python -m exmt_core pack        <bundle> <mesh> in.exmtmesh out.bundle
python -m exmt_core modpack     <spec.json>                    # 补丁集合 → 每包一次写入
python -m exmt_core set-texture <bundle> <tex> png out.bundle  # BC1 + mip 精确填槽
python -m exmt_core copy-texture <dst.bundle> <dst_tex> <src.bundle> <src_tex> out.bundle
python -m exmt_core hide-mesh   <bundle> <mesh> out.bundle
python -m exmt_core retarget    <src.exmtmesh> <template.exmtmesh> out.exmtmesh
# 索引 / 元数据
python -m exmt_core texindex | meshidx | bonenames <src> <out.json>
python -m exmt_core animidx     <游戏目录> <cache>\animindex.json
python -m exmt_core matinfo     <bundle> <mesh>                # 目标子网格数（材质槽上限）
python -m exmt_core analyze     <游戏目录> <cache>              # GameCache 全量；另有 skinpack / packs / cache
```

Blender headless：`blender.exe --background --factory-startup --python you.py`；`--factory-startup` 自带 2 m Cube/相机/灯，脚本里先 `for o in list(bpy.data.objects): bpy.data.objects.remove(o, do_unlink=True)`。

## 14. 阈值速查

| 阈值 | 值 |
| --- | --- |
| 加密前缀 / 流对齐 | 32768 B / 每条流起点 16 B 对齐 |
| 顶点 stride | s0=40 / s1=20 / s2=32；总量 = 顶点数 × 92 |
| 贴图包判定 | node0 ≤ 512 KiB |
| 2048² 12 mips | BC1 = 2796216 B / BC7 = 5592432 B |
| 骨骼并集 | 单皮肤 226 根（Alva）；可命名 104/226 |
| 权重行和 / 动作覆盖率 | ≈1.0（否则废数据）/ <50% 跳过、<95% 警告 |
| 边缘拉伸 | 中位 ≤0.2 mm、p95 ≤2.7 mm |
| `alphaTest` | 0（BC3/BC7 的 alpha 全 0） |
| 多拷贝比例 | 网格名 240/2544（9.4%）、贴图名 72/3824 |
| 缓存规模 | 249 皮肤包 / 1.54 GB；单皮肤 1.9–7.4 s |
