# Unity / Unreal 模型替换 Mod：路线判据

> 目标形态：不改游戏逻辑，只拦截「模型加载 / 渲染」环节；按资源身份（不是文件路径）替换；尽量热加载免重启。
> 这里的结论用于选路线、判断瓶颈和预估成本，不是调研过程。

## 1. 结论速览

| 问题 | 结论 |
| --- | --- |
| Unity 可行吗 | 可行；有大量产品先例（GIMI / SRMI / ZZMI / WWMI / EFMI 等） |
| Unity 通用吗 | 工具层通用、资产层不通用；每个游戏需要 per-game profile，这是原理性成本 |
| Unreal 可行吗 | 可行，且「按路径覆盖资产 + 热加载」明显更优 |
| Unreal 通用吗 | 天花板仍在，但瓶颈从「逆向工程」转移到「资产管线」 |
| 热加载 / 免重启 | Unreal 架构性占优（VFS 挂载 + 路径覆盖优先级）；Unity 需自己 hook，但同样做得到 |

**一句话**：Unity 的瓶颈在「能不能拦到、能不能正确改」，Unreal 的瓶颈在「能不能 cook 出引擎接受的资产」。

## 2. Unity 侧：拦截阶梯

从磁盘到 GPU 是一条阶梯，选哪一级决定语义能力与通用性：

| # | 拦截点 | 拿到的信息 | 语义能力 | 跨版本 | 跨图形 API | 难度 |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | `SerializedFile::ReadObject(pathID)` | 原始序列化字节 + TypeTree | 全资产类型 | 差 | 无关 | 极高 |
| L2 | `Object::Produce` 出口 | 半成品原生对象 | 中 | 差 | 无关 | 高 |
| **L3** | **`Mesh::AwakeFromLoad` / `UploadMeshData`** | **`Mesh*` + CPU 顶点数据** | **完整** | 中 | 无关 | 高 |
| L4 | 托管 `Mesh.SetVertices` / `ApplyAndDisposeWritableMeshData` | 完整语义 | 完整 | 好 | 无关 | 低 |
| L5 | `GfxDevice::UploadMeshData` → `CreateBuffer` | 顶点/索引缓冲字节 | 无 | 好 | 每 API 一套 | 中高 |
| **L6** | **`DrawIndexed` + buffer 绑定（3Dmigoto 路线）** | 只有原始 stream | 无 | 极好 | 每 API 一套 | 中高 |

- 「渲染管线之前」有两种读法：Unity 渲染管线之前 = L3/L4；GPU 管线之前（3Dmigoto）= L5/L6。
- **主后端选 L3**：语义完整（可改顶点数、可重算 bounds、可同步骨骼/BlendShape）、与图形 API 无关、不破坏游戏逻辑。
- **L6 作兜底**：GPU-resident mesh、Unity 6、L3 定位失败时用。

## 3. Unity 上纯 3Dmigoto 路线的四个障碍

3Dmigoto 在 D3D11 成立的前提是：input layout 语义固定、buffer 绑定 = 资源身份、draw call 无隐藏状态。Unity 三条全破：

1. **顶点布局 per-mesh 变化**：`VertexDataLayout` + channel 的 format/dimension 由导入设置决定；`Mesh Compression: High` 会量化成 `UNorm8`；从 `IASetInputLayout` 无法反推通道语义。
2. **缓冲区可能只是内存池一角**：Unity 会把多个 mesh 打包进同一个大 buffer，用 `byte_offset` 区分。3DMigoto 为此新增 `track_region_hashes` / `track_implicit_index_buffers` / `overlay_buffer_hash_lifetime` / `allow_buffer_resize`。**缓冲区哈希 ≠ 资产身份**，已实际打崩过整个生态。
3. **实例化 / GPU 驱动绘制**：`DrawIndexedInstanced` 下同一 draw 覆盖多对象；HDRP 的 GPU-driven culling 参数由 GPU 填入 indirect buffer，CPU 侧读不到顶点数据。
4. **GPU 蒙皮 / BlendShape**：buffer 里是绑定姿态，骨骼矩阵在 cbuffer；BlendShape 用顶点 ID 索引结构化缓冲。替换模型一旦顶点数/顺序变化 → 蒙皮错乱、BlendShape 全废、UV/lightmap 索引错位。

SSMT 用 CPU-PreSkinning / GPU-PreSkinning 区分这类游戏，后者需要单独适配。

## 4. Unity 哈希身份设计

**核心原则：不在字节层哈希，在逻辑资产层哈希。** 字节会被压缩设置、平台、版本改变；逻辑资产不会。

```text
弱键（索引）：(name, vertexCount, indexCount, layoutHash, subMeshCount)
中键（主键）：layoutSignature + 规范化顶点流（逐顶点按语义顺序反量化成 float）
             fingerprint = H(layoutSignature ‖ 规范化顶点流 ‖ 索引流)
强键（人工校验）：参考渲染图 / 包围盒 / 拓扑统计
```

- 反量化是跨游戏、跨压缩设置复用的关键：`UNorm8/SNorm16` 解码成 float 后，同一模型在不同游戏可得到一致指纹。
- 免费强身份：il2cpp 元数据给出 `(AssetBundle 名, 资源名, pathID)`，再用托管对象 `m_CachedPtr` 映射到原生 `Mesh*`。可读稳定身份 + 内容寻址抗改名指纹，优于任何单方案。

## 5. Unreal 为什么在热加载上更强

- **官方内置资产覆盖**：`FPakPlatformFile` 是 VFS 层，pak 按 mount order 分层，后挂载覆盖先挂载。关键 API：`Mount` / `Unmount` / `MountAllPakFiles` / `Mount` 重载（含 IoStore）/ `MakeUniquePakFilesForTheseFiles` / `bLookLooseFirst`。官方建议用 `FCoreDelegates::OnMountPak` 委托，不要直接调 `Mount`。
- **万能钩子**：`UObject::ProcessEvent` 是 blueprint 函数与多数 `UFUNCTION` 的汇聚点；`TObjectIterator<UStaticMesh>` 可直接枚举全部静态网格，无需 native hook。
- **运行时换模型是公开 API**：`SetStaticMesh` / `SetSkeletalMesh` 由引擎维护 bounds、渲染状态、物理状态。
- **语义差异**：Unity AssetBundle 是「显式加载的资产容器」，UE pak 是「带优先级叠加的 VFS 层」。所以 Unity 适合「加载自己的资产再换引用」，UE 天然适合「按路径覆盖游戏自己的资产」。

热加载能力对比：

| 能力 | Unity | Unreal |
| --- | --- | --- |
| 运行时加载新模型资产 | 可（`AssetBundle.LoadFromFile`） | 可（`Mount` + registry 扫描） |
| 路径级覆盖游戏原有资产 | 不可，需 hook | 可，凭 mount order |
| 运行时换掉已加载对象上的模型 | 可（原生 Mesh API 原地改） | 可（`SetStaticMesh`） |
| 卸载并回滚 | 可（换回原 Mesh 内容） | `Unmount` 存在，但 `UPackage` 缓存使同路径重载不可靠 |

## 6. Unreal 的真实代价

1. **AOB 签名维护**：UE4SS 明确声明不保证每个游戏可用，需要自己更新 AOB；proxy DLL 注入，与 3Dmigoto 同构。
2. **挂载 ≠ 资产可用**（最常见）：`Mount` 只让文件可读，AssetRegistry 不会自动发现；必须 `IAssetRegistry::ScanPathsSynchronous` 或加载 pak 内 `AssetRegistry.bin`，更优是走 chunk（`UAssetManager::LoadPrimaryAssets`）。
3. **Mount point rebasing**：pak 内烘焙了 MountPoint，从别的工程 cook 出来挂载点不匹配，必须手动 rebase 并 `RegisterMountPoint` + `AddPath`；`/Game/` 是 `Content/` 的引擎别名。
4. **cook 设置必须完全一致**：至少渲染设置要对齐，否则材质损坏。
5. **EDL 时序**：打包版本挂载 mod pak 可能崩溃；插件 Loading Phase 改 `PreEarlyLoadingScreen`，或禁用 Event Driven Loader。
6. **依赖完整性**：pak 内资产的依赖必须同 pak 提供并注册进 registry。
7. **覆盖同名资产未获官方承诺**：机制上 mount order 决定优先级，但需实测。
8. 其他：pak 签名/加密、反作弊（EAC/BattlEye）、IoStore（`.utoc/.ucas`，落点 `IIoDispatcherBackend`）、Nanite（GPU-resident 几何，CPU 侧可能没有完整可改 LOD）。

## 7. 对比总表

| 维度 | Unity | Unreal |
| --- | --- | --- |
| 资产覆盖/优先级 | 无内建机制，需自己 hook | 内建 pak mount point + mount order |
| 反射 | IL2CPP 元数据可得；原生 C++ 无符号 | 完整 UPROPERTY/UFUNCTION 反射 + TObjectIterator |
| 逻辑 hook 落点 | Harmony（托管）/ 原生 inline hook | `UObject::ProcessEvent` 单一汇聚点 |
| 运行时换模型 | 原生 Mesh API 原地改，或图形 API 换 buffer | `SetStaticMesh` / `SetSkeletalMesh` 公开 API |
| 运行时挂载新内容 | `AssetBundle.LoadFromFile`（显式容器） | `FPakPlatformFile::Mount`（VFS 层） |
| 免重启 | 可行 | 成熟 |
| 版本耦合 | 每 Unity 大版本重做签名 | 每 UE 小版本 + cook 版本匹配 |
| 内容兼容性 | 顶点布局 per-game profile | Skeleton / PhysicsAsset / 渲染设置 / cook 版本匹配 |
| 官方支持 | 无 | 有（Plugin / GameFeature / pak 均为官方设计） |
| 资产管线工具链 | 社区（AssetStudio/AssetRipper/UABPy） | 官方（UnrealPak / cook / UnrealBuildTool） |

## 8. 工程建议

**选 Unity**：

1. 主后端做 L3（原生 Mesh 对象层），L6 兜底。
2. 身份用三级指纹，规范化到逻辑层。
3. 接受 per-game profile 是固定成本。
4. 热加载走「运行时 LoadFromFile + 换 Mesh 内容」。

**选 Unreal**：

1. 先验证能不能 cook——这是真实瓶颈，不是 hook。
2. 路径覆盖靠 pak mount order；热加载靠运行时 Mount + registry 扫描。
3. 逻辑用 `ProcessEvent` hook（UE4SS 路线），几何用 `SetStaticMesh`。
4. 建立 AOB 签名库 + per-game config。
5. 内容管线尽量复用官方工具链，把适配成本压到配置层。

**通用结论**：任意引擎下「工具通用 + 资产按游戏适配」无法绕过；声称能免掉资产适配的方案，一定在某个环节偷了语义。

## 9. 参考

- SSMT 支持列表 / CPU-GPU PreSkinning：https://perxenic-acid.github.io/SSMT4-Documents/newbie/ssmt/SSMTSupportedGames/SSMTSupportedGames.html
- 终末地 `track_region_hashes` 等 d3dx.ini 变更：https://perxenic-acid.github.io/SSMT4-Documents/games/efmi/D3dxIniChanges/D3dxIniChanges.html
- UE4SS 文档：https://docs.ue4ss.com/
- `FPakPlatformFile` API：https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/PakFile/FPakPlatformFile
- 运行时加载 Content/Pak：https://dev.epicgames.com/community/learning/knowledge-base/D7nL/unreal-engine-primer-loading-content-and-pak-files-at-runtime
- Mount 后资产找不到：https://bugnet.io/blog/fix-unreal-pak-mounted-but-asset-not-found
- 社区实战（mount point rebasing、cook 一致性、EDL）：https://forums.unrealengine.com/t/load-assets-from-mounted-pak-files/130124
- GIMI：https://github.com/SilentNightSound/GI-Model-Importer
- ZZ-Model-Importer：https://github.com/leotorrez/ZZ-Model-Importer
