# Unity / Unreal 通用模型替换 Mod 工具：可行性评估

> 目标设定：不修改游戏逻辑，只拦截"模型加载/渲染"环节，换成自己的模型。
> 要求：资源级（非文件路径级）的哈希识别与拦截；支持热加载，避免每次重启游戏。
>
> 结论基于对 3DMigoto/SSMT 生态、Unreal `FPakPlatformFile` 官方 API、UE4SS 的公开资料调研。
> 本文是结论性评估，不是调研流水账。引用来源见文末。

---

## 0. 结论速览

| 问题 | 结论 |
|---|---|
| Unity 可行吗 | **可行**。有大量实际产品先例（GIMI / SRMI / ZZMI / WWMI / EFMI 等）。 |
| Unity 通用吗 | **工具层通用，资产层不通用**。每个游戏需 per-game profile，这是原理性成本。 |
| Unreal 可行吗 | **可行，且在你的需求上明显更优**。 |
| Unreal 通用吗 | 同样存在天花板，但**瓶颈位置不同**：从"逆向工程问题"转移为"资产管线问题"。 |
| 热加载 / 免重启 | **Unreal 架构性占优**（VFS 挂载 + 路径覆盖优先级）；Unity 需自行 hook，但同样做得到。 |

**一句话**：Unity 的通用性瓶颈在"**能不能拦到、能不能正确改**"，Unreal 的瓶颈在"**能不能 cook 出引擎接受的资产**"。后者的工具链是官方提供且可工程化的——这是本质差别。

---

## 1. Unity 侧：拦截阶梯

Unity 没有单一的"加载模型函数"。从磁盘到 GPU 是一条阶梯，选哪一级决定语义能力与通用性：

| # | 拦截点 | 拿到的信息 | 语义能力 | 跨版本 | 跨图形 API | 难度 |
|---|---|---|---|---|---|---|
| L1 | `SerializedFile::ReadObject(pathID)` | 原始序列化字节 + TypeTree | 全资产类型 | 差 | 无关 | 极高 |
| L2 | `Object::Produce` 出口 | 半成品原生对象 | 中 | 差 | 无关 | 高 |
| **L3** | **`Mesh::AwakeFromLoad` / `UploadMeshData`** | **`Mesh*` + CPU 顶点数据** | **完整** | 中 | 无关 | 高 |
| L4 | 托管 `Mesh.SetVertices` / `ApplyAndDisposeWritableMeshData` | 完整语义 | 完整 | 好 | 无关 | 低 |
| L5 | `GfxDevice::UploadMeshData` → `CreateBuffer` | 顶点/索引缓冲字节 | 无 | 好 | 每 API 一套 | 中高 |
| **L6** | **`DrawIndexed` + buffer 绑定（3Dmigoto 路线）** | **只有原始 stream** | **无** | 极好 | 每 API 一套 | 中高 |

**"渲染管线之前"有两种读法**：
- 指 Unity 渲染管线（Built-in/URP/HDRP）之前 → 落点 **L3/L4**（原生对象层）
- 指 GPU 管线之前（3Dmigoto 做法）→ 落点 **L5/L6**（渲染管线的**下游**）

**推荐主后端 = L3**，理由：语义完整（可改顶点数、可重算 bounds、可同步骨骼/BlendShape）、与图形 API 无关、不破坏游戏逻辑（游戏仍持有自己的 Mesh 实例）。
**L6 作为兜底**：应对 GPU-resident mesh、Unity 6、以及 L3 定位失败的情况。

---

## 2. Unity 上"纯 3Dmigoto 路线"的四个特有障碍

3Dmigoto 在 D3D11 上成立的前提是：input layout 语义固定、buffer 绑定 = 资源身份、draw call 无隐藏状态。Unity 三条全破：

### 2.1 顶点布局 per-mesh 变化
`VertexDataLayout` + channel（`Position/Normal/Tangent/Color/TexCoord0..7/BlendWeights/BlendIndices`），每通道 format/dimension 由导入设置决定。`Mesh Compression: High` 会量化成 `UNorm8`，字节全变。且从 `IASetInputLayout` 无法反推通道语义。

### 2.2 缓冲区不一定是资产，可能只是内存池一角 —— **已获实证**
Unity 会把多个 mesh 打包进同一个大 buffer，用 `byte_offset` 区分。终末地（Endfield）某次版本更新后**所有 Mod 全部失效**，原因正是这个。

3DMigoto 为此**专门新增了 `track_region_hashes`**：
- IB：按 `IndexCount` 确定区间，哈希实际使用范围
- VB（非索引绘制）：哈希完整引用区间
- VB（索引绘制，无显式 VertexCount）：按 IndexCount 约 15% 估算顶点数，哈希该区间前约 50%

配套新增 `track_implicit_index_buffers`（记住上次 `IASetIndexBuffer` 的绑定）、`overlay_buffer_hash_lifetime`、`allow_buffer_resize`（配合 `override_vertex_count` + `override_byte_stride` 做"顶点数上限提升"）。

**这条实证极其重要**：它证明"缓冲区哈希 ≠ 资产身份"不是我推测的理论风险，而是已经打崩过一次整个生态的真实故障。

### 2.3 实例化 / GPU 驱动绘制导致对象身份不可判定
`DrawIndexedInstanced` 下同一 draw 覆盖多对象，per-instance 差异在另一个 buffer，API 层拿不到实例语义。HDRP 的 GPU-driven culling 更彻底：draw 参数由 GPU 填入 indirect 参数 buffer，**CPU 侧读不到顶点数据**。

### 2.4 GPU 蒙皮 / BlendShape 使"顶点流 ≠ 模型"
- GPU skinning：buffer 里是**绑定姿态**网格，骨骼矩阵在 cbuffer
- BlendShape：顶点着色器用**顶点 ID 索引**结构化缓冲做偏移叠加

后果：替换模型一旦**顶点数/顺序**变化 → 蒙皮错乱、BlendShape 全废、UV/lightmap 索引错位。只在"同拓扑同布局"下干净。

SSMT 文档明确区分 **CPU-PreSkinning / GPU-PreSkinning**：前者"也适用于部分使用 Unity CPU-PreSkinning 的游戏"，后者"需要单独适配"。这正是该障碍的工程化表述。

---

## 3. Unity 哈希身份设计

**核心原则：不在字节层哈希，在"逻辑资产"层哈希。**

字节会被 `Mesh Compression`、平台、版本任意改变；逻辑资产不会。

```text
弱键（快，索引用）：(name, vertexCount, indexCount, layoutHash, subMeshCount)
中键（主键，匹配）：layoutSignature = [(channel, format, dimension, stream, offset)...]
                   规范化顶点流 = 逐顶点按语义顺序反量化成 float
                   fingerprint = H(layoutSignature ‖ 规范化顶点流 ‖ 索引流)
强键（人工校验）：参考渲染图 / 包围盒 / 拓扑统计
```

**反量化是跨游戏、跨压缩设置复用的关键**：`VertexDataLayout` 给出每通道 format，`UNorm8/SNorm16` 解码规则确定，解码成 float 后同一模型在不同游戏可得到一致指纹。这是 3Dmigoto shader hash 的等价物。

**免费的强身份别浪费**：经由 il2cpp 元数据可得 `(AssetBundle 名, 资源名, pathID)`，再用托管对象 `m_CachedPtr` 映射到原生 `Mesh*`。"人类可读稳定身份" + "内容寻址抗改名指纹"组合优于任何单方案。

---

## 4. Unreal 侧：为什么它在"热加载"上架构性占优

### 4.1 官方内置的资产覆盖机制
UE 的 `FPakPlatformFile` 是 VFS 层，pak 按 **mount order** 分层，后挂载者覆盖先挂载者。关键 API（UE 5.8）：

| API | 用途 |
|---|---|
| `Mount(InPakFilename, PakOrder, InPath, bLoadIndex, OutEntry)` | 运行时挂载，`InPath` 可指定挂载点 |
| `Mount(FPakMountArgs, FIoStatus*, FPakListEntry*)` | 新版重载，含 IoStore 状态 |
| `Unmount(InPakFilename)` | **运行时卸载存在** |
| `MountAllPakFiles(PakFolders[, WildCard])` | 批量挂载 |
| `GetMountedPakFilenames` / `GetMountedChunkIds` / `RefreshPakChunkIndicies` | 挂载状态查询与刷新 |
| `ReloadPakReaders()` | 重建所有 pak reader |
| `MakeUniquePakFilesForTheseFiles` | **从文件列表生成内存 pak** |
| `HandleMountCommand` / `HandleUnmountCommand` | 运行时控制台命令 |
| `SetMountStartupPaksWildCard` | 覆盖启动扫描通配符 |
| `GetPakCustomEncryptionDelegate` / `GetPakSigningFailureHandlerData` | 加密与签名失败处理委托 |
| `bLookLooseFirst` (`-lookloosefirst`) | **松散文件优先于 pak** |

注意 IoStore 已并入同一类：`IoDispatcherFileBackend`、`bMountFailOnUtoc`、`ForeachPackageInIostoreWhile`、`GetFilenamesFromIostoreContainer`。

**官方建议**：不要直接调 `Mount`，用 `FCoreDelegates::OnMountPak` 委托。

### 4.2 UE 的"万能钩子"
`UObject::ProcessEvent` 是 blueprint 函数与多数 `UFUNCTION` 的汇聚点，hook 它即可拦截大量逻辑——等价于 Unity 的 Harmony，但**无需 IL2CPP dumper + 模式扫描**。UE4SS 即是此路线的成熟实现。

另有完整反射：`TObjectIterator<UStaticMesh>` 可直接枚举全部静态网格，**无需任何 native hook**。

### 4.3 运行时换模型是公开 API
`UStaticMeshComponent::SetStaticMesh` / `USkeletalMeshComponent::SetSkeletalMesh` 是引擎内建、受支持的运行时操作，内部负责 bounds/渲染状态/物理状态重建。对照 Unity：L3 需要自己写原生 Mesh API 调用序列。

---

## 5. 热加载的真实语义（分项给答案）

| 能力 | Unity | Unreal |
|---|---|---|
| 运行时加载新模型资产 | 可（`AssetBundle.LoadFromFile`） | 可（`Mount` + registry 扫描） |
| **替换游戏原有资产的路径级覆盖** | **不可，需 hook** | **可，凭 mount order 优先级** |
| 运行时换掉已加载对象上的模型 | 可（原生 Mesh API 原地改） | 可（`SetStaticMesh`） |
| 改蓝图逻辑 | N/A | 可（`ProcessEvent` / UFunction hook） |
| 改非 UFUNCTION 的原生 C++ | 需 inline hook | 需 inline hook（Shipping 会内联/剥离） |
| 卸载并回滚 | 可（换回原 Mesh 内容） | `Unmount` 存在，但 `UPackage` 缓存使"同路径重载"不可靠 |

**关键差异**：Unity 的 AssetBundle 是"**显式加载的资产容器**"，UE 的 pak 是"**带优先级叠加的 VFS 层**"。所以：
- Unity 适合"加载我自己的额外资产，再换掉引用"
- UE 天然适合"**按路径覆盖游戏自己的资产**"——这正是本需求想要的形态

**结论**：免重启在两边都做得到；但"不写 hook 就能覆盖游戏原有资产"只有 UE 提供。

---

## 6. 别以为 UE 是白送的：它的真实代价

以下均有公开实证，不是理论担忧。

### 6.1 AOB 签名维护成本依然存在
UE4SS 官方原话：

> "The goal of UE4SS is not to be a plug-n-play solution that always works with every game. The goal is to have an underlying system that works for most games. **You may need to update AOBs on your own.**"

且 UE4SS 面向 UE 4.7 至 5.8，每版本需维护，采用 proxy DLL（默认 `dwmapi.dll`）注入——与 3Dmigoto 同构。这与 Unity 的"每大版本重做签名"是同一类税。

### 6.2 挂载 ≠ 资产可用（最常见故障）
`Mount` 只让文件可读，**AssetRegistry 不会自动发现**。必须二选一：
- `IAssetRegistry::ScanPathsSynchronous({"/Game/ModPath/"}, true)`
- 或加载 pak 内打包的 `AssetRegistry.bin`

官方推荐的更优路径：走 chunk 体系（`UAssetManager::LoadPrimaryAssets`），挂载 + 注册一步到位。

### 6.3 Mount point rebasing 是第一号坑
pak 内烘焙了 MountPoint（`UnrealPak -list` 可见，形如 `../../../ProjectName/`）。从**别的工程** cook 出来的 pak 挂载点不匹配，必须手动 rebase。社区实现：

```cpp
FPakFile pak(pakPlatformFile->GetLowerLevel(), *file, bSigned);
FString mountPoint = pak.GetMountPoint();     // 取出
// 修改后重新挂载
pakPlatformFile->Mount(*file, readOrder, *mountPointFinal);
// 并注册挂载点 + 通知 registry
FPackageName::RegisterMountPoint(*MountPoint, *AbsoluteContentFolderPath);
assetRegistry.AddPath(MountPoint);
```

另有历史坑：`/Game/` 是 `Content/` 的引擎内部别名（"In the bowels of the engine it still says Game"），路径写法错误会导致 `LongPackageNameToFilename failed to convert`。

### 6.4 cook 设置必须完全一致，否则材质损坏
社区原话：

> "Make sure the project you package from does have the **EXACT** same settings (at least for rendering). Otherwise you will get problems with your assets (materials)."

这把"资产层不通用"从推测变成了实证，且比预期更严格——不只是引擎版本，**渲染设置也必须对齐**。

### 6.5 Event Driven Loader 时序坑
在打包版本中挂载 mod pak 曾导致崩溃，社区解法是把插件 Loading Phase 从 `Default` 改为 `PreEarlyLoadingScreen`，或在项目设置里禁用 Event Driven Loader。说明挂载**时机**敏感。

### 6.6 依赖完整性
pak 内资产若有依赖（如角色蓝图依赖骨骼网格），依赖必须同 pak 提供且已注册进 registry，否则报 `Found N dependent packages` 后加载失败。

### 6.7 覆盖同名资产的语义未获官方明确保证
社区对"mod 提供与游戏同路径资产时，是加载顺序决定还是冲突"的提问，至今没有权威答复。机制上 mount order 决定优先级，但**官方未把"覆盖内置资产"作为承诺用例**，需实测验证。

### 6.8 其他
- **pak 签名/加密**：`bEnablePakSigning` + AES key + `bEncryptedIndex`。绕过方式是在 pak 验证层**之上**插 `IPlatformFile` 包装，让自己的内容根本不走签名路径。
- **反作弊**：EAC/BattlEye 在 UE 游戏上同样普遍。
- **IoStore（UE5）**：`.utoc`/`.ucas` 取代单体 pak，拦截面不同（`IIoDispatcherBackend` 是干净落点）。
- **Nanite**：几何以 cluster 存在于 GPU，CPU 侧可能没有可修改的完整 LOD——即 UE 版的"GPU-resident mesh"问题，与 Unity 6 风险对应。

---

## 7. 对比总表

| 维度 | Unity | Unreal |
|---|---|---|
| 资产覆盖/优先级 | 无内建机制，需自己 hook | **内建 pak mount point + mount order** |
| 反射 | IL2CPP 元数据可得；原生 C++ 无符号 | **完整 UPROPERTY/UFUNCTION 反射 + TObjectIterator** |
| 逻辑 hook 落点 | Harmony（托管）/ 原生 inline hook | **`UObject::ProcessEvent` 单一汇聚点** |
| 运行时换模型 | 原生 Mesh API 原地改，或图形 API 换 buffer | **`SetStaticMesh`/`SetSkeletalMesh` 公开 API** |
| 运行时挂载新内容 | `AssetBundle.LoadFromFile`（显式容器） | **`FPakPlatformFile::Mount`（VFS 层）** |
| 免重启 | 可行 | **成熟** |
| 版本耦合 | 每 Unity 大版本需重做签名 | 每 UE 小版本 + cook 版本需匹配 |
| 内容兼容性 | 顶点布局 per-game profile | **Skeleton / PhysicsAsset / 渲染设置 / cook 版本匹配** |
| 官方支持 | 无 | **有（Plugin / GameFeature / pak 均为官方设计）** |
| 资产管线工具链 | 社区（AssetStudio/AssetRipper/UABPy） | **官方（UnrealPak / cook / UnrealBuildTool）** |
| 反作弊 | mhyprot 等 | EAC / BattlEye |

---

## 8. 工程建议

**若选 Unity**：
1. 主后端做 L3（原生 Mesh 对象层），L6 做兜底
2. 身份用三级指纹，规范化到逻辑层
3. 接受 per-game profile 是固定成本（参考 SSMT 的游戏配置模式）
4. 热加载走"运行时 LoadFromFile + 换 Mesh 内容"

**若选 Unreal**：
1. **先验证能不能 cook**——这是项目的真实瓶颈，不是 hook
2. 路径覆盖靠 pak mount order；热加载靠运行时 Mount + registry 扫描
3. 逻辑用 `ProcessEvent` hook（UE4SS 路线），几何用 `SetStaticMesh`
4. 建立 AOB 签名库 + per-game config（对标 UE4SS 的 custom game configs 模式）
5. 内容管线尽可能复用官方工具链，把适配成本压到配置层

**通用结论**：任意引擎下，"**工具通用 + 资产按游戏适配**"是无法绕过的形态。声称能免掉资产适配的方案，一定在某个环节偷了语义。

---

## 引用来源

- SSMT 支持游戏列表 / CPU-GPU PreSkinning 说明：https://perxenic-acid.github.io/SSMT4-Documents/newbie/ssmt/SSMTSupportedGames/SSMTSupportedGames.html
- 终末地 `track_region_hashes` 等 d3dx.ini 变更（Unity buffer 打包实证）：https://perxenic-acid.github.io/SSMT4-Documents/games/efmi/D3dxIniChanges/D3dxIniChanges.html
- UE4SS 文档（AOB 维护声明、ProcessEvent、Blueprint Modloader、proxy DLL）：https://docs.ue4ss.com/
- `FPakPlatformFile` API（Mount/Unmount/lookloosefirst/IoStore）：https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/PakFile/FPakPlatformFile
- `FPakPlatformFile::Mount` 重载与官方建议：https://dev.epicgames.com/documentation/en-us/unreal-engine/API/Runtime/PakFile/FPakPlatformFile/Mount
- Epic 官方运行时加载 Content/Pak 指南：https://dev.epicgames.com/community/learning/knowledge-base/D7nL/unreal-engine-primer-loading-content-and-pak-files-at-runtime
- Mount 后资产找不到（registry 不同步）：https://bugnet.io/blog/fix-unreal-pak-mounted-but-asset-not-found
- 社区实战：mount point rebasing、cook 设置一致性、EDL 时序：https://forums.unrealengine.com/t/load-assets-from-mounted-pak-files/130124
- GIMI（Unity 顶点缓冲哈希替换产品先例）：https://github.com/SilentNightSound/GI-Model-Importer
- ZZ-Model-Importer（绝区零）：https://github.com/leotorrez/ZZ-Model-Importer
