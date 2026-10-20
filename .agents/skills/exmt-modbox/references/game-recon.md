# GF2 Exilium（少女前线2：追放）逆向技术判据

> 定位：Unity 2019.4 + IL2CPP + 私有保护层的商业手游。本文只保留事实、做法、判据、坑与常量。
> 路径以游戏安装根为相对基准。已否掉的假设一律保留，用于止损。

## 1. 目标与保护层

| 项 | 值 |
|---|---|
| 引擎 | Unity 2019.4.40f1（PE `ProductVersion` = `2019.4.40.16762411`） |
| 资产烘焙版本 | 2019.4.29f1（bundle 内 `unityRevision`，与播放器同 LTS 线） |
| 脚本后端 | IL2CPP x64：`GameAssembly.dll` ≈94 MB + `il2cpp_data/Metadata/global-metadata.dat` 17,924,096 B |
| 资源体量 | 84.6 GB / 17,802 文件；主库 `LocalCache/Data/AssetBundles_Windows` = 9,612 个 `.bundle` / 67.8 GB |
| 图形 API | **D3D11**（加载 `d3d11.dll` 2.37 MB + `dxgi.dll` 1.23 MB；无 `d3d12`/`vulkan-1`/`nvoglv64`） |

| 组件 | 判定 | 判据 |
|---|---|---|
| `NEP2.dll`（≈39 MB） | **保护 + 文件读取 detour + 前缀解密** | 仅 1 个导出 `NEP_Unknown`；节区名 `.ndata0/.fdata1/.fptable/Shared/.200` 非标准；导入 `CRYPT32`+`WINTRUST`+`bcrypt`；`.rsrc` 26.7 MB；`GF2_Exilium.exe` 只导入 NEP2/UnityPlayer/KERNEL32；读取栈形态 `UnityMain → NEP2 → NEP_Unknown → UnityMain` |
| `KS_Diagnostics_Process.dll`（3.5 MB） | 无害，崩溃上报器 | `.managed` 节 = .NET 程序集，导出全是 `Process`/`ProcessStartInfo` 包装 |
| `gfcommonlib.dll`（0.12 MB） | 无关 | 导出为 `_CalcBloomPass`/`_Ceil`/`_GetShaderString` 等着色器数学函数 |

- NEP2 静态形态的三个反常规特征（同时出现即高度可疑）：导出只有 `ret 0` + `int3`；`.pdata` 仅 108 条有效记录（含 `0x1..0x3DB6BF2` 这种 64 MB 垃圾项）；关键常量表落在 `.rsrc`。
- NEP2 的 AES 实现（T-table `c66363a5` @0x18C3540、逆 S-box @0x7A6EA0、Rcon @0x7A6FAC、分组候选 0x2306A0、4×u64 密钥材料）**与资源无关**：24 组轮密钥 × ECB/CBC × 128/256 位，`UnityFS` 命中 0；`bcrypt` 只出现 SHA256/SHA1/RSA，`BCryptGenerateSymmetricKey` 与 `BCryptDecrypt` 均为 0 次。
- `GameAssembly.dll` 无任何密码学痕迹（`bcrypt`/`CryptDecrypt`/`AES` 字符串全 0 次）⇒ 资源解密不在游戏逻辑里。

## 2. 资源拓扑与明文面

| 位置 | 状态 | 用途 |
|---|---|---|
| `StreamingAssets/aa/StandaloneWindows64` | 4 个**明文**标准 UnityFS（13.9 MB） | 场景包 `aotres_scenes_all`（1,643 对象）、`Reporter.prefab` 等；无角色模型 |
| `LocalCache/Data/AssetBundles_Windows` | 9,612 个，**仅前 0x8000 字节加密** | 全部角色/场景/贴图资产 |
| `LocalCache/Data/Table/*.bytes` | 2,288 个**明文** protobuf（小端长度前缀 `0C 00 00 00 …`） | 玩法配置，含完整资产路径与角色名 |
| `LocalCache/.../catalog_main_28495.bin` | 59,261,318 B **明文** | Addressables 主清单 |
| `ClientRes_Windows/4.0.5136/Codes` | 79 文件 / 488.6 MB，`.dll.bytes` 为真 PE | 托管程序集（仅名称可用） |
| `ClientRes_Windows/4.0.5136/A5CFF04…` | 1,031 个 `.bytes` / 7.4 MB，加密 | 第三类容器，格式未解 |
| 单一 `GFF\0` 容器 | 1 个 / 286 MB（占 0.4%） | 自定义容器，未解 |

- **核心判据：资产加密 ≠ 清单加密。** 正文（bundle 数据）加密，但目录、数据表、程序集元数据、bundle 内资源名全为明文 ⇒ 「定位目标」这一半完全不需要解密。
- `StreamingAssets` 的明文包 `unityRevision` 是 `"0.0.0"`（非法版本串），UnityPy 必须设 `UnityPy.config.FALLBACK_UNITY_VERSION = "2019.4.29f1"`，否则报 `No valid Unity version found`。

## 3. UnityFS 头与 XOR 加密

头部字段（解密后）：

| 偏移 | 类型 | 值 |
|---|---|---|
| +000 | bytes[8] | `"UnityFS\0"` |
| +008 | u32 BE | `7`（版本） |
| +012 | cstring | `"5.x.x"` |
| +018 | cstring | `"2019.4.29f1"`（句点；曾被误读为逗号 `2019.4,29f1`） |
| +030 | i64 BE | 文件字节数 ← **唯一硬自校验锚点** |
| +038 | u32 BE | `cbis`（blocksInfo 存储长度） |
| +042 | u32 BE | `ubis`（blocksInfo 解压后长度） |
| +046 | u32 BE | `flags = 0x43` |
| +050..+063 | — | 14 个 `00`（16 字节对齐填充） |
| +064 | — | blocksInfo 起点 |

- 头部是**变长 cstring**：明文包 revision 为 `"0.0.0"`（5 字符），加密包为 `"2019.4.29f1"`（11 字符），后续字段整体差 6 字节。解析必须按 cstring 走位，禁止硬编码偏移。

密钥派生（只有一个未知量）：

```
X   = cipher[0:4] XOR "Unit"          # 4 字节
PAD = sign_extend_int32_to_int64(X)   # 本作全部样本为 FF FF FF FF
X2  = X 按小端 u32 异或 0x00000002     # 等价 X[0] ^= 0x02
ks  = X ‖ PAD ‖ X2 ‖ PAD              # 16 字节周期
```

- 独立校验 PAD：`cipher[4:8] == "yFS\0"` ⇒ PAD 全 `00`；`== ~"yFS\0"` ⇒ PAD 全 `FF`。
- **加密范围只有前 `0x8000`（32,768）字节，其余 99.8% 原样明文。**

```
plain  = cipher[:0x8000] XOR ks(循环) ‖ cipher[0x8000:]
cipher = plain[:0x8000]  XOR ks(原密文头派生) ‖ plain[0x8000:]
```

- 无外部密钥、无机器绑定。写回时必须由**原密文前 4 字节**派生 `ks`（由明文头派生会得 X=0）；因此密文前 4 字节保持不变，解密端仍能派生出同一 X。
- XOR 自逆 ⇒ 加密与解密同一函数，且 `X` 可任选（任取末字节最高位为 1 的值即可模拟 `PAD=FF`）。

判据与验证顺序：

1. 解出的 `+030 size` == 磁盘字节数 ⇒ 头部解对。对不上立刻停，不要继续解析。
2. 怀疑「只加密了一部分」时，把前缀长度 N 当参数扫，以**下游 LZ4 块能否全部解出**为目标函数：N=32752/32768 时 39/39、222/222 块全解，N=32784 时 38/39 ⇒ 边界精确 `0x8000`。
3. 密钥流按 8 字节块统计若恰好只有 2 种且严格交替 ⇒ 真实周期是 16 字节。

坑：

- 把 16 字节周期当 8 字节，会同时表现为：`version` 读成 `0x02000007`、revision 读成逗号、`cbis/ubis` 出现「1.96× 膨胀」。这些异常同一根因，别分别归因。
- `decrypt(encrypt(x)) == x` 的自我往返**不能**证明算法正确（两侧共用同一错误密钥流，必然自洽）；必须让外部消费者判定：LZ4 解码器、`size` 字段、SerializedFile 解析器。
- 只解 4 KB 就断言「整段正确」会误判；真值采样上限受 I/O 分块限制（`ReadFile` 32 KB），有效验证范围 = 采样上限。
- 判断 XOR 是否正确要看**已知明文锚点**（字段名、路径），不要看整体熵（XOR 保熵，密文 7.27 / 解后 7.32，几乎不变）。

## 4. bundle 内层结构（blocksInfo / 块表 / 节点表）

- **header 之后按 16 字节对齐**：header=50 ⇒ `+50..+63` 补 14 个 `00`，blocksInfo 从 `+64` 开始。对齐长度 = `16 − (header 长度 mod 16)`。
- blocksInfo 自身按 `flags & 0x3F`：`3` = LZ4HC 压缩，`0` = 原样；解压后长度 = `ubis`。

解压后的布局（字段全 **大端**）：

```
hash[16]                     # 恒为 16 个 0
u32 blockCount
blockCount × { u32 uncompressedSize; u32 compressedSize; u16 flags }
u32 nodeCount
nodeCount × { i64 offset; i64 size; u32 flags; cstring path }
```

- 块 flags：`3` = LZ4HC，`0` = 原始存放。node flags：`4` = SerializedFile，`0` = `.resS`。
- 块尺寸 `0x20000`（128 KiB）：块 i 覆盖 `[i*0x20000, (i+1)*0x20000)`。
- `data_off` = header + blocksInfo 压缩长度（实测 340 / 1126 等），即块 0 首字节。
- **两条自校验必须同时成立**：`Σ uncompressedSize == Σ node.size`；`Σ compressedSize == 文件长度 − data_off`。只对上一条可能是巧合。
- SerializedFile = `[data_off, data_off + node[0].size)`；头部整数字段大端（version=21），`endian` 字节之后转小端；`UnityPy.load(bytes)` 可直接得对象表。

改包做法（只重建必要块）：

1. 目标贴图在解压流中的区间 = `SF_size + resS_offset`。
2. 凡与该区间相交的块重建，其余块**按原 `compressedSize` 逐字节搬运**（不解压、不碰漂移问题）。
3. 重建块保持 `uncompressedSize` 不变（node offset 才不变），`compressedSize` 用新压缩结果。
4. blocksInfo 重新 LZ4 压缩，回写 header 的 `size/cbis/ubis`。
5. 只加密前 `0x8000`，其余原样写出。

坑：

- 直接在 `+50` 读 blocksInfo ⇒ 永远解不开（漏了 16 字节对齐，这是长期卡点的真正原因）。
- `cbis/ubis` 比值 1.5~2.4 是 LZ4 正常区间，不是「膨胀异常」。
- 多块包的块流用 python-lz4 从 60~120 KB 起会漂移/越界（`offset=0`、越界），单块明文包可严格解出 ⇒ **多块包不要依赖全量解压，只搬不改**。
- 数据块内能读到的字段名（`m_ForcedFallbackFormat`/`CompleteImageSize` 等）只存在于元数据区，不代表数据块未压缩。判压缩用**滑窗熵**：全程 7.6~7.9 = 压缩；出现 <6.5 的段才是明文结构。
- 「解析器打不开」不等于数据坏；但当块未压缩/可原样搬运时，也不要为了迁就标准解析器去改数据。

## 5. 资源名、清单与角色资产

| 来源 | 规模 | 内容 |
|---|---|---|
| `catalog_main_28495.bin` | 59,261,318 B | 68,723 个 bundle ID，覆盖磁盘 9,353/9,612（97.3%）；833 个 `Assets/` 地址（786 个是 Scene）；Addressables 1.21.18 |
| `Table/*.bytes` | 2,288 文件 | 明文 protobuf，含完整资产路径；角色资产**不在** catalog 路径寻址体系内 |

角色资源前缀统计（全量 Table 去重后）：

| 前缀 | 次数 | 含义 | 3D 模型 |
|---|---|---|---|
| `Avatar_Head_*` | 10,623 | UI 头像图集 | 否 |
| `Avatar_Bust_*` | 272 | 半身像 | 是 |
| **`Avatar_Whole_*`** | 106 | 全身 | 是 |
| **`Avatar_Gacha_*`** | 61 | 抽卡展示 | 是 |
| `Avatar_Half_*` / `Avatar_Character_*` | 12 / 6 | 半身 / 编号角色 | 是 |

- 命名规律：`<名><稀有度><序号>`（如 `Avatar_Whole_UllridSSR`、`Avatar_Gacha_NikketaSSR`）。
- 部件命名：`<角色><稀有度><序号>_slg_<部件>_lod<N>`，部件 `cloth`/`body`/`hair`/`bear`/`plush`。
- 前缀归类法判据：出现次数最高的是 UI 资源；**次数少但语义明确的才是模型本体**（Whole/Gacha 仅 106/61）。
- 定位 recipe：只读前 16 KB → 由密文头 4 字节派生 X → 解密 → 子串搜索角色名。9,612 包 × 16 KB = 154 MB 即可建「资源名 → 包哈希」索引（全量解密需 67 GB）。
- 收敛 recipe：hook `CreateFileW` 只按 basename 记录 `.bundle`（一次启动 641 个），与候选集求交集，64 → 7。改动无效果时**先查该包是否被打开过**。
- 运行时 `LoadAsset*` 的资产名是 32 位十六进制哈希（85 次调用 0 个可读名）⇒ 不能按可读名过滤，只能靠 bundle 内元数据 + 属性指纹（尺寸/类型/加载顺序）。

服装包 `287dc73c9302244853e6d72689dd51bb`（resS 29,010,816 B 被 9 张贴图恰好铺满）：

| 贴图 | 尺寸 | 格式 | resS offset | size |
|---|---|---|---|---|
| `cloth01_n` | 2048² | BC7 | 0 | 5,592,432 |
| `cloth02_d` | 2048² | **DXT1** | 5,592,432 | 2,796,216 |
| `cloth01_d` | 2048² | **DXT1** | 14,680,184 | 2,796,216 |
| `cloth02_rmo` | 2048² | BC7 | 9,087,752 | 5,592,432 |

其余：`cloth01_rmo`/`cloth02_n` 2048² BC7（各 5,592,432）、`plush_nt`/`plush_dt`/`plush_bt` 512² BC7（各 349,552）。

- DXT1 纯红块：`00 F8 00 00 00 00 00 00`（`c0=0xF800` 纯红、`c1=0x0000`、索引全 0）。`c0=c1` 的写法无效。
- 整段覆写会同时覆盖全部 mip；要保留明暗需按 mip 逐级处理。

## 6. IL2CPP 与托管程序集

- `GameAssembly.dll` 导出 240 个，**229 个为 `i<随机串>`，0 个含 `il2cpp`** ⇒ `il2cpp_domain_get`/`class_from_name`/`object_new`/`array_new`/`string_new` 全部不可用。
- 候选方法指针表 130,562 项，静态与加载后完全一致（非加载填充）；但单 `Assembly-CSharp` 就有 201,514 个 `MethodDef`，全量 >40 万 ⇒ 不是全量映射，「方法定义 → 原生地址」未取得。
- `global-metadata.dat` 整体加密：熵 7.9843、`0xFAB11BAF` 正确但 `version=0x01117CE8`。运行时以 17,924,096 B 双份 mmap（`r--` 与 `rw-`），`version` 仍为垃圾值 ⇒ **按需解密，没有可 dump 的完整明文** ⇒ Il2CppDumper / Il2CppInterop / BepInEx 全线不可用。
- `Codes/*.dll.bytes` 是真 PE（`Assembly-CSharp` 32.21 MB：CLI cb=72、runtime 2.5、Metadata RVA 0xF8AF74 / size 17,477,456、BSJB 正确），但**方法体是占位垃圾**：`Crypt.EncryptXor` 只有 `bgt.s`+`ldc.i4 475118386`+`nop`；所有 async `MoveNext` 为空；不同类的 `get_Current` 字节完全相同（196,105 个方法体可读、0 解析异常）。
- 因此托管 DLL 只能当**符号字典**：`#Strings` 393,680 条（去重 357,903）、`#US` 49,364 条（去重 49,174）。可取得 `Utage.Crypt.EncryptXor/DecryptXor`、`SetAssetBundleDecryptKey`、`Utage.WWWEx.*`、`Framework.ResSys.ResourceManager.*`、`SkinnedMeshMerger*` 等名称。
- `.u` 变体 = 12 字节头 + `MZ`（offset 12）的打包 PE。
- 判据：真实逻辑在 `GameAssembly.dll` 原生码；托管 DLL 不是逻辑来源。
- 结论：**美术替换不需要游戏 C# 逻辑**，只依赖 Unity 引擎侧（Mesh/AssetBundle/PersistentManager）与图形 API。`SkinnedMeshMerger` 系列存在 ⇒ 替换必须发生在加载层，早于合并烘焙。
- 框架的 `Crypt` 类 ≠ 资产解密器（Utage 本身不加密 AssetBundle，其 XOR 用于存档/剧本文本）；`SetAssetBundleDecryptKey` 是游戏自有接口。

## 7. 运行时 hook：icall 槽与地标

IL2CPP 惰性解析 icall，解析结果写入静态槽：

```asm
mov  rax, [rip + D]      ; 读槽缓存
test rax, rax
jne  <已解析>
lea  rcx, [rip + S]      ; 签名字符串
call <parser>            ; GameAssembly + 0x8D670
mov  [rip + D], rax      ; 写槽
call rax
```

三层策略：**L1 hook 解析器 `+0x8D670`**（任何 icall 一解析即捕获）+ **L2 每 1.5 s 轮询槽**（捕捉惰性填充）+ **L3 拿到地址即刻挂接**。

| icall | 槽 RVA |
|---|---|
| `AssetBundle::LoadFromFile_Internal` | `0x55A3448` |
| `AssetBundle::LoadFromMemory_Internal` | `0x55A3498` |
| `AssetBundle::LoadAsset_Internal` | `0x55A3420` |
| `AssetBundle::LoadAssetAsync_Internal` | `0x55A3370` |
| `AssetBundle::LoadAssetWithSubAssets_Internal` | `0x55A33A0` |
| `AssetBundle::LoadFromFileAsync_Internal` | `0x55A33B0` |
| `AssetBundle::LoadFromMemoryAsync_Internal` | `0x55A3478` |
| `AssetBundle::Unload` | `0x55A33F8` |
| `AssetBundle::GetAllAssetNames` | `0x55A3438` |
| `AssetBundle::GetAllScenePaths` | `0x55A33F0` |
| `AssetBundle::Contains` | `0x55A34B8` |

- 未解析的槽读出 `null`（未被调用过的 icall）⇒ 必须配解析器 hook，不能只轮询。
- blob 回扫五步法：① 搜签名子串得签名 RVA ② 回扫 ≤512 B 找「4 字节长度前缀 + blob 含该签名」的起点 ③ 在可执行段搜指向 blob 起点的 `lea` ④ 从 `lea` 解出槽地址 ⑤ 运行时 `base.add(slot).readPointer()`。签名永远在 blob **内部**，故「`lea` 目标 == 签名 RVA」必不成立。
- 坑：icall 签名是长度前缀拼接的 blob，**子串匹配会误命中**（`LoadFromStreamInternal` 被认成 `LoadFromMemory_Internal`）；必须用完整签名或精确偏移。
- 坑：调用前先打印所有槽指针；`GetPropertyCount` 槽 `0x55BE378` 运行时读出 `0x0`，直接导致枚举静默失败。返回值与输入指针差固定偏移（`get_shader` 返回 `mat+0x20`）说明它不是目标对象而是字段地址。
- 坑：`NativeFunction` 调用会改寄存器，在 `onEnter` 里必须先把参数读进 `this` 再调用；多个 icall 串联要**分层加码、逐层打日志**；计数变量不要写在条件分支内（会造成 `mats=1` vs 实际 79 的假矛盾）。

Material 家族（全原生参数，可 Frida 直调）：

| icall | 槽 RVA | 备注 |
|---|---|---|
| `Material::SetColorImpl_Injected(int, Color&)` | `0x55B4BB0` | 首选的「改色」入口 |
| `MaterialPropertyBlock::SetColorImpl_Injected(int, Color&)` | `0x55B46E8` | 属性块版本 |
| `Material::get_shader()` | `0x55B4AD0` | 返回值可疑（= mat+0x20） |
| `Shader::GetPropertyCount()` | `0x55BE378` | 运行时为 NULL，不可用 |
| `Shader::GetPropertyNameId(Shader,int)` | `0x55BE418` | 返回 int |
| `Shader::GetPropertyName(Shader,int)` | `0x55BEFA8` | 返回字符串（只读，无需分配） |
| `Material::HasProperty(int)` | `0x55B54B0` | 可用性未验证 |

调用约定（`SetColorImpl_Injected`）：`rcx` = 材质指针，`rdx` = `int nameID`，`r8` = 指向 16 字节 `Color{r,g,b,a}` 的指针；纯红 = `{1.0, 0.0, 0.0, 1.0}`。全程不需要构造托管对象。

- **`SetColorImpl` 是通用 Vector4 通道，不专属颜色**：50 个 nameID 里含屏幕尺寸 `(2400,1350,0,0.001)`、轴向 `(1,0,0,0)/(0,1,0,0)/(0,0,1,0)`。无差别覆写 → 角色消失、界面发黑。
- 安全流程：只记录不修改 → 按值域筛候选（四分量都在 [0,1]、非纯白、alpha 合理）→ **单 ID 白名单**验证安全（改 1 个 ID、4,498 次调用零破坏）→ 再小批量试。
- 判据：原值像目标颜色不足以断定用途（`(0,0.3,0.55)` 深蓝并非服装属性），必须改完看画面。
- 运行时能力边界：读字段 ✅（按 `+0x10` 长度、`+0x14` UTF-16 等偏移直读）；调「参数为 int/float/指针」的 icall ✅；调「参数为 string/数组/托管对象」的 icall ❌；构造托管对象 ❌（无 `il2cpp_*` 导出）。设计原则：把需要托管对象的环节挪到离线/一次性步骤。

## 8. 渲染与校验

- D3D11 已确认 ⇒ 3Dmigoto 风格（GPU 层）路线可行，且落在生态最成熟的后端上，无需处理 D3D12 描述符堆/Vulkan 内存类型。
- **CRC 实测：加密包全部不做校验。** 1,797 次加载走 `AssetBundle.LoadFromStreamInternal(Stream, crc, 0x8000)`，调用点 `GameAssembly+0x20511BC`：

```asm
mov r8d, esi      ; 缓冲大小 0x8000
xor edx, edx      ; ★ crc = 0
mov rcx, rdi      ; Stream
call rax          ; LoadFromStreamInternal(stream, 0, 0x8000)
```

- 仅 4 个明文包走 `LoadFromFile_Internal` 且带真实 CRC：`0x175fd061` / `0xa787c68b` / `0xfc18c503` / `0x1407deac`。
- 判据：`xor edx,edx` 紧接 call = 传 0 关闭校验；判断校验是否生效看调用点实参，不要猜。
- 错误码机制（`UnityPlayer+0x2E5FE8`）：`cmp ebp,0xb` → `mov ecx,[rdx+rbp*4+0x2E6170]` → `add rcx,rdx` → `jmp rcx`；**表基址 `0x2E6170`，表项为绝对 RVA**。`code 2` = `AssetBundle '%s' has invalid crc!`，`code 9` = `Failed to decompress data`。
- 格式化器 `UnityPlayer+0x2E3790`：`rcx` = 输出对象、`edx` = 错误码、`r8` = bundle 名（为 0 时取 `[rcx+0x20]`）；11 个调用点 `0x2D9B8C 0x2D9C7E 0x2DA456 0x2DA4FB 0x2DA760 0x2DA7B8 0x2DA824 0x2DA8D3 0x2DA947 0x2DA9C0 0x2DAA37`。
- 已否决：`0x83C700`（原判为块解压器，`cmp eax,-1` 后设 code 9）在启动/进大厅/触发导航全程 0 次调用。**「失败后设哪个错误码」不能唯一确定函数职责**，反汇编给可能性、运行时调用计数给事实。
- 薄封装判据：函数体 <256 B 且主要是 `mov`/`lea`/`call` ⇒ 只是转发，继续跟 `call`。`0x3B7650`(210 B) → `0x2D8BC0`(209 B) → `0x593DC0` 才是解析核心。
- 去符号 64 位 DLL 的定位三件套：**字符串 xref + `.pdata` 函数边界 + capstone**，秒级出结果（Ghidra 对 25 MB 的 `UnityPlayer.dll` 会超时）。`.pdata` 有 87,065 条 `RUNTIME_FUNCTION`。
- 字符串找不到引用不等于没用：`0x1504538` 的 `"UnityFS\0UnityWeb\0…"` 归档签名表 `lea` 与 RIP 相对引用均为 0 ⇒ 经 `.data` 指针表间接访问（搜「等于该地址的 8 字节值」再找读它的指令）。
- 空转判据（区分「进程活着但在空转」与「真在加载资源」）：窗口标题 + CPU 累计 + `CreateFileW`/`ReadFile` 计数三重指标（本例空闲态：opens 0 / reads 46 / assetReads 0）。

## 9. 可行路线与禁区

可行：

| 路线 | 状态 | 关键依据 |
|---|---|---|
| 磁盘改包 + 自有加密前缀 | ✅ 引擎内验证通过 | 只加密前 `0x8000`；`X` 可任选；构造后游戏正常加载并渲染 3D 场景 |
| 只重建覆盖目标区间的块 | ✅ 已验证 | 其余块逐字节搬运，绕开多块 LZ4 漂移 |
| 按块重建替换 DXT1 漫反射贴图 | ✅ 已验证 | 44 块重建 / 178 块搬运；制服整体变红，面部/头发/UI 正常 |
| hook 解析器/槽取任意 icall 地址 | ✅ | 槽表 + `+0x8D670`，不需解密元数据 |
| `Material::SetColorImpl_Injected` 染色 | ⚠ 可用但需白名单 | 174,201 次调用 / 50 个 ID；单 ID 安全 |
| D3D11 层（3Dmigoto 风格） | ✅ 不受加固影响 | `d3d11`+`dxgi` 已加载 |
| hook `LoadFromStreamInternal` 替换 Stream | ⚠ 备用路线 | 地址已知、CRC=0（磁盘路线已达同目的，未实现此项） |
| 离线解出 bundle 内层 | ✅ 已解 | 16 字节对齐 + 大端块表 + 两条求和自校验 |

禁区（已证明无效，勿重复投入）：

| 做法 | 否证方式 |
|---|---|
| Il2CppDumper / BepInEx / Il2CppInterop | 元数据按需解密、无完整明文；方法体占位 |
| 全内存扫 `UnityFS\0` 或解密后 SerializedFile | Unity 解析头后丢弃原始映像：3,315 区间 / 5,297 MB → 0 命中 |
| 盲改贴图字节（按 DXT 块覆写） | 数据块压缩且全程熵 7.6~7.9，覆写只破坏压缩流 |
| 无差别 `SetColor` 全量覆写 | 破坏 shader 参数：角色消失、界面发黑 |
| 按可读资源名过滤 `LoadAsset*` | 运行时参数是 32-hex 哈希 |
| 用 NEP2 的 AES 解资源 | 24 组轮密钥 × 3 模式 × 2 长度，`UnityFS` 命中 0 |
| 追 NEP2 自解密/自补丁 | 内存与磁盘逐字节一致（`.text` 0 差异、`.rsrc` 0 差异、`.200` 仅 80 字节基址重定位） |
| 猜 blocksInfo 为某种已知布局变体 | 标准明文布局穷举只出退化组合；LZ4 与 zlib/gzip/bz2/lzma 全家桶全失败；真实原因是 16 字节对齐 |
| 把 `50+cbis` 当 SerializedFile 起点 | 按此解析得天文数字 |
| 用「头 4 KB 能读到字段名」推断块未压缩 | 那 4 KB 是元数据区；熵剖面才是判据 |
| 把 Utage.Crypt 当资产解密器 | Utage 框架不加密 AssetBundle |
| 只靠反汇编断定函数职责 | `0x83C700` 被 0 次调用否决 |

最小替换流程（已验证）：

1. 备份原包（实验后必须还原并用 SHA256 校验一致性）。
2. 用 UnityPy 解析模板/目标包的 SerializedFile，改属性（贴图/材质）。
3. 只重建与目标区间相交的块，其余块按原字节搬运；更新 `cbis/ubis/size`。
4. 只对前 `0x8000` 施加由**原密文头**派生的密钥流，其余原样。
5. 写回磁盘，启动游戏验证；失败先区分「加密不对」还是「资源 ID/地址不匹配」。
6. 若磁盘替换受 bundle ID 绑定限制，退路是 hook `LoadFromStreamInternal` 整体替换 Stream。

验证纪律：先证明「能解」→ 再证明「能逆」→ 最后才做「引擎内验证」。只做「同内容重加密」只能排除加密因素，还需再做一次「不同内容替换」以排除内容校验因素（两次均通过，替换路径才算通）。证据边界：不崩溃 + 画面正常，不等于该 bundle 被读取并解析成功。

## 10. 关键地址与常量

| 对象 | 值 |
|---|---|
| `UnityPlayer.dll` 首选基址 | `0x180000000` |
| 错误格式化器 | `0x2E3790` |
| 错误码表基址（表项 = 绝对 RVA） | `0x2E6170` |
| CRC 文案 | `0x14872B0` |
| `AssetBundle` icall 名称串 | `0x14AB5E8` / `0x14AB620` / `0x14AB650` / `0x14AB688` / `0x14AB788` / `0x14AB7F0` / `0x14AB840` |
| `AssetBundle::LoadFromStreamInternal` | `0x3B7650`（下游 `0x2D8BC0` → `0x593DC0` → `0x595CE0`/`0x594180`） |
| 归档签名表 `"UnityFS\0UnityWeb\0"` | `0x1504538`（无直接引用） |
| 已否决的解压候选 | `0x83C700`（运行时 0 次） |
| 错误码 9 设置点 | `0x2DA77D` |
| GameAssembly icall 解析器 | `0x8D670` |
| GameAssembly 加载编排 / 建流 | `0x2050990` / `0x20505B0` / `0x2050CB0` |
| `NEP_Unknown` 导出（`ret 0` 空壳） | `0x1BD9B0` |
| NEP2 AES Te0 / 逆 S-box / Rcon | `0x18C3540` / `0x7A6EA0` / `0x7A6FAC` |
| NEP2 AES 分组候选 / 密钥材料 | `0x2306A0` / 4×u64 `0x2fcbf11b064a9ac5`… |
| 加密边界 / 块尺寸 | `0x8000` / `0x20000` |
| DXT1 纯红块 | `00 F8 00 00 00 00 00 00` |
| UnityPy 兜底版本 | `2019.4.29f1` |
| 目标包（服装） | `287dc73c9302244853e6d72689dd51bb.bundle` |

## 11. 残余未知

- NEP2 只处理前 `0x8000` 的确切原因（`ReadFile` detour 是否按固定分块）。
- 多块包 LZ4 流在 60~120 KB 后漂移的成因（流式/链接变体？）。
- 属性名 ↔ `nameID` 映射（`GetPropertyCount` 槽为 NULL，`GetPropertyNameId` 可用但未打通枚举）。
- BC7 编码：法线/RMO 贴图暂无法替换（只做了 DXT1 漫反射）。
- 第三类容器 `A5CFF04…`（1,031 个 `.bytes`，头 `E4 B3 8A 9E AC FF E6 6C…`）与单个 `GFF\0` 容器格式。
- 全量角色资产评估集未逐一穷举（命名规律已知，覆盖面待补）。
