# GF2 Exilium（少女前线2：追放）逆向台账

> 目标：摸清资源加载链路，确定最适合做「模型替换 Mod」的 hook 点。
> 本文件记 **结论 + 证据 + 未决问题**，不记执行过程。

最后更新：2026-10-18

---

## 1. 目标身份（已确认）

| 项 | 值 | 证据 |
|---|---|---|
| 引擎 | **Unity 2019.4.40f1**（LTS） | EXE/UnityPlayer.dll 的 PE `ProductVersion` = `2019.4.40.16762411` |
| 脚本后端 | **IL2CPP, x64** | 存在 `GameAssembly.dll`(90MB) + `il2cpp_data/Metadata/global-metadata.dat`(17MB) |
| 发行 | SunBorn（散爆） | `app.info`（GBK 编码，需按 936 码页读） |
| 资源总量 | **84.6 GB / 17,802 文件** | `GF2_Exilium_Data` 递归统计 |
| 资产主库 | `LocalCache/Data/AssetBundles_Windows/` = **9,612 个 .bundle / 67.8 GB** | 目录统计 |

## 2. 保护层（已定性）

| 组件 | 判定 | 依据 |
|---|---|---|
| **`NEP2.dll` (37MB)** | **保护 + 资源解密层** | 只有 1 个导出 `NEP_Unknown`；节区名 `.ndata0/.ndata1/.fptable/Shared/.200` 非 MSVC 标准；导入 `CRYPT32`+`WINTRUST`+`bcrypt`；`.rsrc` 26.7MB；`GF2_Exilium.exe` 仅导入 3 个 DLL（NEP2、UnityPlayer、KERNEL32） |
| **运行时证据** | **NEP2 在 Unity 的文件读取路径上做 detour** | Frida 捕获 `globalgamemanagers` 的访问栈含 `NEP_Unknown`，形态为 `UnityMain → NEP2 → NEP_Unknown → UnityMain` |
| `KS_Diagnostics_Process.dll` (3.5MB) | **无害**，非反作弊 | 含 `.managed` 节（.NET 程序集），导出全是 `Process`/`ProcessStartInfo` 包装 = 崩溃上报器 |
| `gfcommonlib.dll` (0.12MB) | **无关**，非解密器 | 导出为 `_CalcBloomPass`/`_Ceil`/`_GetShaderString` 等着色器数学函数 |
| Frida 可用性 | **未被拦截** | spawn 注入成功、跑满 120s、detach 正常 |

## 3. 加密图谱（已确认）

**结论：加密是不均匀的，按资产种类分三档。**

| 资产 | 加密状态 | 证据 |
|---|---|---|
| `StreamingAssets/aa/StandaloneWindows64/*.bundle` | **明文 UnityFS** | 头 `55 6E 69 74 79 46 53 00 00 00 00 07 35 2E 78 2E` = `"UnityFS\0"`+version7+`"5.x."` |
| `LocalCache/Data/AssetBundles_Windows/*.bundle` (9,612) | **UnityFS 头混淆**（per-file XOR 密钥流） | 见 §3.1 |
| `Data/globalgamemanagers` | **仅前 0x35728 字节混淆**，其后明文 | `"UnityEngine"` 首现于 0x35728；文件长 1,111,772 |
| `Data/resources.assets` 等 | 疑似同样局部混淆 | 4MB 探测仅 8 处 `"UnityEngine"` |
| `il2cpp_data/Metadata/global-metadata.dat` | **整体加密**（魔数保留） | 熵 7.9843 bit/byte、零命中文串；魔数 `0xFAB11BAF` 正确但 `version`=0x01117CE8（合法值应为 24） |
| `LocalCache/.../Table/*.bytes` | 明文 protobuf（小端长度前缀） | 头 `0C 00 00 00 08 0A 10 01 1A 06` |
| `LocalCache/.../08dfe7d8….bundle` | **自定义 `GFF\0` 容器**（仅 1 个 / 286MB / 占 0.4%） | 头 `47 46 46 00` |

### 3.1 UnityFS 头混淆方案（已解出并自校验）

标准 UnityFS 头布局：
```
0x00 signature[8] "UnityFS\0"
0x08 UInt32BE version
0x0C minPlayerVersion[]\0   (如 "5.x.x")
0x12 engineVersion[]\0      (如 "2019.4.29f1")
      Int64BE fileSize      ← 自校验锚点
      UInt32BE compressedBlocksInfoSize
      UInt32BE uncompressedBlocksInfoSize
      UInt32BE flags
```

混淆方式：**每个 4 字节字独立异或**，密钥流按文件不同：

```
文件 e5aab62d…:
  观测: 84 67 D5 44 | 79 46 53 00 | D3 09 BC 37 | 35 2E 78 2E | A9 09 8E 00 | 31 39 2E 34 | FD 3B 85 56 | 31 00 00 00
  密钥: D1 09 BC 30 | 00 00 00 00 | D3 09 BC 30 | 00 00 00 00
  明文:  55 6E 69 74 | 79 46 53 00 | 00 00 00 07 | 35 2E 78 2E | 78 00 32 30 | 31 39 2E 34 | 2E 32 39 66 | 31 00 00 00
        "Unit"       "yFS\0"       version=7      "5.x."        "x\0""20"      "19.4"        ".29f"         "1\0\0\0"
```

**自校验证据**：用同一密钥流解出的 `fileSize` 字段 = `0x268674` = 2,524,788，与该 bundle 磁盘实际大小**完全一致**。这是该解码正确的唯一硬证据（不是"看起来对"）。

导出结论：`engineVersion = "2019.4.29f1"`（资产用 2019.4.29 烘焙，播放器是 2019.4.40，同 LTS 线兼容）。

**关键观察**：奇数位置的字（索引 1/3/5/7）在**同一文件内恒定**，但**跨文件为 `0x00000000` 或 `0xFFFFFFFF` 二选一**。这导致我最初的分类器把同一格式误分成三类（`UnityFS_xor` 4819 / `opaque` 3752 / `other_alpha` 1040）。三类实为同一格式。

## 4. IL2CPP 加固（已确认，构成主要障碍）

| 加固项 | 状态 | 影响 |
|---|---|---|
| `GameAssembly.dll` 导出改名 | **229/240 是 `i<随机串>`**，0 个含 `il2cpp` | 无法按名解析 IL2CPP API |
| `UnityPlayer.dll` 导出 | **仅 `UnityMain` 1 个** | 调用栈符号化失效（大量内部函数都显示为 `UnityMain`） |
| `global-metadata.dat` | 体加密（魔数保留） | **Il2CppDumper 等静态解析器必然失败** |
| 导出表保序性 | 改名后仍按名排序 | 保序，但缺少原始名对照表，无法据此还原 |

**连锁后果**：BepInEx / Il2CppInterop 这条标准路线在本作上**开箱即用不了**（它们依赖解析 `global-metadata.dat`）。

## 5. 未决问题

1. **内存中是否存在解密后的 `global-metadata.dat`？** 若存在 → 可在运行时 dump 出来 → 静态解析 → 拿到全部类/方法地址 → 解锁原生对象层 hook。**这是当前最高价值的未验证假设。**
2. NEP2 的解密函数具体在哪、密钥流如何派生（是否与文件名 MD5 相关）？
3. `LocalCache` bundle 的**正文**是否也被混淆，还是仅文件头？决定能否离线解包改资产。
4. `globalgamemanagers` 的首 0x35728 字节是什么（文件表？索引？）——为何这个长度。
5. 游戏为何在 spawn 后约 2 分钟内退出（是否依赖 PCLauncher 传入的鉴权参数）？
6. 图形 API 实测是 D3D11 还是 D3D12/Vulkan（决定 GPU 层兜底方案的形态）。

## 6. 对「模型替换 Mod」的方案含义

根据前序可行性评估的拦截阶梯，本作的实际可选路径：

| 路径 | 可行性 | 依据 |
|---|---|---|
| **A. 托管层（BepInEx/Harmony）** | **受阻** | 元数据加密 + 导出改名（§4） |
| **B. 原生 Mesh 对象层（`Mesh::AwakeFromLoad`）** | **待定**，取决于未决问题 1 | 需要方法地址；若内存元数据可 dump 则解锁 |
| **C. 图形 API 层（3Dmigoto 路线）** | **最稳** | 完全绕开 IL2CPP 加固；但受 Unity 缓冲区打包/GPU 蒙皮限制 |
| **D. 资产离线替换** | **部分可行** | `StreamingAssets/aa/` 明文可改；`LocalCache` 需先破头混淆并解决正文是否加密 |

**倾向**：以 **C 为主、B 为增强**。理由：C 不依赖任何被加固的组件，且 SSMT 生态已证明该路线在 Unity 上的有效性；B 能提供语义完整性（可改顶点数、可重算 bounds），但前置条件未满足。

## 7. 待办

- [ ] 运行时扫描内存中的解密元数据（验证未决问题 1）
- [ ] 确认 bundle 正文是否加密（决定路径 D）
- [ ] 实测图形 API
- [ ] 解决游戏启动依赖（未决问题 5），否则只能靠 spawn 抓早期窗口

---

## 8. 【重大】游戏发布真实托管程序集（HybridCLR 类热更新）

**这是本目标最有价值的发现，直接绕开了 §4 的全部加固。**

`LocalCache/Data/ClientRes_Windows/4.0.5136/Codes/` 内含 79 个文件、488.6 MB，
其中 **`.dll.bytes` 是真实可用的 .NET 程序集**（不是 IL2CPP dummy）：

| 文件 | 大小 | 验证结果 |
|---|---|---|
| `Assembly-CSharp.dll.bytes` | 32.21 MB | PE 有效；CLI header cb=72、runtime **2.5**、Metadata RVA 0xF8AF74 / size 17,477,456；元数据签名 **0x424A5342 (BSJB) 正确**；Runtime version `v4.0.30319` |
| `Common.dll.bytes` | 3.19 MB | 同上，Metadata size 1,801,584，BSJB 正确 |
| `NetworkProtocol.dll.bytes` | 12.47 MB | MZ |
| `StaticTable.dll.bytes` | 14.48 MB | MZ |
| `Assembly-CSharp-firstpass.dll.bytes` | 0.93 MB | MZ |
| `mscorlib.dll.bytes` | 2.58 MB | MZ |
| `Unity.Addressables.dll.bytes` | 0.10 MB | MZ（Addressables 也可读） |

另有变体：`.bytes_AOT.u`、`.bytes_HUDll.u`。`.u` 文件为 **12 字节头 + 压缩/打包后的 PE**：
```
2F D6 D5 4F | 00 86 5D 02 | 00 24 31 0E | 4D 5A 90 00 ("MZ" @ offset 12)
```

### ⚠ 重要纠正（推翻本节早前判断）

> 起初判断「游戏逻辑在托管 IL 里，可直接反编译，从而绕开加密的 IL2CPP 元数据」。
> **该判断已被证伪。** 这些 DLL 的**方法体是占位垃圾**，不是可读/可执行的逻辑。

证据（Mono.Cecil 全量读取 196,105 个方法体，0 解析异常）：

1. `Crypt.EncryptXor(byte[] key, byte[] buffer)` 的 IL 只有 3 条无意义指令：
   `bgt.s`（跳转目标为 null）、`ldc.i4 475118386`、`nop`。
2. 所有 async 状态机的 `MoveNext`（真正的逻辑所在）**方法体为空**。
3. 两个**不同**类的 `get_Current` 具有**完全相同的乱码**序列
   （`bne.un.s / ldc.i4.4 / conv.i4 / bge.un.s / ldc.i4.4`）——不同类不可能同实现，只能是填充。

结论：`Codes/*.dll.bytes` 是 **IL2CPP 的元数据载体程序集**（类型/字段/签名完整，方法体被剥离）。
**真实逻辑编译在 `GameAssembly.dll` 的原生代码中。**

### 仍然成立的价值：完整的游戏 API 地图

| 资产 | 数量 |
|---|---|
| 类型/方法/字段名（#Strings） | 393,680 条（去重 357,903） |
| 字符串字面量（#US） | 49,364 条（去重 49,174） |

据此已定位的关键 API：

- `Utage.Crypt`：`EncryptXor` / `DecryptXor`，签名 `(byte[] key, byte[] buffer[, int offset, int count])`
  → **解密就是"调用方传 key 的 XOR"**，算法极简，难点全在 **key 的来源**
- 字符串字面量 **`SetAssetBundleDecryptKey`**（反射调用名或配置键）
- `Utage.WWWEx`：`LoadFromCacheOrDownloadAssetBundleAsync` / `DownLoadAssetBundleAsync` / `LoadAssetBundleAllAsync`
- `Utage.AssetFileUtage` / `AssetBundleInfo` / `AssetBundleInfoManager` / `AssetBundleHelper`
- `Framework.ResSys.ResourceManager.*`（自研资源系统）
- `<AssetBundleHash>k__BackingField` / `GetAssetBundleHash` / `ComputeHash` / `CalculateMd5`

### 由此确定的正确路线

读逻辑必须走**原生**：拿到「名称 ↔ 原生地址」映射 → Ghidra。

### ❗ 运行时实测：元数据是**按需解密**，没有可 dump 的完整明文

在运行中的游戏（pid 23992，4.4 GB 工作集）里扫描全部可读区间（3,309 个区间 / 5,301 MB）：

```
META? @0x1f282b70000  ver=17923304 (0x01117CE8)  prot=r--  range=17924096
META? @0x1f283c9d040  ver=17923304 (0x01117CE8)  prot=rw-  range=17924096
```

**判读：**

1. `version` 字段仍是**与磁盘版相同的垃圾值**，没有变合法 ⇒ 没有解密后的完整 blob。
2. 命中区间大小 **17,924,096 字节 = 17.09 MB**，恰好等于 `global-metadata.dat` 的磁盘大小，
   且存在 `r--` 与 `rw-` 两份 ⇒ 典型 mmap 映射。
3. ⇒ 游戏把**加密文件直接映射进内存，按需解密单个元数据结构**。

**后果**：早前计划的「dump 解密元数据 → Il2CppDumper → 得到方法名↔RVA」**这条路是断的**。

**替代路线**（按代价排序）：

| 路线 | 说明 | 代价 |
|---|---|---|
| A. 托管 DLL 取名称 + `Il2CppCodeRegistration` 取地址 | IL2CPP 与托管元数据信息等价，名称已免费获得；方法指针表在 `GameAssembly.dll` 的 data 段（未加密） | 中 |
| B. Hook 元数据访问函数，逐结构捕获解密结果 | 能拿到全部元数据，但需先定位访问点（鸡生蛋） | 高 |
| C. 放弃名称，Ghidra + 字符串 xref 硬啃 | 无前置依赖 | 高 |

### 战略结论：做模型替换 Mod **不需要**游戏的 C# 逻辑

这是本轮最重要的判断。模型替换依赖的是 **Unity 引擎侧**的 API：

- `UnityEngine.Mesh` / `AssetBundle` / `PersistentManager`
- `GfxDevice`（图形抽象层）
- D3D11 / Vulkan 命令流

这些都在 **`UnityPlayer.dll`** 与图形 API 层，**不在 `GameAssembly.dll`**（那是游戏逻辑）。
而 Unity 引擎内部的符号/特征有大量公开积累，定位成本远低于游戏自身的 IL2CPP 原生代码。

⇒ **不必为了 hook 点去攻克元数据加密。** §6 的路径 C（图形 API 层）与
路径 B（原生 Mesh 对象层）都不依赖游戏 C# 逻辑。

因此：

- **做 Mod**：走 Unity 引擎侧 / 图形 API 侧，与元数据加密无关。
- **读游戏逻辑**（例如找资源校验函数）：才需要上面 A/B/C 之一。

### 待验证

- [x] ~~运行时内存中的解密元数据~~ → **已证伪：按需解密，无完整明文**
- [ ] `Codes/` 目录与 bundle 的完整性校验实现在哪（原生 or 自研资源系统）
- [ ] `.u` 变体 12 字节头字段含义与解包方式
- [ ] 游戏需登录到主界面才会加载模型 bundle（当前停在启动阶段，未观测到 bundle 解密）
- [ ] 实测图形 API（D3D11 / D3D12 / Vulkan）——决定兜底方案形态

### 本轮方法论沉淀

以下手法已提炼进 `.agents/skills/unity-il2cpp-game-recon/`：

- `references/asset-format-and-obfuscation.md`：UnityFS 头结构、已知明文 + `fileSize`
  自校验、密钥周期判定的交叉验证、多形态陷阱、正文加密判定
- `references/frida-il2cpp-runtime.md`：Windows Frida 自举、两阶段 payload、
  内存扫描的性能与双重校验、按需解密识别、.NET 程序集读取工具选型（Cecil vs dnfile vs ILSpy）
- `SKILL.md`：主工作流与常见坑

---

## 9. 第二轮实测：图形 API、正文加密、离线能力

### 9.1 图形 API = **D3D11**（已确认）

运行中进程（pid 23992）已加载模块：

```
d3d11.dll   2.37 MB   C:\WINDOWS\SYSTEM32\d3d11.dll
dxgi.dll    1.23 MB   C:\WINDOWS\SYSTEM32\dxgi.dll
```

**未加载** `d3d12.dll` / `vulkan-1.dll` / `nvoglv64.dll`。

**意义**：§6 路径 C（图形 API 层 / 3Dmigoto 风格）**直接可行**，且落在生态最成熟的
D3D11 上。无需处理 D3D12 描述符堆或 Vulkan 内存类型。

### 9.2 离线解析能力：明文 bundle ✅ 可解析

**`UnityPy 1.25.4` 可直接解析 `StreamingAssets/aa/StandaloneWindows64/*.bundle`。**

关键前提：这些 bundle 的 `engineVersion` 字段是 **`"0.0.0"`**（非合法 Unity 版本串），
必须显式设置兜底版本，否则 UnityPy 报 `No valid Unity version found`：

```python
import UnityPy
UnityPy.config.FALLBACK_UNITY_VERSION = "2019.4.29f1"
env = UnityPy.load(path)
```

实测 `aotres_scenes_all_*.bundle`（13.8 MB）：

```
objects = 1643
  MonoBehaviour 416, GameObject 366, RectTransform 338, CanvasRenderer 288,
  CanvasGroup 46, Material 32, Texture2D 31, Sprite 29, AnimationClip 20, ...
containers: Assets/Scene/Login.unity, Assets/Scene/Start.unity
```

⇒ **`StreamingAssets/aa/` 下的资产可以完全离线提取与修改。**

### 9.3 LocalCache bundle：**头可解，正文加密**

#### 头部混淆已完全解出（394/400 个 bundle 自校验通过，98.5%）

结构：**每 4 字节字为一组；偶数字被 XOR，奇数字为明文。**

```
密钥 = 8 字节周期 [K(4B)][PAD(4B)]
  K   = head[0..3] XOR "Unit"     ← 可从最前 4 字节直接反推，无需知道引擎版本
  PAD = 0x00000000 或 0xFFFFFFFF  ← 由 head[4..7] 判定：
        等于 "yFS\0"      → PAD 全 00
        等于 ~"yFS\0"     → PAD 全 FF
```

实测分布（400 个样本）：`PAD=00` 203 个、`PAD=FF` 191 个；
解出的 `version`/`engine` 字段在 394 个文件上完全一致。

**细节**：存在 16 字节的细微周期——解出的 `version=0x02000007`、
`engine="2019.4,29f1"` 两处都只在所属字第 0 字节偏差 `0x02`。
即真实密钥流为 `[K][PAD][K^2][PAD]`。但**该偏差不影响 `fileSize` 读取**，
因此头解密对提取用途已够用。

#### 正文加密（已用参数扫描排除所有"仅头加密"假设）

用 `UnityPy` 作为判据，扫描 `{key8, key16} × K∈{50,64,96,128,192,256,512,1024,4096,全部}`，
3 个样本全部失败，一律报 `LZ4BlockError: Decompression failed`。

⇒ **正文使用未知方案另行加密**，头密钥流外推无效。

**因此：LocalCache 的 67.8 GB 资产无法离线解密**，需要
(a) 从原生代码取得正文解密算法，或 (b) 运行时捕获明文。
受 §8 的元数据按需解密限制，(a) 代价很高；**(b) 是现实路径**。

### 9.4 更新后的可行性结论

| 目标 | 状态 | 依据 |
|---|---|---|
| 图形 API 层 hook（D3D11 / 3Dmigoto 风格） | **可行，且最优** | §9.1 D3D11 确认 |
| 原生 Mesh 对象层 hook | 待定 | 需 Unity 引擎内部符号；与游戏 C# 无关 |
| 离线修改 `StreamingAssets/aa/` 资产 | **可行** | §9.2 UnityPy 实测成功 |
| 离线解密 `LocalCache` 资产 | **不可行** | §9.3 正文加密 |
| 读游戏 C# 逻辑 | **受阻** | §8 元数据按需解密 + 托管方法体为填充 |
| 资源校验点定位 | 待定 | 需运行时观测校验触发时机 |

### 9.5 仍未解决

- [ ] 需登录到主界面才能观测 bundle 解密与校验触发（当前停在启动阶段）
- [ ] 正文加密方案（只能在原生代码中找到）
- [ ] 校验是否发生在 Addressables catalog/CRC 层，还是自研资源系统层

---

## 10. 第三轮：密钥来源追查（含一处重要纠正）

### 10.1 ❗ 纠正：`Utage.Crypt` **不是**资产解密器

早前把 `Utage.Crypt` 当作资源解密核心。**该判断错误。**

Utage（宴）是 MadnessLab 的公开 Unity ADV 框架。查阅其使用者文档可知：
**Utage 本身不对 AssetBundle 做加密**，加密需开发者自行用 `CustomLoadManager` 实现。

> 「宴にはリソースをサーバや AssetBundle から読み込む仕組みもあるのですが、
> 暗号化はしてくれません。」

⇒ `Utage.Crypt` 的 `EncryptXor`/`DecryptXor` 更可能用于存档或剧本文本。
**GF2 的资产加密是游戏自研实现**，与 Utage 无关。

这也解释了为什么 `SetAssetBundleDecryptKey` 这个字符串字面量存在于游戏代码中——
那是游戏自己的接口。

### 10.2 每文件 4 字节密钥 K 的来源：**全部否定**

K 的获取方式（已验证 394/400 通过 `fileSize` 自校验）：
`K = head[0:4] XOR "Unit"`

用 400~500 个样本逐一否定：

| 假设 | 结果 |
|---|---|
| K = 文件名 hex 解出的前/后 4 字节 | 0/400 |
| K = md5(文件名字符串)[0:4] / [4:8] | 0/400 |
| K = md5(文件名 + ".bundle")[0:4] | 0/400 |
| K = md5(完整路径)[0:4] | 0/400 |
| K = 文件大小（LE / BE） | 0/400 |
| K = md5(前 4MB 内容)[0:4] | 0/60 |
| K = 哈希 ⊕ 固定 salt（8 种来源各测） | 全部 500 种不同差值 ⇒ 无固定 salt |
| K 以明文 4 字节形式存在于任一 <64MB 小文件中 | 无命中（扫 1,435 个文件） |
| bundle 的 32-hex 名出现在小文件中 | 无命中（含 `catalog.bin`） |

**结论：K 来自外部元数据或某种尚未识别的变换，离线反推失败。**

### 10.3 但发现了算术结构信号

`K` 的位分布并非随机：

```
bit00: 23.8%   ← 显著偏低
bit01: 36.0%
bit02: 41.6%
bit03: 47.4%
bit04~31: 接近 50%
```

等价地，`head[0]` **有 76.5% 是偶数**，且 `head[0] & 7` 的计数呈几何衰减：

```
0 → 124, 4 → 79, 2 → 63, 6 → 40, 1 → 26, 5 → 25, 3 → 23, 7 → 20
```

**低位趋于零、且按 2 的幂衰减** —— 这是**对齐值 / 长度 / 指针类整数**的典型指纹，
而不是加密哈希。说明 `head[0:4]` 可能不是纯密文，或密钥本身含算术成分。

**该信号值得后续重点追查**，是目前最有价值的未解线索。

### 10.4 IL2CPP 方法指针表：静态与运行时都不匹配

- 静态扫描 `GameAssembly.dll`：指向可执行段的最大指针数组 = **130,562** 项
- 运行时扫描同一模块内存：结果**完全一致**（同长度、同偏移）
  ⇒ 该结构在文件中即为静态，**不是加载后填充**
- 但 `Assembly-CSharp` 单个程序集就有 **201,514** 个 `MethodDef`，全游戏 40+ 程序集
  总数应在 40 万以上

⇒ 130,562 不是全量方法指针表；"方法定义索引 → 原生地址"的映射**尚未取得**。

### 10.5 新发现的第三类资产库

`LocalCache/Data/ClientRes_Windows/4.0.5136/A5CFF04BAF8EAC27EF4D4716C075F344/`
= **1,031 个 `.bytes` 文件 / 7.4 MB**，全部加密，头为：

```
E4 B3 8A 9E AC FF E6 6C F2 F5 E5 F5 FB F7 FB F7 F7 87 A9 FF FF FF FF FF FF FF FF FF FF FF D7 88
```

与 `AssetBundles_Windows` 的 UnityFS 头混淆**完全不同**，是另一套容器/混淆。

### 10.6 当前判定

「完全破解密」在**纯离线**条件下受阻于两点：

1. **K 的来源未知**（非文件名/大小/内容哈希，无固定 salt，未存于小文件）
2. **正文加密方案未知**（非头密钥流外推）

两者都指向同一个动作：**取得原生解密代码**，而这需要
(a) 「方法定义索引 → 原生地址」映射（§10.4 未取得），或
(b) 运行时 hook 解密函数 / 捕获明文（受限于 §8 按需解密 + 游戏未过登录）。

**下一步优先级**：
1. 登录游戏 → 运行时捕获明文 bundle（绕开算法，直接可用）
2. 追查 §10.3 的算术结构信号（最有希望的离线突破口）
3. 继续找全量方法指针表（换特征：可能在 `.pdata` 或独立注册结构中）

---

## 11. 第四轮：头部机制**完全解明**，正文确认独立加密

### 11.1 ✅ 机制突破：密钥 = 符号扩展的 int32（500/500 验证）

之前把 `PAD` 时而 `00000000` 时而 `FFFFFFFF` 当成"两种密钥"。**这是误判。**

实测统计（500 个样本）：

```
pad=FF 的数量 = 243
X[3] 最高位为 1 的数量 = 243
两者同时成立的数量 = 243        → 完全重合
PAD == X 的符号扩展：500 / 500  （100.00%）
```

⇒ `PAD` 完全由 `X` 的**符号位**决定，即 C# 里的 `int32 → long` **符号扩展**：

```csharp
long key64 = (long)(int)X;                    // 符号扩展
byte[] key = BitConverter.GetBytes(key64);    // 恒为 8 字节
for (i...) buffer[i] ^= key[i % 8];
```

**因此密钥实质是单个 int32 `X`**，密钥流 = `[X0 X1 X2 X3 sign sign sign sign]`，周期 8。

**连带纠正**：早前 §9.3 记录的"16 字节细微周期 `[K][PAD][K^2][PAD]`"是伪影。
真实明文里 `version` 字段就是 `0x02000007`、`engineVersion` 就是 `"2019.4,29f1"`。
这两个值虽非标准，但在**全部 394 个样本上完全一致**，说明是发行版本身如此
（游戏很可能改过 header 写入逻辑），不是解密错误。

**该结论是确定性的**：符号扩展解释了 100% 的 PAD 取值，无需任何额外假设。

### 11.2 ❌ 正文（blocksInfo + data）：独立方案，非头密钥流任何变体

用官方 `lz4` 库直接检验 blocksInfo 能否解出声明的 `ubis`：

| 文件 | size | cbis | ubis | 明文 blocksInfo | 解密后 blocksInfo |
|---|---|---|---|---|---|
| 000a54a5… | 1,009,077 | 710 | 363 | FAIL | FAIL |
| 0013549f… | 1,168,305 | 13 | 1123 | FAIL | FAIL |
| 0025a79a… | 20,815,934 | 1604 | 2433 | FAIL | FAIL |
| 00276f6e… | 1,293,121 | 785 | 533 | FAIL | FAIL |
| 002d223b… | 4,937,444 | 801 | 573 | FAIL | FAIL |
| 00375de5… | 1,135,883 | 785 | 513 | FAIL | FAIL |

再做组合扫描（4 个文件，共 **612 组**）：

- keystream 变体 9 种：`k8` / `k8_rev` / `k8_swap` / `x4` / `x4_rev` / `x4_not` /
  `x4_inc` / `x4_dec` / `pad_only`
- blocksInfo 偏移微调：`hdr-8 .. hdr+8`
- 相位扫描（另一实验）：period 8/16 × 全相位 × 全解/仅解头

**全部失败。**

⇒ **正文使用与头部完全无关的加密方案。** 头部密钥流外推无效。

### 11.3 关键判断：**头可解 ≠ 资产可用**

现有能力边界必须说清：

| 能力 | 状态 |
|---|---|
| 解出 UnityFS 头（fileSize/cbis/ubis/flags） | ✅ 394/400（98.5%） |
| 定位 blocksInfo 与 data 区段 | ✅ 由头部字段可算 |
| 解出 blocksInfo 内容 | ❌ |
| 解压 data block | ❌ |
| **用 UnityPy 读取加密 bundle** | ❌ |
| **用 UnityPy 读取明文 bundle**（StreamingAssets/aa） | ✅ 1643 对象 |

**因此"破头部"对提取资产没有实质帮助**——只证明了算法族，拿不到内容。

### 11.4 结论与建议

本轮把问题收敛为一个**单一未知量**：
**正文加密方案**。其余全部已解明（引擎版本、保护层、头机制、密钥形态、资产拓扑）。

要取得正文方案，只剩两条路：

| 路径 | 前提 | 代价 |
|---|---|---|
| **A. 运行时捕获明文** | 游戏登录到会加载模型 bundle 的界面 | 低（Frida attach + 内存扫描） |
| **B. 读原生解密代码** | 「方法索引→地址」映射，或运行时定位解密函数 | 高 |

**A 明显优先**，且 A 一旦成功，就**不需要再破算法**即可直接做资源替换——
这正是用户要求的目标（hook 资源替换）。

**B 的现实起点**：`Utage.Crypt.DecryptXor(byte[] key, byte[] buffer, int offset, int count)`
的签名与已解明的头机制高度吻合（接收 key 参数、按 offset/count 处理）。
若能拿到其原生地址，读 20 行汇编写出算法即可。

### 11.5 本轮方法论沉淀

- `references/asset-format-and-obfuscation.md` 已补：
  「框架的 Crypt 类 ≠ 资产解密器」、「每文件密钥否证清单」、「位分布别忽略」
- 新增判据：**`PAD` 呈 00/FF 二值且与某字节符号位完全相关 ⇒ 是符号扩展，不是两种密钥**
- 新增教训：**头部解出 ≠ 内容可读**。必须在"能否解压/解析内容"这一层设判据，
  否则会高估进展。

---

## 12. 第五轮：转向 Unity 引擎侧 —— 拿到资源校验与加载链路的确定地址

### 12.1 为什么转向 UnityPlayer.dll

游戏自身的解密器被 `global-metadata.dat` 按需解密挡住（§8），方法指针表也没找到（§10.4）。
但**资源最终必然经过 Unity 自己的 AssetBundle API**，而 `UnityPlayer.dll`：

- 字符串**明文**（不像 `global-metadata.dat` 被加密）
- `.pdata` 含 **87,065 个 RUNTIME_FUNCTION 记录** = 全量函数边界
- 因此可以用「字符串 → 交叉引用 → `.pdata` 定位函数入口」三步拿到任意 API 地址

### 12.2 已定位的确定地址

**镜像基址 0x180000000**，以下为 RVA（运行时 VA = 模块基址 + RVA）：

| 用途 | RVA | 说明 |
|---|---|---|
| **错误文案格式化器** | **0x2E3790** | 已实测挂接成功，见 §12.5 |
| CRC 文案引用点 | 0x2E37EC / 0x2E41F0 / 0x2E6087 | 三条 `lea rdx, [rip+...]` |
| CRC 文案字符串 | 0x14872B0 | `"AssetBundle '%s' has invalid crc!"` |
| 错误码跳转表 | 0x2E38DC | 12 项 int32，相对镜像基址 |

**错误码跳转表已完整解出（0x2E38DC，12 项）：**

| 码 | 分支 VA | 码 | 分支 VA |
|---|---|---|---|
| 0 | 0x1802E37B8 | 6 | 0x1802E384C |
| 1 | 0x1802E37D4 | 7 | 0x1802E3864 |
| **2** | **0x1802E37EC** ← **CRC 错误** | 8 | 0x1802E387C |
| 3 | 0x1802E3804 | 9 | 0x1802E3894 |
| 4 | 0x1802E381C | 10 | 0x1802E38C4 |
| 5 | 0x1802E3834 | 11 | 0x1802E38AC |

⇒ **错误码 2 = 资源 CRC 校验失败**。这是"资源校验"这条线的关键常量。

**该格式化器的调用约定**（由反汇编确定）：

```
rcx = 输出对象（内含 std::string 缓冲）
edx = 错误码（0..11）
r8  = AssetBundle 名称字符串（为 0 时取 [rcx+0x20]）
```

**11 处调用点（RVA）**，即资源加载的错误出口：

```
0x2D9B8C  0x2D9C7E  0x2DA456  0x2DA4FB  0x2DA760  0x2DA7B8
0x2DA824  0x2DA8D3  0x2DA947  0x2DA9C0  0x2DAA37
```

### 12.3 已定位的 Unity AssetBundle icall 名称（明文）

`.rdata` 中连续排列，可直接用于匹配/断点：

```
0x14AB620  UnityEngine.AssetBundle::LoadFromFile_Internal
0x14AB688  UnityEngine.AssetBundle::LoadFromMemory_Internal
0x14AB650  UnityEngine.AssetBundle::LoadFromMemoryAsync_Internal
0x14AB5E8  UnityEngine.AssetBundle::LoadFromFileAsync_Internal
0x14AB788  UnityEngine.AssetBundle::LoadAsset_Internal
0x14AB7F0  UnityEngine.AssetBundle::Unload
0x14AB840  UnityEngine.AssetBundle::GetAllScenePaths
```

注：这些名字符串的地址**未**以绝对指针形式出现在 .rdata/.data 中（8 字节 VA 与
4 字节 RVA 两种形式均查无结果），说明该 icall 表要么用其他编码，要么在运行时构造。
**因此「由名字直接读函数指针」这条路未走通。**

### 12.4 一处自我纠正

先前推断 `0x2D7E90` 是"资源校验函数"（因为它的返回值进了 `esi` 再作为错误码传入）。
**反汇编后证伪**：该函数仅 248 字节，含
`cmp byte ptr [r12 + r8], dil` + `inc r8` 的 strlen 循环，以及 `0x4b`/`0x49` 这类
std::string 小字符串优化容量常量 ⇒ 它是 **std::string 辅助函数**，不是校验器。

CRC 错误码 2 是**经 `esi` 动态传入**的，因此校验逻辑在别处。
**教训：仅凭"返回值进了错误码参数"就断定函数职责，是不可靠的。**

### 12.5 可执行产物：错误探针（已实测挂接成功）

`C:\DevCache\gf2\gf2_assetprobe.js` —— 挂 `UnityPlayer.dll+0x2E3790`，记录
`(错误码, bundle 名, 返回地址/所属模块+偏移)`。

实测：

```
UnityPlayer.dll base=0x7ff8ebe30000  错误格式化器=0x7ff8ec113790
已挂上错误格式化器，等待资源加载事件…
```

**挂接成功、地址正确。** 45 秒内无事件（游戏停在启动界面，无资源加载失败）。

**该探针的价值**：一旦游戏加载资源，它能直接回答两个悬而未决的问题：

1. **资源校验是否真的会触发？**（错误码 2 出现 = 校验存在且会拦）
2. **哪个 bundle 被判失败？**（r8 的 bundle 名）

这是目前**唯一已实测可用的运行时观测点**。

### 12.6 仍未解决

- [ ] 需要游戏进入会加载资源的界面，探针才有事件（当前停在启动界面）
- [ ] `LoadFromMemory_Internal` 的实际函数地址（icall 表编码未知）
- [ ] 正文加密方案（§11.2）
- [ ] 校验是发生在 Unity 内部 CRC 层，还是游戏自研资源系统层（§12.2 的证据指向
      Unity 内部确实有 CRC 校验端口，但是否被调用需运行时确认）

### 12.7 本轮方法论沉淀

新增一条对任意**去符号 64 位 DLL** 通用的手法（已写入
`references/static-native-triage.md`）：

**「字符串交叉引用 + `.pdata` 函数边界 + capstone」三件套，可在秒级定位目标函数，
完全绕开 Ghidra 全量分析的超时问题**（本项目 Ghidra 对 25MB 的 `UnityPlayer.dll`
直接超时，而该方法秒级出结果）。

---

## 13. 第六轮：两次全量扫描均 0 命中，以及由此得到的结论

### 13.1 扫描结果

| 扫描目标 | 模式 | 扫描量 | 命中 |
|---|---|---|---|
| 明文 UnityFS bundle | `"UnityFS\0"` + 头字段校验 | 5,297 MB / 3,315 区间 | **0** |
| 解密后 SerializedFile | `fileSize BE32 + version`（6 个已知文件大小） | 5,302 MB / 3,283 区间 | **0** |

### 13.2 ❗ 方法论纠正：内存里不存在"原始文件映像"

起初把 0 命中理解为"没加载资源"。**更根本的原因是方法错了。**

Unity 加载 AssetBundle / SerializedFile 时，**解析完头部就丢弃原始文件映像**，
只保留目录表、block 表等结构，之后按需解压 block。
因此**内存中不存在连续的、以 `UnityFS\0` 开头的原始 bundle 字节**可供扫描。

这与 §8 的发现一致：`global-metadata.dat` 是被 mmap 进内存并在加密状态下按需解密，
同样找不到"解密后的完整明文"。

**结论：对 Unity 加载后的资产，全内存扫描是无效手段。**
唯一可靠的捕获点是**拦截读取/解密/解压这一动作本身**。

### 13.3 进程状态实测：游戏活着，但完全空闲

| 指标 | 实测值 |
|---|---|
| 主窗口标题 | `少女前线2：追放`（有句柄） |
| CPU 累计 | 3,132.7 秒（约 1.1 核持续，说明在渲染） |
| 工作集 | 5,300 MB |
| 线程数 | 281 |
| `PCLauncher.exe` | 存活（pid 21976） |

**50 秒 I/O 追踪**（hook `CreateFileW` / `ReadFile`）：

```
opens: 0        ← 没有打开任何文件
reads: 46       ← 走的是早已打开的句柄
assetReads: 0   ← 零资产读取
```

⇒ **游戏确实停在标题/登录界面，不做任何资源加载。**

### 13.4 由此确定的运行时探索路线（按可行性）

| 路线 | 前提 | 状态 |
|---|---|---|
| **A. 进游戏到会加载 bundle 的界面，hook 解密/解压点** | 需要登录 | **唯一可靠路径** |
| B. 主动触发 bundle 加载（调用 Unity API） | 需 `LoadFromMemory` 地址 | 地址未取得（§12.3） |
| C. 全内存扫描持久明文 | — | **已证否**（§13.2） |

**A 的具体做法**（探针已就绪）：`gf2_assetprobe.js` 挂
`UnityPlayer.dll+0x2E3790`，登录后进入游戏，错误码 2 若出现即证明
「资源 CRC 校验实际生效」，同时给出被拦的 bundle 名。

### 13.5 完整可行性结论

**目标：模型替换 Mod**

| 环节 | 可行性 | 依据 |
|---|---|---|
| 确定图形 API | ✅ D3D11 | 已加载 `d3d11.dll`+`dxgi.dll` |
| 读懂游戏 C# 逻辑 | ❌ | 元数据按需解密（§8）+ 托管方法体为填充（§8） |
| 离线解密 LocalCache 资产 | ❌ | 正文独立加密，612 组否定（§11.2） |
| 离线解析 StreamingAssets 资产 | ✅ | UnityPy 实测 1643 对象（§9.2） |
| 定位资源 CRC 校验 | ✅ 已定位错误码 2 + 探针 | §12.2 / §12.5 |
| 定位 bundle 加载入口 | ⚠ 部分 | icall 表编码未知，`LoadFromMemory` 地址未取得 |
| 运行时捕获明文资产 | ⏳ 待验证 | 需进入游戏（§13.4） |
| hook 资源替换 | ⏳ 待验证 | 依赖上一行 |

**总体判断**：

- **做模型替换 Mod 在技术上可行**，但**不能走"离线解密资产"路线**——
  正文加密无法离线攻破。
- **必须走运行时路线**：在游戏解密/加载资产的瞬间拦截，用自备资源替换。
- 该路线的**关键未知量只剩一个**：`AssetBundle` 加载入口的运行时地址
  （或游戏自身解密函数的地址）。
- 拿到它之后，**不需要破任何加密**即可实现替换。

### 13.6 未决问题（交付清单）

1. `UnityEngine.AssetBundle::LoadFromMemory_Internal` / `LoadFromFile_Internal`
   的运行时地址 —— icall 表编码方式未知（§12.3）
2. 正文加密方案 —— 需原生代码或运行时捕获（§11.2）
3. 资源 CRC 校验是否在真实加载路径上被调用 —— 需进入游戏观测（§12.5）
4. 角色模型对应的 bundle 名 —— 需遍历 9,612 个 bundle 的目录表（需解密）
5. `SkinnedMeshMerger` 的合并/烘焙时机 —— 决定替换必须在加载层还是可在 Mesh 层
6. 第三类资产库（`A5CFF04…` 下 1,031 个 `.bytes`）的容器格式（§10.5）

### 13.7 本轮方法论沉淀

已写入 `references/frida-il2cpp-runtime.md`：

**「对 Unity 已加载的资产，全内存扫描找原始文件映像必然为 0 命中」**——
Unity 解析头部后即丢弃原始映像，只保留结构与按需解压的 block。
必须先判断"目标数据是否会被持久保留"，再决定扫描还是 hook。

同时新增诊断手法：**用窗口标题 + CPU 累计 + `CreateFileW`/`ReadFile` 计数
三重指标判断「进程是活着但在空转」还是「真的在做资源加载」**。
本项目曾因未做这个区分，把"空转"误读为"没加载到"。

---

## 14. 第七轮：✅ 找到明文主清单与明文资产名 —— 定位目标不再需要解密

本轮的发现**实质性改善了可行性**：可以在完全不碰加密的前提下**定位目标资产**。

### 14.1 ❗ 前提确认：`StreamingAssets` 里没有角色模型

| 内容 | 数量 | 大小 |
|---|---|---|
| `.bundle` | **4 个** | 13.9 MB（几乎全是 `aotres_scenes_all` 场景包） |
| `.bytes`（托管程序集） | 39 个 | 67.4 MB |
| `.usm`（视频） | 1 个 | 95.9 MB |
| 音频 | 5 个 | 2.1 MB |

⇒ **全部角色内容只存在于加密的 `LocalCache`**，**没有明文旁路可走**。

### 14.2 ✅ `catalog_main_28495.bin`：覆盖 97.3% bundle 的明文主清单

**位置**：`LocalCache/Data/AssetBundles_Windows/catalog_main_28495.bin`
**大小**：59,261,318 字节 — **未加密**

| 指标 | 值 |
|---|---|
| bundle ID 数（`<hash>.bundle`） | **68,723** |
| 覆盖磁盘上的 bundle | **9,353 / 9,612 = 97.3%** |
| 资源地址（`Assets/...`） | 833 |
| Addressables 版本 | 1.21.18 |

记录结构（长度前缀字符串内联存储）：

```
002351F4  "000300a5800521ac6ddb8d54246bf163.bundle"     ← 内部 ID
00235218  28 00 00 00                                    ← 长度 40
0023521C  "\000300a5800521ac6ddb8d54246bf163.bundle"    ← 带路径前缀
00235249  bf b4 47 81 ...                               ← 请求选项（含 CRC 等）
0023525C  "8e74fe9464de596d3450e6e6a1a15557"            ← 关联哈希
```

**重要限制**：833 个地址里 **786 个是 `Assets/ArtsResource/Scene`**，
即这是**场景清单**。角色模型**不走 Addressables 路径寻址**，
而是由自研资源系统按资产 ID 从 bundle 内部取。

### 14.3 ✅ 游戏数据表是明文 protobuf，含真实资产路径

`LocalCache/Data/Table/` 共 **2,288 个 `.bytes`**，**全部明文 protobuf**。
关键词统计：

| 关键词 | 出现次数 |
|---|---|
| `Avatar` | 11,881 |
| `Character` | 3,530 |
| `ArtsResource` | 2,445 |
| `prefab` / `Prefab` | 1,985 / 995 |
| `Body` | 332 |
| `Model` | 28 |
| `mesh` | 13 |

**资产路径是完整形态**（`StageSceneShiftData.bytes`，71.7 KB，345 个不同 `ArtsResource` 路径）：

```
Assets/ArtsResource/Scene/SLG_S/s_slg_simcombat01/SIM_Common/highwall_edge_mesh_sim1.prefab
Assets/Level/LevelItem/CoverItem/p_l_slg_gasTank_H_s01.prefab
Assets/ArtsResource/2DRes/0ob/Background_image/BG_Green_Room_Day_01J
Assets/ArtsResource/Effect/Effect_sum/Scene/SLGInteraction.prefab
```

**角色名**（`DialoguePageData.bytes` 等，1,899 个不同 `Avatar_Head_*`）：

```
Avatar_Head_BiyocaSSR / Avatar_Head_LeneSSR / Avatar_Head_MitylSSR
Avatar_Head_CheyanneSSR / Avatar_Head_PhaetusaSSR / Avatar_Head_NemesisSR
```

### 14.4 这一发现如何改变 Mod 方案

**原来**：必须先解密 bundle → 读目录表 → 才知道哪个 bundle 装了什么 → 才能替换。
**现在**：资产名与路径**从明文数据表直接可得**，因此可以**按名字定位**：

| 步骤 | 原方案 | 现方案 |
|---|---|---|
| 找到目标资产 | 需解密 bundle | **查明文 Table** ✅ |
| 找到目标 bundle | 需解密目录表 | 可跳过（按名字 hook） |
| 资产替换时机 | 需知 bundle 名 | **按资产路径/名匹配** ✅ |
| 资产内容 | 需解密 | 仍需运行时捕获或自备资源 |

⇒ **Mod 的定位环节已完全解锁**（不再依赖解密），
只剩**内容获取/替换**环节需要运行时方案。

### 14.5 更新后的可行性与 hook 方案

**推荐 hook 方案（按可行性排序）**：

| 序 | hook 点 | 依据 | 状态 |
|---|---|---|---|
| **1** | 按**资产路径名**匹配的资产加载调用 | §14.3 已获全部路径名 | 需定位该 API 地址 |
| 2 | 错误格式化器 `UnityPlayer+0x2E3790` | 已实测挂接成功（§12.5） | ✅ 可用于观测校验 |
| 3 | `AssetBundle::LoadFromMemory_Internal` | icall 表编码未知 | 地址未取得 |

**可行性结论更新**：

- **定位目标资产**：✅ 完全可行，无需解密（明文 Table + 明文 catalog）
- **观测校验**：✅ 探针已就绪（错误码 2）
- **获取/替换资产内容**：⏳ 需运行时拦截（正文加密仍无法离线攻破）
- **离线解密资产**：❌ 已证否（§11.2 的 612 组否定）

### 14.6 未决问题（更新）

1. 资产按路径/名加载的 API 地址（`AssetBundle::LoadAsset_Internal` 或自研资源系统入口）
2. 正文加密方案（需原生代码或运行时捕获）
3. 角色**模型本体**的资产名规范（当前只确认了 `Avatar_Head_*` 头像；身体/武器命名待查）
4. `SkinnedMeshMerger` 合并烘焙时机（决定替换层次）
5. 第三类资产库（`A5CFF04…` 下 1,031 个 `.bytes`）容器格式

### 14.7 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「资产加密 ≠ 清单加密」**——游戏常把**内容**加密，却把**清单（目录/表）**留明文。
本项目：9,612 个 bundle 正文加密无法离线解，但
① 59 MB 的 Addressables 主清单明文、② 2,288 个数据表明文且含完整资产路径。

因此**在动手破加密之前，必须先穷举"哪些元数据是明文的"**。
明文清单往往足以完成"定位目标"这一半工作，把需要破加密的范围大幅缩小。

---

## 15. 第八轮：✅ 角色模型资产命名规范完全解出

### 15.1 从明文数据表抽到的资产全貌

全量抽取 2,288 个 Table 文件，去重后仅 **1,176 条完整资产路径**，分布高度集中：

| 路径前缀 | 条数 |
|---|---|
| `Assets/_UI/UIRes/Atlas` | 482 |
| `Assets/ArtsResource/Scene/SLG_S` | 164 |
| `Assets/ConfigPrefab/Darkzone/DynamicInteractObject` | 142 |
| `Assets/ConfigPrefab/DarKov/InteractObject` | 140 |
| `Assets/Level/LevelItem/InteractItem` | 61 |
| `Assets/Level/LevelItem/CoverItem` | 41 |
| `Assets/ConfigPrefab/Character/Enemy` | 5 |

⇒ 这些表是**玩法配置**（关卡、交互物、教程），只引用场景道具与敌人预制体，
**不含玩家角色模型**。玩家角色由独立的头像/角色系统加载。

### 15.2 ✅ 角色资产命名规范（关键成果）

按 `Avatar_` 前缀归类全部命中：

| 前缀 | 出现次数 | 含义 | 是否 3D 模型 |
|---|---|---|---|
| `Avatar_Head_*` | 10,623 | 头像（UI 图集） | ❌ |
| `Avatar_Bust_*` | 272 | 半身像 | ✅ |
| **`Avatar_Whole_*`** | **106** | **全身** | ✅ |
| **`Avatar_Gacha_*`** | **61** | **抽卡展示** | ✅ |
| `Avatar_Half_*` | 12 | 半身 | ✅ |
| `Avatar_Character_*` | 6 | 编号角色 | ✅ |
| 其余 `Avatar_<名>` | 406 个不同名 | 基础标识 | — |

实例：

```
Avatar_Whole_UllridSSR      Avatar_Whole_HarpsySSR     Avatar_Whole_WelrodSSR
Avatar_Gacha_NikketaSSR     Avatar_Gacha_LittaraSR     Avatar_Gacha_LindSR
Avatar_Bust_Player_Male03   Avatar_Bust_ZhaohuiSSR     Avatar_Bust_LoreleySSR01
Avatar_Biyoca / Avatar_Basti / Avatar_Alva / Avatar_Andoris
```

**命名模式**：`<名><稀有度><序号>`
（稀有度如 `SSR`/`SR`；序号如 `01`；变体后缀如 `b`/`P`）

### 15.3 对 Mod 方案的意义：hook 按名字匹配即可

**模型替换的目标资产集合已明确**：

| 优先级 | 目标 | 理由 |
|---|---|---|
| **1** | `Avatar_Whole_*` | 全身模型，最可能是大厅/展示用 |
| **2** | `Avatar_Gacha_*` | 抽卡展示模型（特写角度，替换效果最直观） |
| **3** | `Avatar_Bust_*` | 半身像 |
| 备选 | `Avatar_<名>` 基础名 | 可能是根预制体 |

⇒ hook 逻辑可以简化为：**拦截资产加载，匹配名字前缀 `Avatar_Whole_` / `Avatar_Gacha_`**，
无需知道 bundle 哈希、无需解密目录表。

### 15.4 合并烘焙类已确证

托管 DLL 字符串中确认存在以下类（决定替换时机）：

```
SkinnedMeshMerger
SkinnedMeshMergerLOD
SkinnedMeshNode
SkinnedMeshRenderer
SkinnedMeshRenderers
```

⇒ 角色模型加载后会经过 `SkinnedMeshMerger` **合并/烘焙**。
因此**替换必须发生在合并之前**（即资产加载层），
这正是选择"hook 资产加载"而非"事后替换 Mesh 对象"的技术依据。

### 15.5 最终 hook 方案（可执行）

```
拦截点：资产加载 API（按路径/名匹配）
匹配条件：name.StartsWith("Avatar_Whole_") || name.StartsWith("Avatar_Gacha_")
动作：   （a）观测：记录加载的资产名 → 确认目标资产清单
         （b）替换：用自备资产替换（需先解决正文解密/运行时捕获）
时机要求：必须早于 SkinnedMeshMerger 的合并烘焙
```

**已就绪的观测工具**：
- `gf2_assetprobe.js` —— 挂 `UnityPlayer.dll+0x2E3790`，观测 CRC 校验（已实测挂接成功）
- `parse_catalog.py` —— 解析明文主清单
- `extract_asset_paths.py` —— 抽取明文数据表资产路径

### 15.6 可行性结论（最终）

| 环节 | 结论 | 依据 |
|---|---|---|
| 定位目标资产 | ✅ **完全可行，无需解密** | §15.2 命名规范 + §14.3 明文表 |
| 确定 hook 时机 | ✅ 已确定 | §15.4 `SkinnedMeshMerger` 要求必须在加载层 |
| 观测资源校验 | ✅ 工具就绪 | §12.5 探针 |
| 获取/替换资产内容 | ⏳ 需运行时 | 正文加密无法离线破（§11.2） |
| 离线解密资产 | ❌ 已证否 | §11.2 的 612 组否定 |

**总体判定：模型替换 Mod 技术上可行。**
**定位与时机问题已全部解决**，唯一剩余工程问题是"取到并替换资产内容"，
需要在游戏加载资产的瞬间拦截（运行时方案），而非离线解密。

### 15.7 未决问题（最终清单）

1. 资产加载 API 的运行时地址（`AssetBundle::LoadAsset_Internal` 或自研
   `Framework.ResSys.ResourceManager` 入口）—— 这是最后一个工程阻塞点
2. 正文加密方案（仅在"需要离线准备资产"时才是问题；运行时拦截可绕过）
3. `Avatar_Whole_*` 等资产实际落在哪些 bundle（可在运行时由 hook 直接观测，无需离线解密）
4. 第三类资产库（`A5CFF04…` 下 1,031 个 `.bytes`）容器格式
5. 替换后的资产需满足 `SkinnedMeshRenderer` 的骨骼/bindpose 兼容约束

### 15.8 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「资产命名规范要从明文数据表反推，它是 name-based hook 的前提」**——
本项目的角色资产并非从 catalog 或 bundle 目录得知，而是从
`Table/*.bytes` 里 `Avatar_*` 前缀的出现频次推导出来的：
`Head`(10623) / `Bust`(272) / `Whole`(106) / `Gacha`(61)。

**手法**：对明文数据表做「前缀归类统计」——按第一个/前两个下划线分段聚合，
出现次数最高的通常是 UI 资源，而**次数少但语义明确的才是 3D 模型**
（本项目 `Whole`/`Gacha` 分别只有 106/61 次，正是模型本体）。

---

## 16. 第九轮：✅ 攻克 icall 地址 —— hook 框架已可执行并实测运行

这一轮解决了**最后一个工程阻塞点**：拿到 Unity AssetBundle API 的运行时地址。

### 16.1 关键发现：`GameAssembly.dll` 内含 IL2CPP 的 icall 签名字符串

```
0x3FFA940  "UnityEngine.AssetBundle::LoadFromMemory_Internal(System.Byte[], ..."
0x3FFA730  "UnityEngine.AssetBundle::LoadFromFile_Internal(System.String, ..."
0x3FFA990  "UnityEngine.AssetBundle::LoadAsset_Internal(System.String, ..."
```

（含**完整参数类型**。注：这些串未被 RIP 相对 `lea` 从 `.text` 引用，
必须在 **`il2cpp` 段** 里扫才找得到——该段是 54 MB 的 IL2CPP 生成代码。）

### 16.2 ✅ 机制：IL2CPP 惰性解析 icall，结果存进静态槽

生成代码形态（`0x181375628` 处，`LoadFromMemory_Internal`）：

```asm
mov  rax, [rip + 0x422de55]      ; 读缓存
test rax, rax
jne  <已解析则跳过>
lea  rcx, [rip + …]              ; rcx = 签名字符串
call 0x18008d670                 ; 解析器
test rax, rax
je   <失败分支>
mov  [rip + 0x422de38], rax      ; ← 把真实函数指针存进静态槽
call rax                         ; 调用
```

⇒ **槽里存的就是 `UnityPlayer.dll` 内的真实函数地址。运行时读槽即得。**

**解析器入口：`GameAssembly.dll + 0x8D670`**
（入参 `rcx` = 签名字符串，返回 `rax` = 函数指针）

### 16.3 ✅ 已解出的全部槽 RVA

| icall | 槽 RVA | 签名 RVA |
|---|---|---|
| `LoadFromMemory_Internal` | **0x55A3498** | 0x3FFBF40 |
| `LoadFromFile_Internal` | **0x55A3448** | 0x3FFBD30 |
| `LoadFromMemoryAsync_Internal` | 0x55A3478 | 0x3FFBEE0 |
| `LoadFromFileAsync_Internal` | 0x55A33B0 | 0x3FFBCD0 |
| `LoadAsset_Internal` | **0x55A3420** | 0x3FFBF90 |
| `LoadAssetAsync_Internal` | 0x55A3370 | 0x3FFBFE0 |
| `LoadAssetWithSubAssets_Internal` | 0x55A33A0 | 0x3FFC0E0 |
| `Unload` | 0x55A33F8 | 0x3FFBD90 |
| `GetAllAssetNames` | 0x55A3438 | 0x3FFC168 |
| `GetAllScenePaths` | 0x55A33F0 | 0x3FFC0A8 |
| `Contains` | 0x55A34B8 | 0x3FFC070 |

### 16.4 ✅ 实测：槽机制有效（已运行验证）

`gf2_loadhook.js` 实测输出（pid 23992，2026-10-18）：

```
GameAssembly.dll base=0x7ff8c4350000
  槽 LoadFromFileAsync_Internal -> 0x7ff8ec1e7650 @ UnityPlayer.dll   ✅
  槽 LoadAsset_Internal         -> 0x7ff8ec1e74a0 @ UnityPlayer.dll   ✅
  槽 LoadAssetAsync_Internal    -> 0x7ff8ec1e6fb0 @ UnityPlayer.dll   ✅
  槽 LoadFromFile_Internal      -> null（未解析）
  槽 LoadFromMemory_Internal    -> null（未解析）
已挂 LoadAsset_Internal @ 0x7ff8ec1e74a0
```

**关键规律：icall 是惰性解析的**——只有被调用过的才填槽。
所以 `LoadFromFile/Memory` 仍为 null（游戏尚未加载过 bundle）。

⇒ 由此确定**三层 hook 策略**（`gf2_loadhook2.js`，已实测运行）：

| 层 | 手段 | 作用 |
|---|---|---|
| **L1** | hook 解析器 `+0x8D670` | 任何 icall 一解析即捕获，不必等槽填上 |
| **L2** | 每 1.5 秒轮询槽 | 捕捉惰性解析结果 |
| **L3** | 拿到地址即刻挂接 | 采集 path/crc/资产名 |

实测：解析器挂在 `0x7ff8c43dd670`，两层同时生效，已挂上 `LoadAsset_Internal`。

### 16.5 ✅ 可执行产物清单

| 文件 | 作用 | 状态 |
|---|---|---|
| **`gf2_loadhook2.js`** | **最终 hook 框架**（解析器+槽轮询+按需挂接） | ✅ 实测运行 |
| `gf2_loadhook.js` | 槽读取版（验证机制） | ✅ 实测运行 |
| `gf2_assetprobe.js` | CRC 错误码探针（`UnityPlayer+0x2E3790`） | ✅ 实测运行 |
| `parse_catalog.py` | 解析明文主清单 | ✅ |
| `extract_asset_paths.py` | 抽取明文数据表资产路径 | ✅ |
| `find_all_icall_slots.py` | 批量计算 icall 槽位 | ✅ |
| `unityfs_probe.py` / `verify_key.py` | 头部解密验证 | ✅ |

### 16.6 目标达成情况

| 目标要求 | 状态 |
|---|---|
| 摸清资源加载链路 | ✅ Unity 2019.4 IL2CPP → NEP2 拦截文件读取 → 解密 → AssetBundle → SerializedFile → **SkinnedMeshMerger 合并烘焙** |
| 确定最适合的 hook 点 | ✅ **资产加载层**：`LoadFromFile/Memory_Internal`（bundle+CRC）与 `LoadAsset_Internal`（资产名），均在 `SkinnedMeshMerger` 之前 |
| 产出可执行的 hook 方案 | ✅ `gf2_loadhook2.js` 已实测运行，三层策略 |
| 可行性结论 | ✅ 见 §15.6 |
| 固化到 docs 与 skills | ✅ 本台账 + `unity-il2cpp-game-recon` 技能 |

**剩余验证项**（需游戏进入会加载资源的界面，属外部前提）：
- `LoadFromFile_Internal` 的 `crc` 参数实际取值 → 确认资源校验是否生效
- `LoadAsset_Internal` 实际加载的 `Avatar_Whole_*` / `Avatar_Gacha_*` 资产名
- 目标资产实际落在哪个 bundle（hook 会直接给出）

### 16.7 本轮方法论沉淀

已写入 `references/frida-il2cpp-runtime.md`：

**「IL2CPP 的 icall 地址可以通过静态槽 + 解析器 hook 拿到，完全不需要解密元数据」**
——这是绕开 `global-metadata.dat` 加密的通用解法：

1. 在 `GameAssembly.dll` 的 **`il2cpp` 段**里搜 icall 签名字符串
   （不在 `.text` 引用，只在 `il2cpp` 段里 `lea`）
2. 从 `lea` 往后找 `mov [rip+D], rax` → 算出**静态槽 RVA**
3. 运行时读槽 → 得到 UnityPlayer 内真实地址
4. 槽是**惰性填充**的，所以还要 hook **解析器**（`lea` 后的 `call` 目标）
   才能捕获尚未被调用的 icall

---

## 17. 第十轮：⚠ 更正 + **资源校验实测结论（关键）**

### 17.1 更正：加密 bundle 走的是 `LoadFromStream`，不是 `LoadFromMemory`

早前把 1797 次加载判定为 `LoadFromMemory_Internal`。**该判定错误。**
解析器日志显示真实签名是：

```
UnityEngine.AssetBundle::LoadFromStreamInternal(System.IO.Stream, System.UInt32, System.UInt32)
```

原因：icall 签名在 GameAssembly 里是**长度前缀拼接的 blob**，
我的子串匹配在同一个 blob 里误命中了 `LoadFromMemory_Internal(`。

⇒ 游戏的做法是：**解密后包成 `Stream`，再交给 `AssetBundle.LoadFromStreamInternal`**。

### 17.2 ✅ 运行时实测：1797 个加密 bundle 的 **CRC = 0，不做校验**

调用点反汇编（`GameAssembly+0x2050CB0`，函数内偏移 `0x20511BC`）：

```asm
0x20511BC  mov r8d, esi          ; r8 = 缓冲大小（0x8000）
0x20511BF  xor edx, edx          ; ★ rdx = 0  —— CRC 参数就是 0
0x20511C1  mov rcx, rdi          ; rcx = Stream
0x20511C4  call rax              ; AssetBundle.LoadFromStreamInternal(stream, 0, 0x8000)
```

对照 `LoadFromFile_Internal` 的 4 次调用（这些**带真实 CRC**）：

```
crc=0x175fd061  aotres_assets_all_418d97e4b424a16053c3f4e2bff259e7.bundle
crc=0xa787c68b  e15b8935b10b0266fec2cac95f89fc7c.bundle
crc=0xfc18c503  aotres_scenes_all_ac7c0fef08ae85593834b63b7c3b3309.bundle
crc=0x1407deac  97d94d143d5e7e57ece1b6760333965b.bundle
```

### 17.3 结论：**Mod 替换不会被资源校验拦住**

| 资产来源 | 加载方式 | 数量 | CRC 校验 |
|---|---|---|---|
| `StreamingAssets/aa`（明文包） | `LoadFromFile_Internal` | 4 | ✅ 有，会校验 |
| `LocalCache`（加密包，含全部角色） | `LoadFromStreamInternal` | **1797** | ❌ **CRC=0，不校验** |

⇒ **全部角色资源所属的那批加密 bundle，加载时不做完整性校验。**
这正是做「hook 资源加载替换成自有资源」最关键的前提，实测已确认。

### 17.4 解密算法的定位进展

1797 次调用**共享同一条调用栈**（唯一栈签名），GameAssembly 内偏移：

```
0x20511C6  → 函数 0x2050CB0 (1505 B)   ← 直接调用 LoadFromStreamInternal
0x2050C9F  → 函数 0x2050990 (794 B)
0x204EB55  → 函数 0x204EAC0 (161 B)
0x2849D09  → 函数 0x2849B10 (608 B)
0x273DD22  → 函数 0x273DCA0 (238 B)
0x2739752  → 函数 0x2739480 (761 B)
0x2739D25  → 函数 0x2739CE0 (80 B)
0x204A0E5  → 函数 0x20498E0 (2678 B)
0x204B7C1  → 函数 0x204B4C0 (1893 B)
0x2049EBD  → 函数 0x20498E0 (2678 B)
```

调用点上下文（`0x2051141` 起）显示：`rdi = [rbx+0xa0]` 是一个带虚表的 Stream 对象，
经 `call [rax+0x1e0]` 取其长度/容量后传入。
⇒ **解密发生在这个 Stream 被填充的地方，即更上游的帧（0x204EAC0 / 0x2739480 一线）。**

**下一步**：沿 `rdi` 这个 Stream 对象回溯其数据来源，或在 `0x2050990` / `0x204EAC0`
里找「分配缓冲 → 写入明文」的位置，再对写入点下硬件断点捕获解密例程。

### 17.5 本轮方法论沉淀

已写入 `references/frida-il2cpp-runtime.md`：

**「icall 签名是拼接 blob，绝不能用子串匹配来识别 icall 身份」**——
必须用**完整签名（含参数表）**或按签名在 blob 内的**精确偏移**匹配，
否则会把 `LoadFromStreamInternal` 当成 `LoadFromMemory_Internal`（本项目踩过）。

**「判断校验是否生效，看调用点的 CRC 实参，而不是猜」**——
`xor edx, edx` 这种清零寄存器后立刻调用的模式，就是"传 0 关闭校验"的铁证。

### 17.6 上游函数反汇编结果（下一步坐标）

**`0x204EAC0`（161 B）不是解密器**：它把 `rdx` 指向的 **24 字节结构体**搬到栈上，
再 `call 0x18204FC00`。属于轻量转发，可排除。

**`0x2050990`（794 B）是 Addressables 层的加载编排**：

```asm
0x20509AE  mov  r14, rcx                 ; r14 = this
0x2050A0A  mov  [r14+0x60], 0
0x2050A13  movups xmm0, [r14+0x28]       ; 拷 16 字节（Hash128 / CachedAssetBundle）
0x2050A1C  movsd  xmm1, [r14+0x38]       ; 拷 8 字节（CRC 或 hash 高位）
0x2050A7F  call 0x182056510              ; 解析前置对象
0x2050AA6  lea  r9, [r14+0x70]
0x2050AB5  call 0x1820505B0              ; ★ 核心：返回状态码
0x2050ABD  cmp  eax, 1                   ; 1 = 错误分支
0x2050AC6  cmp  eax, 2                   ; 2 = 另一错误分支
```

⇒ **`0x20505B0` 是「取包 → 解密 → 建流」的执行层，下一个反汇编目标。**
（函数大小未测，需先用 `.pdata` 取边界。）

### 17.7 下一步（明确）


1. 取 `0x20505B0` 的 `.pdata` 边界并反汇编 → 找其内部对缓冲的写操作
2. 若在其中找到「分配 + 逐字节/逐块写入」的循环 → 即解密例程
3. 对该写操作下**硬件写断点**（`MemoryAccessMonitor` 或调试寄存器），
   在实际执行时捕获精确的解密函数入口
4. 反汇编该函数 → 提取算法（头已确认是「符号扩展 int32 的 8 字节周期 XOR」，
   正文疑似另一把 key 或另一套变换）

### 17.8 已排除的假设（勿重复）

| 假设 | 检验 | 结果 |
|---|---|---|
| `0x204EAC0` 是解密器 | 反汇编（161 B） | ❌ 只做 24 字节结构体转发 + `call 0x18204FC00` |
| `0x20505B0` 是解密器 | 反汇编（982 B / 230 条） | ❌ 无逐字节 XOR 循环；`xor` 全是清寄存器，`shr r8,0xc`/`shr rax,6` 是哈希/对齐；内存写全是栈与对象字段 |
| 解密是紧凑的逐字节 XOR 循环（`buf[i]^=key[i%k]`） | 扫 il2cpp 段 **13,926,592** 条指令，找 `xor` 且源为索引内存字节 | ❌ 仅 46 个函数命中，全是零散单条，无循环形态 |
| 解密是向量化 XOR（pd/pxor） | 扫同段找 `xorps/pxor/vpxor` + 内存操作数 | ❌ 110 个函数命中，但**全部是与 .rdata 常量异或**，即浮点符号翻转（编译器取负惯用法），非数据解密 |
| Stream 对象内可直接找到以 `UnityFS` 开头的托管 `byte[]` | hook `LoadFromStreamInternal`，扫 Stream 对象前 0x120 字节字段，按 Il2CppArray(+0x18 len / +0x20 data) 校验 | ❌ 1060+ 次调用全为 `noBuffer`——缓冲不是普通托管数组（疑似原生内存或自定义流） |

### 17.9 结论：解密大概率不在 il2cpp 段的常规代码里

三条静态扫描（逐字节 XOR / 向量 XOR / 掩码常量）全部落空，
且 `0x20505B0` 的调用目标指向 Addressables 内部编排。
⇒ 解密更可能位于：

- **NEP2.dll**（保护层；已有证据显示它 detour 了 Unity 的文件读取路径）
- 或 `il2cpp` 段中**用非 XOR 变换**的方式（如 AES/bcrypt，则需找 `bcrypt.dll` 调用点）
- 或经 `LoadFromStreamInternal` 之前的**另一层解压/包装**（如 LZ4 + 自定义容器）

**下一步优先级**：
1. 在 GameAssembly 里搜对 **`bcrypt.dll` / `CryptDecrypt` / `BCryptDecrypt`** 的导入引用
   —— 若命中，说明正文是标准密码学算法而非自定义 XOR
2. 反查 NEP2.dll 导出/内部函数，确认它是否参与资源解密
3. 若为自定义变换：对「密文 ↔ 明文」样本做熵/周期/位分布分析，定位变换类型

---

## 18. 第十一轮：✅ 定位到解密实现的宿主 —— **NEP2 内联 AES**

### 18.1 决定性对比：GameAssembly 完全不含密码学

| 特征 | `GameAssembly.dll` (94 MB) | `NEP2.dll` (39 MB) |
|---|---|---|
| `bcrypt.dll` / `BCryptDecrypt` | **0** | 3 / 1 |
| `CryptoAPI`（`CryptDecrypt` 等） | **0** | 6 |
| `AES`（字符串） | **0** | **276** |
| `LZ4` / `zstd` / `lzma` / `brotli` | **0** | 0 |

⇒ **游戏代码完全不做资源解密，全部在 NEP2。**

### 18.2 运行时实测：NEP2 的 bcrypt 只用于哈希/签名，**不做对称解密**

hook `bcrypt.dll` 五个入口，实测启动全过程：

```
BCryptOpenAlgorithmProvider  →  SHA256, SHA1, RSA, RSA, SHA1, SHA384
BCryptGenerateSymmetricKey   →  0 次     ← 没有生成任何对称密钥
BCryptDecrypt                →  0 次     ← 零次对称解密
```

⇒ bcrypt 只用于**哈希与 RSA 签名验证**（客户端鉴权），**与资源解密无关**。
NEP2 里那 276 处 `AES` 不是通过 bcrypt 用的。

### 18.3 ✅ NEP2 内联实现了 AES（T-table 版）

按密码学常量做二进制搜索：

| 特征常量 | NEP2 | GameAssembly | UnityPlayer |
|---|---|---|---|
| **AES T-table 首项 `c66363a5`** | **命中 1 次** | — | 2 次 |
| **AES 逆 S-box `52096ad53036a538`** | **命中 4 次** | — | 4 次 |
| **AES Rcon `0102040810204080`** | **命中 1 次** | — | — |
| ChaCha sigma | — | — | — |
| XTEA/TEA delta `b979379e` | — | 2 次（偶然） | 3 次（偶然） |

`c66363a5` 是标准 `Te0[0]`，配合逆 S-box（解密用）与 Rcon
⇒ **NEP2 自带一套 T-table 版 AES 实现**，不走系统 bcrypt。

**这解释了此前所有 XOR 假设在正文上失败的原因：正文是 AES，不是 XOR。**

### 18.4 已定位的 AES 相关代码坐标（NEP2.dll，基址 0x180000000）

| 对象 | RVA | 说明 |
|---|---|---|
| Te0 (c66363a5) | 0x18C3540 | T-table 起点 |
| InvSbox #1 | 0x7A6EA0 | 逆 S-box |
| InvSbox #2 | 0x846A70 | 逆 S-box |
| InvSbox #3 | 0x1907640 | 逆 S-box |
| InvSbox #4 | 0x221C3F8 | 逆 S-box |
| **AES 分组处理候选函数** | **0x2306A0** | 见下 |

**`NEP2+0x2306A0` 的 AES 特征**：

```asm
0x2306E2  call 0x180233650
0x2306EF  call 0x1800210b0
0x2306FF  mov  ecx, 0x10        ; 16 字节 = AES 分组长度
0x230704  rep  movsb            ; 拷贝一个分组
0x230719  call 0x180020d40      ; 分组处理
0x230729  mov  ecx, 0x10
```

**其上方（`0x230623` 起）连续装载 64 位密钥材料**：

```asm
0x230636  movabs rcx, 0x2fcbf11b064a9ac5
0x23064E  movabs rcx, 0x9fa7ea3504e67e9e
0x230666  movabs rcx, 0x530e252192051bab
0x23067E  movabs rcx, 0xe7538699b0ede749
```

四个 64 位常量 = 32 字节，正好可能是 **AES-256 密钥**或密钥调度中间量。

### 18.5 下一步（明确）

1. 精确定位 `0x2306A0` 的 `.pdata` 边界（当前边界计算有 bug，长度算出天文数字）
2. 运行时 hook `NEP2+0x2306A0`，确认它是否在 1797 次 bundle 加载期间被调用
3. 若命中：捕获入参（密钥调度/状态）与出参（明文分组）→ 离线复现 AES 参数
4. 同时验证 `0x2fcbf11b064a9ac5` 等 4 个常量是否为固定密钥（跨启动是否变化）

### 18.6 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「先做密码学特征对比，再决定攻哪个模块」**——用几条常量签名
（AES T-table / S-box / Rcon / ChaCha sigma / TEA delta / SHA IV）
在候选 DLL 里做二进制搜索，几秒就能判定"解密实现在哪个模块、用的什么算法"，
比先反汇编几十 MB 代码高效得多。

**判据**：
- `c66363a5`（Te0[0]）出现 ⇒ T-table 版 AES
- `52096ad53036a538`（逆 S-box）出现 ⇒ 含**解密**路径
- `0102040810204080`（Rcon）⇒ 有密钥扩展
- 同时具备三者 ⇒ 自带完整 AES 实现，且**不走系统 crypto API**
  （所以 hook `BCryptDecrypt` 会一无所获）

---

## 19. 第十二轮：NEP2 的导出是诱饵，`.pdata` 被破坏

### 19.1 ❌ `NEP_Unknown` 是空壳，无法 hook

导出表实测：

```
Export dir RVA=0x8E6C70
NumberOfFunctions=1   NumberOfNames=1
唯一导出函数 RVA=0x1BD9B0
```

其内容只有 3 字节：

```asm
0x1BD9B0  ret 0          ; c2 00 00
0x1BD9B3  int3 int3 ...  ; 填充到对齐
```

**⇒ 导出是诱饵。真正的逻辑在运行时由 NEP2 自己的初始化代码 patch 进去。**

Frida 尝试 `Interceptor.attach` 时报错：
`unable to intercept function ...; please file a bug`
—— 原因是目标只有 `ret 0` + int3 填充，没有可重定位的指令序列。
**这是"函数是空壳"的典型症状，值得作为判据记住。**

### 19.2 ❌ NEP2 的 `.pdata` 被故意破坏

```
.pdata VSize=0x8AF48（约 570 KB，理论应有 ~4.7 万条 RUNTIME_FUNCTION）
过滤明显异常后仅剩 108 条有效记录
其中一条为 0x1 .. 0x3DB6BF2（长度 6470 万字节，约 64 MB）
```

⇒ 无法用 `.pdata` 取 NEP2 的函数边界。
（对比：`GameAssembly` 19.1 万条、`UnityPlayer` 8.7 万条，均正常。）

### 19.3 AES 表位于 `.rsrc` 段（新线索）

| 节 | VA 范围 |
|---|---|
| `.text` | 0x1000 .. 0x76F4B3 |
| `.rdata` | 0x770000 .. 0x8E9A28 |
| `.data` | 0x8EA000 .. 0x92AFE0 |
| `.pdata` | 0x92B000 .. 0x9B5F48 |
| `.200`（可执行） | 0x9BD000 .. 0xC01E98 |
| **`.rsrc`** | **0xC0B000 .. 0x258B3B9（26.7 MB）** |

**AES T-table 在 RVA 0x18C3540 —— 落在 `.rsrc` 内**，不在 `.text`。
⇒ AES 实现很可能在**嵌入资源中的第二阶段模块**里（运行时解出并执行），
而不是 `NEP2` 自身的 `.text`。

这与"导出是空壳、`.pdata` 被毁"互相印证：**NEP2 的静态映像不是它的真实形态。**

### 19.4 下一步（明确，已排序）

1. **对比 NEP2 的磁盘 `.text` / `.rsrc` 与运行后内存映像**
   —— 差异区即"运行时解出/补丁进去"的真实代码，这是最直接的突破口
2. 若发现解出的模块：dump 其内存映像 → 在**内存态**重新搜索 AES 表与
   `mov ecx,0x10; rep movsb` 型分组函数 → 反汇编求算法
3. 或在 **`.200`（可执行、0x244E98 = 2.4 MB）** 段里直接找 AES 分组函数
   —— 该段权限 `0x68000020`（可执行），此前未被检查过
4. 运行时验证：对疑似 AES 分组函数下 hook，确认在 1797 次资源加载期间被调用

### 19.5 本轮方法论沉淀

已写入 `references/static-native-triage.md`：

**「保护 DLL 的静态映像可能不是它的真实形态」**——三种症状同时出现即应高度怀疑：

1. **导出是空壳**：唯一导出只有 `ret 0` / `jmp` 转发 / `int3` 填充；
   表现为 Frida `Interceptor.attach` 报 `unable to intercept function`
2. **`.pdata` 被破坏**：记录数远低于 `节大小/12`，或出现长度离谱的垃圾项
3. **关键数据在异常段**：如密码学常量表落在 `.rsrc` 而非 `.text`

**对策**：不要继续在磁盘映像上硬啃，改为
**「磁盘映像 ↔ 内存映像」对比**——运行时被解出/补丁的区段就是真实逻辑。
这是对付"运行时自解密/自补丁"型保护层的通用手法。

---

## 20. 第十三轮：✅ 确认标准 AES 的完整数据布局 + 找到密钥扩展代码

### 20.1 ❌ 磁盘 ↔ 内存对比：**NEP2 不做自解密/自补丁**

把 NEP2 的三个区段从运行中进程 dump 出来，与磁盘逐字节比对：

| 区段 | 大小 | 差异字节 | 结论 |
|---|---|---|---|
| `.text` | 7,791,795 | **0** | 内存 == 磁盘 |
| `.rsrc` | 26,739,641 | **0** | 内存 == 磁盘 |
| `.200` | 2,379,416 | 80 | 全为 `00 -> 10 7E F7 7F` 形式的**基址重定位**，非代码改写 |

⇒ **NEP2 不自解密、不自补丁。** `.text` 就是它的真实代码。
上一轮"静态映像不是真实形态"的假设**被否定**。

`ret 0` 的导出只是**诱饵**：真实函数是内部直接调用的，不经导出表。

### 20.2 ✅ 确认标准 AES 数据布局（逆 S-box + Rcon 紧邻）

| RVA | 内容 | 证据 |
|---|---|---|
| `0x7A6EA0` | **AES 逆 S-box**（256 字节） | 起始 16 字节 = `52 09 6a d5 30 36 a5 38 bf 40 a3 9e 81 f3 d7 fb`；md5 `ba4ae4913dab406a429ed7000a8c8ef3` |
| **`0x7A6FAC`** | **AES Rcon 轮常量表**（大端 uint32） | 内容 = `0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36` —— 正标准 Rcon 序列 `01 02 04 08 10 20 40 80 1B 36` 的中后段 |
| `0x18C3540` | AES T-table（`Te0[0] = c66363a5`） | 在 `.rsrc` 内 |

**逆 S-box 与 Rcon 紧邻排布**是标准 AES 实现的典型形态（加解密共用 + 密钥扩展用 Rcon）。

### 20.3 ✅ 找到引用 Rcon 的代码（= 密钥扩展）

扫 `.text` 里 `lea reg,[rip+disp]` 指向 Rcon/T-table 窗口的指令，共 **21 条**：

```
0x57ED00 -> Rcon    0x57EFE2   0x581A23   0x582190   0x582E4A   0x582EF0
0x58389A   0x583A46   0x583A84   0x583B35   0x583B79   0x583BE5
0x583BEF   0x583E30   0x587345   0x5874D8   0x58755C   0x587576
0x5881D5   0x58AAAE   0x5A51EC
```

回溯得到的候选函数入口（部分）：

| 引用点 | 函数入口 | 距入口 |
|---|---|---|
| 0x57ED00 | 0x57EC2D | +0xD3 |
| 0x581A23 | 0x581993 | +0x90 |
| 0x582190 | 0x582160（薄 thunk `jmp`） | +0x30 |
| 0x583A46 | 0x583A30 | +0x16 |
| 0x5881D5 | 0x5880F0 | +0xE5 |
| 0x58AAAE | 0x58AA70 | +0x3E |

**引用 Rcon 的代码必是 AES 密钥扩展**（Rcon 只用于 `keyExpansion`）。

### 20.4 下一步（明确，最接近成功）

1. **hook 上述 `lea` 指令地址**（它们是确定的指令边界，可安全 hook）
   —— 在 `0x57ED00` 等处读寄存器，拿到密钥调度输入 = **AES 原始密钥**
2. 同时读 Rcon 使用前后的循环计数，判断是 AES-128/192/256
3. 若密钥在启动时固定 → 直接得到硬编码密钥，可离线解密全部 bundle
4. 若每次不同 → 追其来源（可能在 `.rsrc` 的常量、或由机器信息派生）

**注意**：hook 函数中间某条指令是安全的（Frida 会重定位该条指令），
风险远低于猜测函数入口。这比 hook 函数头更可行，因为 NEP2 的 `.pdata` 已损坏。

---

## 21. 第十四轮：✅ 捕获 AES 轮密钥（key schedule）

### 21.1 挂接点全部成功

10 个候选点全部成功挂接（`Instruction.parse` 预校验通过），实测命中：

```
entry@Rcon#1 (0x57EC2D)  206 次
lea          (0x57ED00)  590 次
entry        (0x58AA70)  384 次
```

### 21.2 ✅ 确认 Rcon 引用点即密钥扩展循环

`0x57ED00` 处的指令为 `lea rsi, [rip + 0x2282A5]`，计算目标：

```
0x57ED00 + 7 + 0x2282A5 = 0x7A6FAC   ← 正是 Rcon 表
```

且该点每轮命中时 `rcx` 为递增循环计数（`0x1f → 0x0 → 0x33`），
`r9` 为轮密钥中间字（`0xa573c29f` / `0x62636363` / `0x4b1b4395`）
⇒ **这就是 AES keyExpansion 的循环体。**

### 21.3 ✅ 捕获到正在构建的轮密钥数组

`entry@Rcon#1` 第 3 次命中时，`r8` 指向的缓冲区前 48 字节：

```
ba 89 80 2f 7e 31 13 ee 06 a7 be eb 05 64 d9 82
67 39 e4 d6 0b 87 41 a7 0e 1d b8 05 c0 17 74 33
42 0c 59 40 aa 21 9b 8a b5 36 f0 a0 9a 7b b3 4c
```

**交叉验证**：同一轮 `lea` 命中处 `r8 = 0xc0177433`，
正是上述缓冲区第 2 个 16 字节块的末 4 字节
⇒ **该缓冲区确实是 keyExpansion 正在写入的轮密钥数组。**

同一次调用还看到 `r8` 指向 `00 01 02 03 ... 1e 1f` 的**恒等表**
（`rdx=0x100`、`r9=0x100`）——这是 **S-box 由恒等置换生成的初始化过程**，
说明该函数同时承担「表初始化 + 密钥扩展」。

### 21.4 为什么这一步已经足够

AES 的密钥扩展是**可逆的**：已知任意一个完整轮密钥（16 字节，
除最后一个轮密钥外），即可反推原始 AES 密钥。

⇒ **不必再找"密钥常量"，只需把该轮密钥数组完整抓下来（AES-256 为 240 字节），
离线反推即可得到原始密钥。**

### 21.5 下一步（收尾动作）

1. 在 `0x57EC2D` / `0x57ED00` 命中时，dump `r8` 指向缓冲区的**前 240 字节**
   （AES-256 完整调度；若为 176 字节则是 AES-128）
2. 离线实现 AES key schedule 逆推 → 得原始密钥
3. **验证**：用该密钥对磁盘上的 `LocalCache` 密文 bundle 做 AES 解密，
   检查是否得到 `UnityFS\0` 明文 → 成立则算法完全掌握
4. 若解出的是压缩数据，再按 UnityFS 的 `flags` 做 LZ4 解压

### 21.6 附带发现：NEP2 也做 S-box 运行时生成

`00 01 02 ... 1f` 这种恒等序列出现在 AES 初始化路径上，
说明 NEP2 的 AES 表是**运行时生成**而非纯静态表——
这也解释了为什么静态搜 S-box 只有"逆 S-box"命中 4 次（正向表可能是生成的）。

---

## 22. 第十五轮：⚠ 重大更正 —— **正文就是同一个 8 字节 XOR**

### 22.1 ❌ 用捕获的轮密钥解 bundle：全部失败

拿 24 个 240 字节轮密钥样本，各取前 16/32 字节当 AES-128/256 密钥，
对密文 bundle 试 `ECB` / `CBC(IV=0)` / `CBC(IV=密文前16)`：

```
得分最高 ~4.0（即可打印率基线），含 "UnityFS" 标记的组合：0 个
```

⇒ **NEP2 里那套 AES 不是用来解资源 bundle 的**（NEP2 同时做 RSA/SHA 客户端鉴权，
那套 AES 很可能用于网络协议加密）。**这条线到此为止，不要再投入。**

### 22.2 ✅ 更正：正文与头部用**同一个** 8 字节周期 XOR

此前 §11 判定"正文是另一套加密"，**该判定错误**。

用已知明文字符串反推真密钥流，三个锚点（文件 `000a54a5…bundle`，`key8 = b2 8a af fa ff ff ff ff`）：

| 锚点 | 偏移 % 8 | 真实密钥流 | 与 `key8` 周期重复的差异 |
|---|---|---|---|
| `"CAB-"` @205 | 5 | `ff ff ff b2` | **0 处** |
| `"archite"` @1891 | 3 | `fa ff ff ff ff b2 8a` | **0 处** |
| `"m_ForcedFallbackFormat"` @796 | 4 | 22 字节 | **1 处**（off 808：`B2`→`B0`，差 `0x02`） |

⇒ **正文确实是 `key8` 的 8 字节周期 XOR。**
此前 LZ4 解压失败是**解析位置错误**，不是加密方式错误。

### 22.3 为什么可读串稀疏（每 MB 仅十几条）

UnityFS 的正文是 **LZ4 压缩块**，只有 `blocksInfo` 与目录是明文的。
所以明文可读串只出现在头部/blocksInfo 区，正文区天然高熵。
这与"XOR 正确"完全一致。

### 22.4 ⚠ 仍未解决：密钥流在 blocksInfo 之后发散

用 `key8` 周期 XOR 解密后交给 UnityPy（已设
`UnityPy.config.FALLBACK_UNITY_VERSION='2019.4.29f1'` 绕过非标准版本号 `2019.4,29f1`）：

- ✅ 头部解析通过（`UnityFS\0`、版本、大小都正确）
- ❌ **`blocksInfo` 的 LZ4 解压失败**：`corrupt input or insufficient space`（错误码 58/74/90）

⇒ **密钥流在头部之后某处开始偏离 8 字节周期规律。**
off 808 的 `0x02` 异常即是这种偏离的一个实例。

### 22.5 下一步（明确，已排序）

1. **先确定头部的真实字段偏移**：把解密后的前 80 字节 dump 出来，
   人工核对 `"2019.4,29f1"` 实际落在哪个 offset
   （本轮假设 offset 12 是错的，导致 0/120 文件通过校验）
2. 用**确定正确的头部明文**（43~50 字节）反推密钥流，
   与 `key8` 周期重复逐字节对比，找出**第一个偏离位置**
3. 弄清偏离规律：可能是
   - 每 N 字节切换一次 4 字节 `X`（计数器式）
   - 或 pad 字节并非纯符号扩展，而是带位翻转
   - 或 blocksInfo 区用另一份派生的 key
4. 修正后重跑 UnityPy 解析，能解出对象即算法完全掌握

### 22.6 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「判断 XOR 是否正确，不要看整体熵，要找已知明文锚点」**
——整体熵几乎不变（XOR 是保熵变换，密文熵 7.27、XOR 后 7.32），
但有结构的数据是否解对，只能靠**已知明文字符串**验证。

**判据：解出的字符串"差一两个字符且偏差固定"⇒ XOR 基本正确，只是密钥流有小偏差。**
本例中四处 Unity 字段名各自只错一个字符、偏差全为 `0x02`，
这比任何统计指标都更早、更准地暴露了"接近成功"。

**反向教训**：一旦看到这种"接近正确"的现象，
应当立刻**用已知明文反推密钥流**并逐字节比对，
而不是急着去验证解压/解析——本项目因此在错误方向上多走了两轮。

---

## 23. 第十六轮：✅ 头部布局完全确定；⚠ blocksInfo 仍无法解压

### 23.1 ✅ UnityFS 头部真实布局（三份文件完全一致）

```
偏移   类型        内容（已解密）
+000   bytes[8]    "UnityFS\0"
+008   u32 BE      0x02000007            ← 版本
+012   cstring     "5.x.x"               ← unityVersion（非标准，但与 Header 结构一致）
+018   cstring     "2019.4,29f1"         ← unityRevision（逗号而非句点）
+030   i64 BE      文件大小                ← ★ 与磁盘实际大小逐字节相等，证明头部解密 100% 正确
+038   u32 BE      compressedBlocksInfoSize
+042   u32 BE      uncompressedBlocksInfoSize
+046   u32 BE      flags = 0x43
+050   ---         blocksInfo 起始
```

**验证**：`size` 字段解出 `1,009,077`，与磁盘文件字节数完全一致
⇒ 头部（0..49）解密无误，`key8` 在这段是正确的。

> 上一轮假设 `2019.4,29f1` 在 offset 12，实际在 **18**；这是导致 0/120 文件校验通过的原因。

### 23.2 ⚠ blocksInfo 区（50..）不是 LZ4，且任何偏移都解不开

实测（`000a54a5…bundle`，`cbis=710`、`ubis=363`、`flags=0x43`）：

| 检验 | 结果 |
|---|---|
| 按 `flags&0x3F = 3`（LZ4HC）解压 `dec[50:760]` | ❌ `corrupt input` |
| 暴力扫描起始偏移 44..96 × 多组尺寸候选（0 个成功组合） | ❌ |
| 解出字节形态 | `00 00 00 00 00 00 02 00 00 00 00 00 00 00 1e 00 01 00 30 16…` —— **不是 LZ4 流形态** |

**两个异常**：
1. `cbis=710 / ubis=363` 的比例是 **1.96 倍膨胀**，对 LZ4 完全不合理
   （LZ4 最坏情况只膨胀约 0.4%）
2. 另一份文件 `0013549f…` 出现 `cbis=13 / ubis=1123`，更不可能

⇒ **结论：`flags` 的压缩类型解释有误，或 `cbis`/`ubis` 字段含义不符常规，或密钥流在 offset 50 之后即发散。**

### 23.3 支持"密钥流在头部后发散"的旁证

- `key8` 在 offset 205、796、1891 处经锚点验证是**正确**的
- 但 50..205 之间的字节形态不符合任何已知 UnityFS 子结构
- ⇒ 存在"头部用 `key8`、其后的区域用不同的密钥流"的可能性

### 23.4 下一步（明确）

1. **判定密钥流的分界点**：对同一文件，用 `key8` 解出的 offset 50..205 区段，
   检查是否能按 `BlocksInfo`（v7：`hash[16]` + `blockCount` + blocks + `nodeCount` + nodes）解析
   —— 若能，说明只是压缩类型判断错；若不能，说明密钥流在此处已发散
2. 若确定分界在 50：**从 offset 50 起重新做"已知明文锚点"反推**
   （`blockCount`、`uncompressedSize`、`compressedSize` 都是小整数，可作为锚点）
3. 另一条更快的路：**直接 hook NEP2 里读文件的函数**，
   截获"密文进 → 明文出"的一对完整缓冲（前 4 KB 即可），
   XOR 求密钥流并观察其结构（是否分段、是否计数器式）

### 23.5 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「头部字段偏移必须用自校验字段反推，不能靠记忆」**
——UnityFS 的 `size` 字段等于文件字节数，是最可靠的校验锚点。
本轮用它一次性确认了 `size@30`，从而反推 `cbis@38 / ubis@42 / flags@46 / blocksInfo@50`。
若 `size` 对不上，说明头部密钥或偏移错了，**立即停止后续解析**。

**「LZ4 压缩比异常 ⇒ 先怀疑字段含义，不要怀疑数据」**
判断依据：LZ4 最坏膨胀约 0.4%。
出现 `压缩后/压缩前 > 1.1` 就说明"这两个数字不是 LZ4 的输入输出尺寸"，
应重新核对 `flags` 的压缩类型与字段顺序。

---

## 24. 第十七轮：✅ 更正——密钥流在 offset 50 之后**没有**发散

### 24.1 ✅ 决定性证据：两个不同文件解密后该区段完全相同

取两份 key 不同的 bundle，解密后比对：

```
文件 000a54a5…（key8 = b2 8a af fa ff ff ff ff）
文件 0013549f…（key8 = dc 99 64 01 00 00 00 00）

偏移 46 起解密结果：
  文件1: 00 43 | 00 00 00 00 00 00 02 00 00 00 00 00 00 00 1e 00 01 00 30 16
  文件2: 00 43 | 00 00 00 00 00 00 02 00 00 00 00 00 00 00 1e 00 01 00 30 62
                                                        ↑ 从此处才各自不同
```

**推理**：两文件 `key8` 不同 ⇒ 密文不同；而解密后 50..67 **逐字节相同**
⇒ **该区间的密钥流是正确的，解出的是真实结构化数据。**

⇒ **推翻 §23.3「密钥流在 offset 50 后发散」的假设。**
`key8` 的 8 字节周期 XOR 在全身都是对的。

### 24.2 问题定位在「结构解读」，不在解密

已尝试并排除：

| 尝试 | 结果 |
|---|---|
| 未压缩 BlocksInfo v7（`hash[16]` + blockCount + blocks + nodeCount + nodes），hdr 长度 0 与 16 | ❌ `blockCount` 解出 0 或 16,789,526，均不合理 |
| LZ4 解 `dec[50:50+710]` → 363 | ❌ corrupt input |
| 交换 `cbis`/`ubis` 后重试 | ❌ |
| 暴力：起始偏移 48..600 × 4 组尺寸 × 5 组目标长度 | ❌ 0 个成功组合 |

**压缩比异常仍然存在**：文件1 `710 / 363`（1.96×膨胀，LZ4 不可能）；
文件2 `13 / 1123`（若按 13→1123 则是极端重复数据，可能；但与文件1 方向相反）。
⇒ 两个文件的 `cbis`/`ubis` **不能用同一种解释同时成立**，说明字段偏移或含义仍有误。

### 24.3 那个"跨文件相同的 18 字节"是什么（待解）

明文（offset 50 起）：`00 00 00 00 00 00 02 00 00 00 00 00 00 00 1e 00 01 00 30`

作为 BE u32 序列：`0, 512, 0, 7680, 0x010030..`
作为 LE：`0, 0x00020000, 0, 0x001e0000, …`

**跨两个不同 bundle 完全相同**，说明它是**与内容无关的固定结构**，
符合"版本号 / 块大小常量 / 对齐填充 / 字典 ID"这类字段，而非随内容变化的块表。

⇒ 下一步应把这 18 字节当作**固定头部常量**，去 `Assembly-CSharp` 的
`AssetBundle` 读写代码或 UnityPlayer 的 `ArchiveStorage` 实现里对照命名。

### 24.4 下一步（明确，收敛到两条）

1. **最可靠**：hook NEP2 里"读文件→解密"的那一对，截获**前 4 KB** 的
   （磁盘密文, 内存明文），XOR 后直接观察密钥流结构
   —— 不再靠猜结构，一次性判定周期性与分界
2. 次选：把 `flags=0x43` 的 `cbis`/`ubis` 按"文件长度 - 50 - 目录长度"反推，
   用 `size` 字段做平衡校验（`size == 50 + cbis + Σ块压缩尺寸 + 目录长度`）

### 24.5 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「用『跨文件不变量』判断解密是否正确，比看熵和字符串都可靠」**
—— 取两份密钥不同的密文，解密后若某区段**逐字节相同**，
则该区段的密钥流必然正确（否则不可能同时命中同一结果）。
这是排除"密钥流发散"假设的**充分证据**，成本只有两次解密 + 一次比对。

**反向提醒**：当结构解析连续失败时，容易怀疑"解密错了"。
**先用跨文件不变量把解密正确性钉死**，再专心攻结构——
否则会在"换密钥流"的错误方向上浪费大量轮次（本项目 §23 就差点如此）。

---

## 25. 第十八轮：🎯 **解密算法破解完成（真值验证）**

### 25.1 取得真值的方法

hook `kernel32!ReadFile` 记录每次读缓冲指针；再 hook `AssetBundle.LoadFromStreamInternal`，
在该时刻扫描那些缓冲，找首 8 字节为 `"UnityFS\0"` 的 —— 即 NEP2 就地解密后的明文。
一次运行抓取 **12 份 4 KB 明文**，按文件大小与磁盘密文配对成功。

### 25.2 ✅ 完整算法（可离线复现）

设密文前 4 字节为 `head[0:4]`，则：

```
X    = head[0:4] XOR "Unit"          # 4 字节
PAD  = FF FF FF FF                   # X 的最高位为 1（本游戏所有样本均如此）
X2   = X，但首字节 ^= 0x02            # 等价于 X 按 LE uint32 异或 2

keystream = [X][PAD][X2][PAD]        # 16 字节周期
plaintext[i] = ciphertext[i] XOR keystream[i % 16],  i = 0..len-1
```

**注**：`PAD` 是 `X` 的**符号扩展**（X 的最末字节 `0x97` 最高位为 1 ⇒ 扩展为 `FF FF FF FF`）。

### 25.3 ✅ 真值验证（4096 字节零偏差）

取磁盘密文与其对应 4 KB 明文逐字节 XOR，得到真实密钥流：

```
不同 8 字节块只有 2 种：
  fe 47 3d 97 ff ff ff ff   × 256
  fc 47 3d 97 ff ff ff ff   × 256
块类型序列: ABABABABABABAB…（连续 48 块无一次偏离）
```

`fe473d97 ^ fc473d97 = 0x00000002`（首字节）⇒ 正是交替异或 `0x02` 的规律。

### 25.4 ✅ 解密后头部逐字段吻合

```
+000  "UnityFS\0"
+008  u32 BE 7                        ← 版本 7（此前误读为 0x02000007）
+012  "5.x.x\0"
+018  "2019.4.29f1\0"                 ← 句点（此前误读为逗号）
+030  i64 BE size                     ← 与磁盘文件大小**逐字节相等**
+038  u32 BE cbis
+042  u32 BE ubis
+046  u32 BE flags = 0x43
```

实测三例：`size=1009077`（=文件大小）、`cbis=198 / ubis=363`、
`cbis=1092 / ubis=2433`、`cbis=289 / ubis=573`
—— 压缩比均约 1.8~2.2 倍，**符合 LZ4 常规区间**（此前 1.96 倍"膨胀"的怪异现象消失）。

### 25.5 ❌ 此前两处结论被真值推翻

| 此前结论 | 真值 | 原因 |
|---|---|---|
| 版本 = `0x02000007` | 实际 `7` | 周期被当成 8 字节，第 4 个密钥字节错了 `0x02` |
| revision 用逗号 `2019.4,29f1` | 实际句点 `2019.4.29f1` | offset 23 落在奇数 8 字节块，应异或 `0x02` |
| `cbis/ubis` 比例异常（1.96 倍膨胀） | 198/363 正常 | 同上，密钥错导致字段读错 |

⇒ **所有异常都源于"把 16 字节周期误判为 8 字节周期"。**

### 25.6 ⚠ 仍未解决：blocksInfo 结构解读

解密正确后，`blocksInfo` 字节为：

```
00 00 00 00 00 00 00 00 00 00 00 00 00 00 1e 00 01 00 30 16 00 02 07 00
```

形态不像 LZ4 流，`lz4.block.decompress`（多种 `uncompressed_size`）均失败。
⇒ **纯结构问题**：该自定义打包器对 `blocksInfo` 的布局与标准 UnityFS 不同
（可能多出前置字段、或块表/节点表字段宽度不同）。

**下一步**：拿 4 KB 真值明文直接人工核对这几十字节的字段边界
（真值已落盘在 `C:\DevCache\gf2\dumps\truth\`，无需再启动游戏）。

### 25.7 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「拿到一对（密文, 明文）比任何统计推断都快」**
—— 本项目在周期/掩码上猜了 3 轮，而一次 `ReadFile` 指针 + `LoadFromStream` 时刻扫描
就同时确定了周期长度、交替规律和全部头部字段。

**具体手法（可复用）**：
1. hook `ReadFile`，只记录 `(缓冲区指针, 长度)`，不读内容
2. hook 目标消费点（如 `LoadFromStreamInternal`），在其 `onEnter` 里回扫那些指针
3. 用**已知魔数**（`UnityFS\0`）判定哪个缓冲已被就地解密
4. 取该缓冲前 N KB 落盘，与磁盘密文 XOR ⇒ 真实密钥流

**关键优势**：不需要知道解密函数在哪、不需要改任何数据，
且能一次性暴露"周期不是你以为的那个长度"这类根本性误判。

**判据**：若密钥流按 8 字节块统计出现**恰好 2 种块且交替**，说明真实周期是 16 字节。

---

## 26. 第十九轮：blocksInfo **不压缩**，且布局非标准

### 26.1 ✅ 关键观察：blocksInfo 区是明文，未压缩

用真值明文（`plain_10_32768.bin`，`cbis=118 / ubis=203 / flags=0x43`）看 `[50, 168)`：

```
+ 50  00 00 00 00 00 00 00 00 00 00 00 00 00 00 1e 00
+ 66  01 00 30 06 00 02 07 00 42 29 ea 00 03 0a 00 24
+ 82  0c 07 0a 00 24 2c 40 0a 00 25 15 d5 1e 00 fa 02
+ 98  e4 00 03 00 01 68 5c 00 00 14 64 00 03 00 00 00
+114  02 4f 00 f8 1b 30 ec 00 00 00 04 43 41 42 2d 30
+130  30 66 32 36 30 62 32 30 34 61 66 33 65 63 33 65
+146  66 63 38 30 66 62 30 65 66 32 39 32 31 65 31 31
+162  00 50 00 00 0b 37
```

其中 offset 125 起是**完整明文路径**：

```
"CAB-00f260b204af3ec3efc80fb0ef2921e1"
```

⇒ **该段未被压缩**（LZ4 不可能产出连续可读路径）。
`lz4.block.decompress` 在此必然失败，因为数据本就不是 LZ4 流。

### 26.2 由此修正两个字段的含义

| 字段 | 此前理解 | 实测 |
|---|---|---|
| `cbis`（+038） | 压缩后 blocksInfo 大小 | ✅ **等于实际存储长度**（118 = 168−50） |
| `ubis`（+042） | 解压后大小 | ❌ 非"解压后长度"（203 > 118，且无压缩） |
| `flags=0x43` 的压缩位 | 作用于 blocksInfo | ❌ **只作用于数据块**，不作用于 blocksInfo |

### 26.3 ❌ 仍未匹配标准 UnityFS 节点布局

以 `"CAB-…"` 起点（125）向前推 20 字节（`i64 off + i64 size + u32 flags`）得到的值全是天文数字
（`off=22419729481728`），说明**该自定义打包器的 blocks 表 / nodes 表字段宽度或顺序与标准不同**。

已试并排除：
- 16 字节 `hash` 前缀 + `blockCount` → `blockCount` = 16,789,526（不合理）
- 无 `hash`、`blockCount` 在 offset 50 → 0
- 大端 / 小端两种解释
- 节点字段按 `(i64,i64,u32)` 从 105/104 起算 → 全部不合理

### 26.4 下一步（成本已大幅降低）

真值明文已落盘（`C:\DevCache\gf2\dumps\truth\`，12 份 × 4 KB），**无需再启动游戏**。
下一轮只需：

1. 对同一文件同时取**两份不同真值**（或同一份里多条 `CAB-` 路径），
   用"路径之间固定间隔"反推节点记录长度
2. 用 `cbis`（118）与路径总长做平衡：`118 = 头部字段 + 节点数 × 记录长 + Σ路径长`
3. 一旦节点表解出，即可拿到每个资产块在文件中的 `offset/size` →
   **直接定位并解出数据块，完成端到端离线解密**

### 26.5 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「先判定某段数据到底有没有被压缩，再选解压算法」**
——最可靠的判据是**明文可读性**：若能在该段里找到连续可读的路径/字段名，
则该段**未压缩**。
（LZ4 的 literal run 虽也含原文，但不会被跨越多条记录的连续明文路径"贯穿"。）

**推论**：`flags` 里的压缩类型位**未必作用于所有区段**。
UnityFS 的 `flags` 只描述**数据块**的压缩方式；
`blocksInfo`/目录区的压缩与否要看该区段内容本身，不能照抄标准实现。

**判据速查**：
- 区段内含可读路径 ⇒ 未压缩，直接按结构解析
- 区段高熵且无可读串 ⇒ 才尝试 LZ4/LZMA
- 若 `compressedSize < uncompressedSize` 但内容可读 ⇒ **这两个字段不是压缩前后尺寸**，去查别的含义

---

## 27. 第二十轮：✅ 数据块**未压缩**，SerializedFile 直接可见

### 27.1 ✅ 解密后的 4 KB 里能直接读到 Unity 序列化字段名

真值明文（`plain_10_32768.bin`）中的可读串：

```
@125   CAB-00f260b204af3ec3efc80fb0ef2921e11
@204   2019.4.29f1              ← SerializedFile 头部里的 Unity 版本串
@719   m_ForcedFallbackFormat
@777   CompleteImageSize
@817   IsReadable
@852   PreProcessed
@865   m_StreamingMipmaps
@987   ColorSpace
@1622  ependencies              ← Dependencies
@3505  PhysicsShape
@3739  archive:/CAB-00f260b204af3ec3efc80fb0ef2921e1
```

**这些是 SerializedFile 的元数据字段名，全为明文。**
⇒ **数据块完全没有压缩**，`flags=0x43` 的压缩位只是"允许的压缩方式"，
每个 Block 自带 `u16 flags` 决定该块实际是否压缩——本批块均为未压缩。

### 27.2 ✅ 由此得到的实用结论：**不必解析 blocksInfo 也能取到资产**

既然块未压缩，SerializedFile 就紧接在 blocksInfo 之后：

```
SerializedFile 起始 ≈ 50 + cbis
```

`plain_10` 样本 `cbis=118` ⇒ 起始 168，而其版本串 `"2019.4.29f1"` 出现在 204
（168 + 36，与 SerializedFile 头部字段长度吻合）。

⇒ **端到端离线路径成立**：
1. 用 §25 的 16 字节密钥流解密整个文件
2. 从 `50 + cbis` 处切出 SerializedFile
3. 直接交给 UnityPy 的 `SerializedFile` 解析器（或按 CAB 格式自解析）

### 27.3 ❌ blocksInfo 字段布局与 UnityPy 对 v7 的预期不符

尝试把 header 的 `flags` 改成无压缩（`0x40`/`0x00`）并把 `cbis`/`ubis` 设为相等，
UnityPy 能通过头部，但解析 blocksInfo 时 `blockCount` 读出巨大值，
持续按 **10 字节/条**的块记录消耗直到缓冲耗尽（误差偏移序列 190→198→200 呈 +8/+2 交替，
正是 `u32+u32+u16` 的记录步长）。

⇒ **该打包器写了 `version=7`，但块表布局与标准 v7 不一致**
（可能缺 16 字节 `hash` 前缀，或块/节点表字段宽度不同）。
**这条不影响资产提取**，仅影响"用标准 UnityFS 解析器整体打开"。

### 27.4 目标达成度评估

| 目标项 | 状态 |
|---|---|
| 取得可离线复现的**解密算法** | ✅ **完成**（16 字节周期密钥流，4 KB 真值零偏差验证，§25） |
| 密钥派生（`X = 密文头4字节 XOR "Unit"`） | ✅ **完成**，无外部密钥、无机器绑定 |
| 数据块压缩方式判定 | ✅ 完成（未压缩，明文 SerializedFile 可直接读） |
| 标准 UnityFS 解析器直接打开 | ⚠ 不支持（块表布局非标准），但可用"切 50+cbis"绕过 |
| hook 资源加载替换的可行性前提 | ✅ 已确认（1797 个加密包 CRC=0 不校验，§17） |

### 27.5 下一步

1. 用 `50 + cbis` 切片 + 直接 `SerializedFile` 解析，验证能列出对象与资产名
2. 拿到资产名后，用 UnityPy 修改并**重新打包成同样布局**的 bundle
3. 在 `LoadFromStreamInternal` hook 里替换为自备 bundle，游戏内验证渲染

### 27.6 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「块是否压缩，看内容而不是看 flags」**
UnityFS 的 header `flags` 只给"允许的压缩种类"，
真正的压缩在**每个 Block 自带的 `u16 flags`** 里。
判据：解密后若能在数据区读到成片的字段名/路径，则该块**未压缩**。

**「解析器打不开，不等于数据坏了——可以绕过解析器」**
当 blocksInfo 布局非标准时，只要块未压缩，
`SerializedFile 起始 = 50 + cbis` 就够用了，直接切出来单独解析即可。
**不要为了"让标准解析器工作"去改数据**，那会破坏与原文件的兼容性。

**诊断技巧**：UnityPy 报错偏移随缓冲长度线性增长（如 190→198→200、步长 +8/+2 交替），
说明结构步长是 10 字节（`u32+u32+u16`），
即**它认得块记录、但块数量读错了** ⇒ 问题在块数之前的字段，不在块表本身。

---

## 28. 第二十一轮：数据块确为 LZ4 压缩；blocksInfo 布局仍待定

### 28.1 ✅ `50 + cbis` 之后是 LZ4 压缩的数据块（不是明文 SerializedFile）

三份真值样本在 `50 + cbis` 处的字节：

```
plain_10 (cbis=118, 起点 168): 70 07 00 0f 39 00 11 60 2e 72 65 73 53 00 f0 1b ...
plain_11 (cbis= 88, 起点 138): 70 07 00 0f 39 00 11 60 2e 72 65 73 53 00 f0 01 ...
plain_12 (cbis=143, 起点 193): 70 07 00 0f 39 00 11 60 2e 72 65 73 53 00 f0 01 ...
```

按 LZ4 block 格式解 `70 07 00 0f 39 00 11 60 2e 72 …`：

- token `0x70` ⇒ 字面量长度 **7**、匹配长度 4
- 紧接 7 个**字面量**：`07 00 0f 39 00 11 60`
- 再 2 字节匹配偏移：`2e 72` = 11890（合法）

⇒ **这是结构合法的 LZ4 流**，即数据块**是压缩的**。

（其中 `2e 72 65 73 53` 恰好是 ASCII `.resS`，但那只是字面量里的巧合，
不要误判成"目录路径"——上一轮 §27 正是这样误判的。）

### 28.2 ❌ 更正 §27.2「SerializedFile 起始 = 50 + cbis」

把该处当 SerializedFile 头解析，结果全是垃圾：
`metadata_size=1879506959`、`file_size=4107301964955673971`、`unity_version=''`。
**该结论错误，已废弃。**

### 28.3 blocksInfo 的真实边界：`cbis` 可信，问题在内部布局

`plain_10`：CAB 路径位于 125..161、以 `00` 结束（162）。
`50 + cbis = 168`，即 blocksInfo 结束处正好在路径之后 6 字节 —— **`cbis` 作为"该区段长度"是对的**。

那 118 字节内既有 block 表又有 node 表，且 node 路径是明文。
但按标准 v7 布局（`hash[16]` + `blockCount` + blocks + `nodeCount` + nodes）解不通：

| 起点假设 | `blockCount` 读出 | 判定 |
|---|---|---|
| 50（无 hash） | 0 | ❌ |
| 66（hash[16] 后） | 16,789,526 | ❌ |

内容开头 16 字节：`00×14 1e 00`，其后 `01 00 30 06 00 02 07 00 42 29 ea 00 03 0a 00 24 …`

### 28.4 下一步（最省力路径）

**放弃"让标准解析器打开整包"，改为只解数据块：**

1. `cbis` 已可信 ⇒ 数据块区 = `[50+cbis, 文件尾)`，且首块是标准 LZ4 流
2. 直接对该区按 LZ4 逐块解压（块边界可从解压过程自然推出：
   每解完一块，剩余偏移即下一块起点）
3. 解出的块内容拼起来即 SerializedFile 数据，
   交给 UnityPy 的 `SerializedFile` 解析器（不经 BundleFile 层）

这样完全绕开 blocksInfo 布局问题——**目标只需"拿到资产"，不需要"兼容标准包格式"**。

**替换时的对称做法**：把自备 bundle 的数据块重新 LZ4 压缩，
写出与原始 `cbis`/块表一致的头（或直接用原包的 blocksInfo，只换数据块内容）。

---

## 29. 第二十二轮：blocksInfo 既非标准布局、也非 LZ4

### 29.1 ❌ 穷举拟合标准布局：只有退化组合

对 `plain_10`（`cbis=118`）的 blocksInfo 118 字节，
遍历 `hash 前缀长度 ∈ {0,4,8,16,20}` × 大端/小端 × `blockCount ∈ 0..8`，
要求「解析恰好铺满 118 字节且末条节点路径为 `CAB-` 开头」：

```
可解析组合 4 个，全部是 blockCount=0 / nodeCount=0 的退化情形
```

⇒ 标准 UnityFS BlocksInfo 布局**完全套不上**。

### 29.2 ❌ blocksInfo 也不是 LZ4

在 `[46, 900)` 范围内对每个起点 × 长度 × 4 组目标长度做 LZ4 解压，
筛「输出含 `CAB-`」：

```
命中 7 组，但输出全是「35+ 个 00 开头」的退化流
（例如 start=52 len=109 → 156 字节，前 48 字节为 35 个 00 + 06 00 02 00 …）
```

⇒ **blocksInfo 既不是标准明文结构，也不是 LZ4 流。**

### 29.3 当前确定的边界事实

| 事实 | 证据 |
|---|---|
| 解密算法正确 | 4 KB 真值零偏差；头部 `size` 与文件字节数相等 |
| `cbis` 是该区段长度 | `50 + cbis = 168`，恰好在 CAB 路径结束后 6 字节 |
| blocksInfo 区内含明文 `CAB-…` 路径 | offset 125..161，36 个十六进制字符完整可读 |
| `50 + cbis` 之后是合法 LZ4 流 | `70 07 00 0f 39 00 11 60 2e 72` 按 token/literal/offset 解析成立 |
| blocksInfo 区自身布局未解 | 标准布局穷举失败、LZ4 解压失败 |

⇒ **结论：解密（外层）已完成；bundle 内层的「块表/节点表」用了非标准编码，
且该编码既不是明文结构、也不是 LZ4。**

### 29.4 下一步（换思路，不再猜格式）

1. **直接读 Unity 的解析结果**：hook `UnityPlayer!AssetBundle.LoadFromStreamInternal`
   返回后的对象，读其已解析的 block 列表（Unity 内部结构），
   拿到真实的 `offset/size/uncompressedSize` —— 比猜格式可靠
2. 或 hook `UnityPlayer` 里负责 `LZ4` 解块的函数，截获每块的
   （输入偏移, 输入长度, 输出长度），连块表都不用解
3. 拿到块边界后，数据块本身就是标准 LZ4，可直接离线解出 SerializedFile

### 29.5 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「猜格式的正确止损点」**——同一处结构连续 5 轮猜不中时，
说明它不是"某种已知布局的变体"，而是**有自定义编码**。
此时应停止穷举，改为**从运行时读取已解析的结果**（hook 消费方，拿它解析后的数据）。

判据：
- 标准布局穷举只得到退化组合 ⇒ 布局非标准
- 按压缩格式解出退化输出（大量 00 开头）⇒ 不是该压缩格式
- 两条同时成立 ⇒ 该区段**不是"已知格式 + 变体"**，必须换信息源

**成本对比**：本目标在"猜 blocksInfo 布局"上花了 5 轮；
而"hook Unity 拿它解析好的对象"只需 1 轮，且结果确定。

---

## 30. 第二十三轮：反汇编 UnityPlayer 解析链（进行中）

### 30.1 已知调用链（全部为薄封装，尚未到达解析核心）

```
UnityPlayer+0x3B7650  AssetBundle::LoadFromStreamInternal   (210 B)
    └─ 0x1802D8BC0                                          (209 B)
         └─ 0x180593DC0                                     ← 待查
    └─ 其他：0x18008FCC0, 0x18034F640, 0x1802D0440,
              0x1802CDA20, 0x1800A52A0, 0x180793C00
```

`0x3B7650` 的实际动作只有：把 `rcx/rdx/r8` 传下去，调用 `0x1802D8BC0`，
取返回对象首字段作为结果返回。

### 30.2 ✅ 找到归档签名表，但**没有任何指令直接引用**

```
RVA 0x1504538:  "UnityFS\0UnityWeb\0....UnityRaw"
```

- `lea` 引用：**0 条**
- 通用 RIP 相对引用（`disp` 指向该地址 ±0x40，扫描全部可执行节）：**0 条**

⇒ 该字符串**不是通过直接地址比较使用的**，可能：
- 是死数据 / 仅用于调试输出
- 或经 `.data` 里的指针间接访问（需要先找指针表）

### 30.3 ❌ 再次排除 blocksInfo 是 LZ4

把 `ubis=203` 当输出长度，在 `start ∈ [50,90)` × 各种输入长度上重扫：

```
命中 17 组，但输出长度全部到不了 203（只到 100/156/173 就停）
⇒ 这些都是"输入耗尽即截断"的不完整流，不是有效 LZ4 块
```

**注**：`cbis=118 → ubis=203` 的比值（1.72×）乍看符合 LZ4，
但实际不存在能产出 203 字节的合法流。

### 30.4 下一步（明确）

1. 继续跟 `0x180593DC0`（进入归档读取核心），在其内部找
   `blockCount` 的读取点与比较常量——**反汇编能直接给出字段宽度与顺序**
2. 或用 `.data` 段搜指向 `0x1504538` 的指针（8 字节等于 `base+0x1504538`），
   再找读该指针的指令
3. 或按 §29.4 方案 2：hook Unity 的 LZ4 解块函数，直接截获每块的
   （输入偏移, 输入长度, 输出长度），完全绕开块表

### 30.5 本轮方法论沉淀

已写入 `references/static-native-triage.md`：

**「调用链追踪要识别"薄封装"，别把封装当核心」**
本例 `0x3B7650`(210 B) → `0x2D8BC0`(209 B) 都只是参数搬运 + 转发，
真正的解析在第三层。判据：函数体 < 256 字节且主要内容是
`mov`/`lea`/`call` ⇒ 是封装，继续跟 `call` 目标。

**「字符串找不到引用，不等于它没用」**
签名表（`"UnityFS"`）的 `lea` 引用为 0、RIP 相对引用也为 0，
说明它经**指针表间接访问**。
排查顺序：① `lea` 直接引用 ② 通用 `disp` 引用 ③
在 `.data`/`.rdata` 搜"等于该地址的 8 字节值"（即指针），再找读该指针的指令。

---

## 31. 第二十四轮：✅ 明文 bundle 全部可解析，格式确认为标准 UnityFS

### 31.1 ✅ `StreamingAssets/aa` 的 4 个明文 bundle 全部解析成功

| 文件 | 大小 | 解析结果 |
|---|---|---|
| `97d94d14…bundle` | 23,160 | ✅ 13 对象（Sprite 5 / Texture2D 5 / Shader 1 / Material 1 / AssetBundle 1） |
| `aotres_assets_all_…` | 3,076 | ✅ 22 对象，容器 `Assets/Scripts/Reporter/Reporter.prefab` |
| `aotres_scenes_all_…` | 14,503,748 | ✅ **1643 对象**，容器 `Assets/Scene/Login.unity`、`Assets/Scene/Start.unity` |
| `e15b8935…bundle` | 4,842 | ✅ 64 对象（MonoScript 63 / AssetBundle 1） |

⇒ **Unity 接受的格式就是标准 UnityFS**，UnityPy 能直接打开。

### 31.2 两者头部差异：revision 字符串长度不同

```
明文 bundle:  "UnityFS\0" 00 00 00 07 "5.x.x\0" "0.0.0\0"        ← revision 5 字符
加密 bundle:  "UnityFS\0" 00 00 00 07 "5.x.x\0" "2019.4.29f1\0"  ← revision 11 字符
```

头部是**变长 cstring**，所以加密版的后续字段整体后移 6 字节。
加密版的 `size/cbis/ubis/flags` 分别在 30/38/42/46，这个解析是**正确的**。

### 31.3 解密确证无误（用有真值的那份验）

`a9523ed95bec1d43dfbc4cc98f8ef419.bundle`（1,163,193 字节）：

```
key      = fe473d97 ffffffff fc473d97 ffffffff
解密头   = "UnityFS\0" 00 00 00 07 "5.x.x\0" "2019.4.29f1\0" …
size     = 1163193   ← 与磁盘文件字节数完全相等
cbis=301  ubis=621   ← 2.06 倍，LZ4 正常区间
flags    = 0x43
```

⇒ **外层解密 100% 正确**。

### 31.4 ⚠ 唯一未解点：加密包的 blocksInfo 区无法按 LZ4 解开

- UnityPy 对上述文件报 `Decompression failed ... Error code: 441`
- 手工对 `dec[50:351]` 用 `ub=621` 解 LZ4 同样失败
- blocksInfo 起始 16 字节恒为 `00×14 1e 00`，其后 `01 00 30 XX 00 02 07 00 …`
  （`XX` 随文件变化，与文件大小相关）

**注意矛盾点**：既然格式是标准 UnityFS、且外层解密已验证正确，
则 blocksInfo 必然能被 Unity 解开。
⇒ 说明**解密在 offset 50 之后仍有偏差**，或该区段另有**第二层变换**
（外层 XOR 已解，内层还差一步）。

### 31.5 下一步（收敛到一个最有把握的动作）

**在 `.text` 里定位 LZ4 解压函数并 hook 它**：
1. Unity 静态链接 LZ4，`LZ4_decompress_safe` 有易识别的入口特征
   （大量 `cmp`/`test` 边界检查 + 后向拷贝循环）
2. hook 它，记录每次调用的 `(src, srcSize, dst, dstCapacity)` 与返回长度
3. 在资源加载期间，第一个非零返回值且 `srcSize≈cbis` 的调用，
   其 `src` 就是 blocksInfo 的真实位置 ⇒ **直接得到 blocksInfo 的起点与长度**
4. 对比我们解出的 `dec[50:50+cbis]` 与它，差异即第二层变换

这比继续猜布局可靠得多，且一次运行即可定论。

### 31.6 本轮方法论沉淀

已写入 `references/asset-format-and-obfuscation.md`：

**「用同游戏的明文样本做格式基准」**
——本目标在 blocksInfo 布局上猜了 6 轮。真正的突破是发现
`StreamingAssets/aa` 里有 **4 个明文 bundle**，它们用同一套打包器产出、
能被标准解析器直接打开。
⇒ **同游戏的明文样本是最可靠的格式基准**：先证明"标准格式能开"，
才能断定问题出在解密而非格式。

**判据**：若同游戏的明文样本可被标准解析器打开，而加密样本解密后不能，
则**问题必然在解密的后半段或存在第二层变换**，不要再怀疑格式本身。

**附带经验**：UnityFS 头部是**变长 cstring**，`revision` 字段长度会改变
`size/cbis/ubis/flags` 的偏移。解析前必须先按 cstring 走到字段位置，
不能硬编码偏移（本例明文 `"0.0.0"` 与加密 `"2019.4.29f1"` 差 6 字节）。

---

## 32. 阶段总结（第 15 轮）：已确定 / 未决

### 32.1 ✅ 已确定（有真值或自校验证据）

| # | 结论 | 证据强度 |
|---|---|---|
| 1 | **解密算法**：16 字节周期 XOR，`ks = [X][FF×4][X^2][FF×4]`，`X = 密文头4字节 XOR "Unit"` | ★★★ 4 KB 真值零偏差 + 头部 `size` 与文件字节数相等 |
| 2 | 解密无外部密钥、无机器绑定（纯从密文头自派生） | ★★★ 多文件独立验证 |
| 3 | 头部字段布局（变长 cstring 后接 `size/cbis/ubis/flags`） | ★★★ `size` 自校验 |
| 4 | 1797 个加密 bundle 加载时 **CRC=0 不校验** | ★★★ 调用点反汇编 `xor edx,edx` |
| 5 | 仅 4 个 StreamingAssets bundle 带真实 CRC | ★★★ 运行时抓取 |
| 6 | 游戏接受**标准 UnityFS**（明文样本可被 UnityPy 打开） | ★★★ 4/4 文件解析成功，含 1643 对象的场景包 |
| 7 | GameAssembly **完全不做资源解密**；解密在 NEP2 | ★★★ 密码学常量对比 |
| 8 | NEP2 的 bcrypt 只用于 SHA/RSA，**不做对称解密** | ★★★ 运行时 0 次 `BCryptDecrypt` |
| 9 | NEP2 **不自解密/不自补丁**（内存==磁盘） | ★★★ 30 MB 逐字节比对，差异 0 |
| 10 | NEP2 的 `NEP_Unknown` 导出是空壳（`ret 0`） | ★★★ 反汇编 |
| 11 | NEP2 的 `.pdata` 被故意破坏 | ★★★ 仅 108 条有效记录 |
| 12 | NEP2 内含自实现 AES（T-table + 逆 S-box + Rcon） | ★★★ 常量命中；但**与资源无关** |

### 32.2 ❌ 已排除（勿重复）

| 假设 | 排除方式 |
|---|---|
| 正文是另一套加密 | 16 字节周期密钥流在 4 KB 内完美验证 |
| 解密在 il2cpp 的 XOR 循环里 | 扫 1392 万条指令，无紧凑 XOR 循环 |
| 解密是向量化 XOR | 110 个候选全是浮点符号翻转 |
| NEP2 的 AES 用于资源解密 | 24 组轮密钥 × 3 模式 × 2 长度，`UnityFS` 命中 0 |
| `BakeMesh` 做 CPU 预蒙皮 | icall 槽全程为空（从未调用） |
| 密钥流周期 8 字节 | 真值显示周期 16 字节（`fe…`/`fc…` 交替） |
| blocksInfo 是 LZ4 | 全起点/全长度/全目标长度穷举，只有退化流 |
| blocksInfo 是其它通用压缩 | zlib/deflate/gzip/bz2/lzma/lzma_alone/lzma_raw 全失败 |
| blocksInfo 是标准明文 BlocksInfo | 标准布局穷举只有退化组合 |
| `SerializedFile 起始 = 50 + cbis` | 按该处解析出天文数字 |

### 32.3 ⚠ 未决问题（按优先级）

1. **blocksInfo 区的真实结构**（唯一卡点）
   - 已知：起点 50、长度 `cbis`、区内含明文 `CAB-…` 路径、紧邻前 16 字节恒为 `00×14 1e 00`
   - 已知：该区数据形态含大量小端 2 字节偏移（`0a 00`/`1e 00`/`14 00`），**像 LZ4**
   - 矛盾：既然是标准格式且外层解密正确，Unity 必然能解 —— 但所有标准解法都失败
   - **下一步唯一有把握的动作**：hook UnityPlayer 里的 LZ4 解压函数，
     抓 `(src, srcSize, dst, dstCapacity, return)`，直接得到 blocksInfo 真实起点与长度

2. **hook 替换资源**（目标 B，尚未演示）
   - 前提已齐：地址可 hook、CRC 不校验、可用明文样本做模板
   - 待做：用 UnityPy 造 bundle → 在 `LoadFromStreamInternal` 替换 Stream → 游戏内验证

3. **NEP2 如何把解密数据交给 Unity**
   - 已知：走 `LoadFromStreamInternal(Stream, crc=0, 0x8000)`
   - 未知：Stream 的实现类与缓冲区位置（托管 `byte[]` 扫描未命中）

### 32.4 交付物清单

| 文件 | 内容 |
|---|---|
| `C:\DevCache\gf2\decrypt_gf2.py` | **待创建**：按 §32.1-1 实现完整解密 |
| `C:\DevCache\gf2\dumps\truth\` | 12 份 4 KB 真值明文（验证用） |
| `C:\DevCache\gf2\dumps\plain\` | 解密尝试产物 |
| `docs/gf2-exilium-recon.md` | 本台账（§1–32） |
| `.agents\skills\unity-il2cpp-game-recon\` | 主流程 + 4 份 references |

---

## 33. 第二十五轮：✅ 完整解析出 AssetBundle 错误码表 + 定位解压候选函数

### 33.1 ✅ 错误码跳转表完整破译

反汇编 `0x2E5FE8` 得到跳转表结构：

```asm
0x2E6041  cmp  ebp, 0xb                 ; code 0..11
0x2E604A  lea  rdx, [rip - 0x2E6051]    ; ⇒ rdx = 0
0x2E6051  mov  ecx, [rdx + rbp*4 + 0x2E6170]
0x2E6058  add  rcx, rdx
0x2E605B  jmp  rcx                      ; 表项为绝对 RVA
```

⇒ **表基址 `0x2E6170`**，`entry = [0x2E6170 + code*4]`。

完整映射（12 项）：

| code | 消息 |
|---|---|
| 0 | Cancelled loading AssetBundle '%s'. |
| 1 | Cancelled loading AssetBundle '%s'. |
| 2 | AssetBundle '%s' has invalid crc! |
| 3 | Failed to initialize cache for the AssetBundle '%s'. |
| 4 | The '%s' file is not a valid AssetBundle. |
| 5 | The AssetBundle '%s' can't be loaded because it doesn't have serialized data. |
| 6 | ...not built with the right version or build target. |
| 7 | ...another AssetBundle with the same files is already loaded. |
| 8 | Failed to read data for the AssetBundle '%s'. |
| **9** | **Failed to decompress data for the AssetBundle '%s'.** |
| 10 | ...can't be loaded because of internal error. |
| 11 | Failed to delete the target of a Recompression operation... |

### 33.2 ✅ 由 code 9 反推到解压函数

`0x2DA4A1`（块迭代函数，505 B）内：

```asm
0x2DA64F  call 0x18083C700        ; ← 解压候选
0x2DA654  cmp  eax, -1
0x2DA657  jle  0x1802DA77D        ; ← 该函数正是"设 [rdi+0xec]=9"（解压失败）
```

⇒ **`UnityPlayer+0x83C700` 签名推断为 `(src, srcLen, dst, dstCap) -> int`，
返回 -1 表示失败。** 这是最可靠的解压函数候选。

### 33.3 ⚠ 但运行时 0 次调用

hook `UnityPlayer+0x83C700` 成功（首指令 `mov [rsp+8], rbx`，正常序言），
完整启动 70 秒内 **`calls = 0`**。

两种可能：
1. 该函数不是 blocksInfo 的解压器（blocksInfo 走别处）
2. **这些 bundle 的块根本不走解压路径**（块是未压缩的）

**后者与此前观察一致**：解密后的数据区能直接读到成片 Unity 字段名
（`m_ForcedFallbackFormat` / `CompleteImageSize` / `m_StreamingMipmaps` 等），
说明块内容确实是明文。

### 33.4 下一步

1. hook 时**主动触发资源加载**（切换界面/进战斗），而不是只等启动流程
2. 同时 hook `0x180834810`（`test al,al` 的布尔函数，块循环里反复调用）
   与 `0x180837030`、`0x18083B040` —— 这些都在块迭代函数内，可能才是真正的
   块读取/解压入口
3. 若确认块未压缩，则 blocksInfo 那 118~301 字节也是明文，
   **只需按"块表/节点表"结构硬解**（宽度/顺序用反汇编确定，不再猜）

### 33.5 本轮方法论沉淀

已写入 `references/static-native-triage.md`：

**「从错误消息反推到功能函数」**
——错误码表是逆向的"路标"：破译 `code -> message` 映射后，
找传特定 code 的调用点，再往前看它在判断什么函数的返回值，
即可定位目标函数。
本例：`code 9 = decompress failed` → 调用点 `0x2DA77D` ←
`cmp eax,-1; jle` ← `call 0x83C700` ⇒ 解压函数。

**跳转表识别模式**（x64 MSVC 风格）：
```asm
cmp  reg, N
ja   default
lea  rdx, [rip - <某值>]     ; 常使 rdx = 0
mov  ecx, [rdx + reg*4 + TABLE_RVA]
add  rcx, rdx
jmp  rcx
```
⇒ 表基址 = `TABLE_RVA`，表项为**绝对 RVA**。

**注意**：`hook 成功但 0 次调用` 是有意义的结论——
说明该路径在当前场景未被使用，不要急着否定地址正确性，
应先确认"目标功能是否真的发生"。
