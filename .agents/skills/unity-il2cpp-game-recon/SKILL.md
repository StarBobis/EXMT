---
name: unity-il2cpp-game-recon
description: Fingerprint a shipped Unity game's asset pipeline and find moddable hook points. Covers Mono vs IL2CPP detection, AssetBundle/Addressables/hot-update cache layouts, encrypted or obfuscated UnityFS headers, IL2CPP export renaming, and runtime artifact location with Frida. Use when asked how a Unity game loads assets, where to hook for a mod, why picks/bundles won't open, or when a Unity game ships an opaque protection DLL.
metadata:
  version: "1"
---

# Unity IL2CPP 游戏侦察

面向"拿到一个已发行的 Unity 游戏，要搞清它怎么加载资源、以及从哪里 hook 最方便"的任务。
产出应当是**可执行的 hook 方案**，不是格式猜测。

## 铁律

1. **不要猜格式。** 每个格式断言都要有一个独立可校验的字段来验证（见下文的"文件大小自校验"）。
2. **先做便宜的静态指纹，再上动态。** 目录布局 + 魔数直方图的成本是秒级，能排除掉大部分错误方向。
3. **加密与否决定路线。** 明文 AssetBundle → 可以离线解包改资产；加密 → 必须动态定位解密点。
4. **区分"工具能拦"和"资产能改"。** 拦到 Mesh 是一回事，产出游戏能接受的资产是另一回事。

## 步骤 1：静态指纹（秒级，先做）

### 1.1 引擎版本与脚本后端

| 线索 | 含义 |
|---|---|
| `*_Data/il2cpp_data/Metadata/global-metadata.dat` + `GameAssembly.dll` | **IL2CPP** 构建 |
| `*_Data/Managed/Assembly-CSharp.dll` | **Mono** 构建（可用 dnSpy/ILSpy 直接读） |
| EXE / `UnityPlayer.dll` 的 **PE 版本资源** `ProductVersion` | Unity 精确版本，如 `2019.4.40.16762411` |
| `*_Data/app.info` | 公司名与产品名（可能 GBK 编码被当 UTF-8 读成乱码） |
| `*_Data/boot.config` | 启动参数 |
| `*_Data/globalgamemanagers`、`resources.assets`、`level0` | 内置资源（SerializedFile） |

```powershell
(Get-Item "$g\UnityPlayer.dll").VersionInfo.ProductVersion
```

### 1.2 资源存放形态

| 目录特征 | 含义 |
|---|---|
| `StreamingAssets/aa/` + `catalog.bin` + `settings.json` | **Addressables**（内容寻址，bundle 名为 MD5） |
| `LocalCache/Data/AssetBundles_<Platform>/` | **热更新下载的 AssetBundle 库**（手游型架构，往往是真正的大头） |
| `StreamingAssets/aa/<Platform>/*.bundle` | Addressables 本地引导包，**通常是明文的** |
| `*_Data/Table/*.bytes` 且首 4 字节是小端长度前缀 | protobuf 配置表，通常明文 |
| `Plugins/x86_64/` | 原生插件（音频 CRIWARE、Live2D、USD、反作弊…） |

**优先看 `LocalCache` 这种热更新目录——它常常是 90%+ 的资源体积所在。**

### 1.3 魔数直方图（判断是否加密的最快手段）

对目标目录**所有** bundle 取前 8 字节做直方图，一次就能看出有几类格式：

```powershell
$ab = "<bundle 目录>"
$files = Get-ChildItem -Force -File -LiteralPath $ab -Filter *.bundle
$sig = @{}
$files | ForEach-Object {
  $fs=[System.IO.File]::OpenRead($_.FullName); $b=New-Object byte[] 16; $null=$fs.Read($b,0,16); $fs.Close()
  $k = -join ($b | ForEach-Object { if ($_ -ge 32 -and $_ -lt 127){[char]$_}else{'.'} })
  $k = $k.Substring(0,8)
  if (-not $sig.ContainsKey($k)) { $sig[$k]=0 }; $sig[$k]++
}
$sig.GetEnumerator() | Sort-Object Value -Descending | Select-Object -First 12
```

判读：
- 出现 `UnityFS.` → 明文标准 AssetBundle。
- 出现**少量但非平凡的近似图案**（如 `....yFS.` 反复出现 x339，而随机数据里三字节精确匹配概率约 1/16M）→ **这不是巧合，是头部被变换过的 UnityFS**。直接进入步骤 2。
- 全部是随机字节 → 可能是整体加密，或不是 Unity bundle。

## 步骤 2：UnityFS 头部结构与已知明文攻击

标准 UnityFS 头（小端序字段，大端序整数）：

```
0x00  char     signature[8]        "UnityFS\0"
0x08  UInt32BE version            6/7/8
0x0C  char     minPlayerVersion[] 以 \0 结尾，如 "5.x.x"
0x12  char     engineVersion[]    以 \0 结尾，如 "2019.4.29f1"
...   Int64BE  fileSize           ← 自校验锚点
      UInt32BE compressedBlocksInfoSize
      UInt32BE uncompressedBlocksInfoSize
      UInt32BE flags
```

**核心方法：用 `fileSize` 字段自校验你的解密假设。**

对每个候选密钥流，解出头部后读出 `fileSize`，与磁盘上该文件的实际长度比对。**一致即证明密钥流正确**——这是唯一不需要"看起来对"的判据。

### 典型混淆：每文件独立的 XOR 密钥流

实测遇到过的一种：偶数 4 字节字被变换、奇数 4 字节字保留。逐字节解密钥流：

```
观测: 84 67 D5 44 | 79 46 53 00 | D3 09 BC 37 | 35 2E 78 2E | A9 09 8E 00 | 31 39 2E 34 | FD 3B 85 56 | 31 00 00 00
假设: 55 6E 69 74 | 79 46 53 00 | 00 00 00 07 | 35 2E 78 2E | 78 00 32 30 | 31 39 2E 34 | 2E 32 39 66 | 31 00 00 00
      "Unit"       "yFS\0"       version=7      "5.x."        "x\0""20"      "19.4"        ".29f"         "1\0\0\0"
密钥: D1 09 BC 30 | 00 00 00 00 | D3 09 BC 30 | 00 00 00 00
```

推导要点：
- 从**确定的 ASCII 串**反推密钥（`"Unit"`/`"yFS\0"`/`"5.x."`/`"19.4"`）。
- `version` 字段给出结构约束（合法值是 6/7/8）。
- 最终用 `fileSize` 确认。

**密钥流予期**：先假设密钥流周期为 4 / 8 / 16 字节，用"某个字节位置在不同字上解出的密钥字节是否一致"来判断周期。此例中 byte0 呈 D1,D3,D1,D3（周期 16），byte1-3 恒定（周期 4），故整体周期为 16。

### 保留魔数的"体加密"

另一种手法：**前 4 字节魔数保持正确，其余全部加密**，用于骗过只检查魔数的工具。

实例（IL2CPP 元数据）：
```
AF 1B B1 FA  ← 0xFAB11BAF 正确魔数
E8 7C 11 01  ← version = 0x01117CE8，合法值应为 24
```
**判据：魔数对但紧随其后的结构字段是垃圾值 → 体被加密。** 此时静态解析器（Il2CppDumper 等）必然失败，必须转运行时内存扫描（步骤 5）。

## 步骤 3：保护层定性（廉价 PE 启发式）

不要一看到大 DLL 就假定是反作弊。用这三项几秒定性：

| 特征 | 指向 |
|---|---|
| **只有 1 个导出**（如 `NEP_Unknown`） | 壳/保护器的转发 stub |
| 非 MSVC 标准节名：`.ndata0/.ndata1/.fptable/Shared/.200` | 加壳器（VMProtect/Themida 类） |
| 导入 `CRYPT32` + `WINTRUST` + `bcrypt` | 签名/授权校验 |
| 导入 `tdh.dll` | ETW 追踪 |
| `.rsrc` 异常大（如 26MB） | 内嵌载荷 |
| **有 `.managed` 节 + 导出 `CreateProcess`/`GetProcessId` 之类** | **是 .NET 崩溃上报器，不是反作弊**（别误判） |
| 导出全是 `_CalcBloomPass`/`_Ceil`/`_GetShaderString` | 着色器数学库，非解密器 |

另外看主 EXE 的**导入表**：如果它只导入 `<保护DLL>`、`UnityPlayer.dll`、`KERNEL32.dll`，说明保护层先于 Unity 加载并包住了引擎。

## 步骤 4：Frida 动态侦察

### 4.1 Windows 上没有 pip/frida 时的自举

```powershell
# DSH 自带 Python（系统 python.exe 可能是 0 字节的 Store 存根）
$PY = "<dsh-runtimes>\dependencies\python\python.exe"
& $PY -m pip install frida frida-tools
& $PY -c "import frida; print(frida.__version__)"
```

REA 的 Frida 适配器可能因缺 `frida_binding.node` 不可用；此时**直接走 Python frida 绑定**，不要卡在修适配器上。

### 4.2 两阶段 payload 架构（关键）

**`device.spawn()` 的注入点在 loader 初始化之后、入口点之前。** 后果：
- 静态导入的保护 DLL 的 `DllMain` **已经执行完毕** —— 它的 hook 已经装好，你装在其上层或下层取决于谁先 patch。
- `GameAssembly.dll` **尚未加载** —— IL2CPP 初始化必然失败。

因此 payload 必须分两阶段：

```
阶段1（脚本加载即执行）：不依赖 IL2CPP 的原生 hook
  - kernel32 CreateFileW（建立 HANDLE→路径 映射，这是"哪个文件在被解密"的唯一线索）
  - kernel32 ReadFile（抓密文；用 handleMap 归属到文件名）
阶段2（轮询到 GameAssembly.dll 后）：IL2CPP 元数据 + 托管边界 hook
  - setTimeout 每 100ms 轮询 Process.findModuleByName('GameAssembly.dll')
```

阶段 1 必须在最前面，因为它能捕获 Unity **启动期**对 `globalgamemanagers`/`resources.assets` 的解密——这是攻击面最窄也最有价值的窗口。

### 4.3 调用栈符号化

`UnityPlayer.dll` 在发行版里**通常只导出 `UnityMain`**，所以 `DebugSymbol.fromAddress` 会把成百上千个内部函数全标成 `UnityMain`，毫无用处。

有效做法：
- 对 `GameAssembly.dll`：枚举 IL2CPP 元数据里的所有方法，把 `MethodInfo->methodPointer`（偏移 0x00）建成**排序数组**，用二分查找把回溯地址翻成 `命名空间.类::方法`。这一步能把"一堆地址"变成"可读的 C# 调用栈"，是定位解密逻辑的关键。
- `MethodInfo` x64 布局：`methodPointer`@0x00、`name`@0x10、`klass`@0x18。
- `Il2CppArray`（byte[]）：长度 @0x18，数据 @0x20。
- 除非必要，索引时**跳过 mscorlib/System/UnityEngine**，否则会膨胀到 50 万+ 条。

## 步骤 5：运行时定位解密后的产物

当静态解析受阻（元数据加密、导出改名），**不要在内存里找算法，去找结果**：

```javascript
// 找解密后的 IL2CPP 元数据：魔数 + 合法 version 字段
Memory.scanSync(base, size, 'AF 1B B1 FA');
// 命中后读 +4 处的 version，合法范围(1..64)才说明这是解密后的真元数据

// 找解密后的 AssetBundle
Memory.scanSync(base, size, '55 6E 69 74 79 46 53 00');   // "UnityFS\0"
// 命中后读 +0x1E 处的 Int64BE fileSize 作为精确 dump 长度
```

**两条判据缺一不可**：魔数命中只能说明"可能有"，紧跟其后的结构字段（version / 声明大小）才说明"是"。

解出明文元数据后，即可离线用 Il2CppDumper 产出完整类/方法名与 RVA，从而绕开"导出被改名"这一障碍。

### 内存访问监视（捕获就地解密）

`MemoryAccessMonitor.enable` 可以捕获对某页面的访问。实测注意：
- 在 `ReadFile` 的 **onLeave** 里布监视，看到的多是 `op:'read'`（消费方读），写已经发生过了。
- 要捕获"谁写了明文"，需要在缓冲区被填充**之前**布监视，或在 Stalker 下追踪。

## 步骤 6：给出 hook 方案

按语义能力排序，优先选**能拿到完整语义**的落点：

| 层 | 落点 | 语义 | 备注 |
|---|---|---|---|
| 资产字节 | `SerializedFile::ReadObject` | 全类型 | 格式逐版本变，成本最高 |
| **原生对象** | **`Mesh::AwakeFromLoad` / `UploadMeshData`** | **完整** | 推荐主后端；与图形 API 无关 |
| 托管 API | `Mesh.SetVertices` / `ApplyAndDisposeWritableMeshData` | 完整 | 覆盖不到烘焙网格 |
| GPU 命令 | `DrawIndexed` + buffer 绑定 | 仅原始流 | 3DMigoto 路线；受实例化/GPU 蒙皮限制 |

**Unity 上纯 GPU 路线的四个已知障碍**：
1. 顶点布局 per-mesh 变化（`VertexDataLayout` + channel），且无法从 `IASetInputLayout` 反推通道语义。
2. **多个 mesh 打包进同一大 buffer 用 byte_offset 区分** —— 哈希整个 buffer 无法唯一标识 draw。3DMigoto 为此新增了 `track_region_hashes` 只哈希 draw 实际引用的区间。
3. 实例化 / GPU-driven 间接绘制使"这是哪个对象"不可判定。
4. GPU 蒙皮（buffer 是绑定姿态）+ BlendShape（按顶点 ID 索引结构化缓冲）→ 顶点数/顺序一变就全废。

## 常见坑（都踩过）

| 坑 | 症状 | 对策 |
|---|---|---|
| PowerShell 十六进制字面量溢出 Int32 | `0xFAB11BAF -eq [BitConverter]::ToUInt32(...)` 返回 False，误判"魔数不对" | 用 `[uint32]`/`[int64]` 显式转换，或比较字符串 |
| `Get-ChildItem -File <子目录>` 返回空 | 文件其实在**父目录**里，子目录只有别的 | 先无条件计数，看到 0 就往上/往下各试一级 |
| GBK 中文被按 UTF-8 读 | `app.info` 显示 `灏戝コ鍓嶇嚎2` | 用 GBK(936) 重读；或直接忽略 |
| 终端中文乱码但日志正常 | 控制台代码页问题，非数据问题 | 输出重定向到 UTF-8 文件再读 |
| `objdump` / pwntools 类 REA 工具在 Windows 不可用 | `capability_unavailable: unsupported_host` | 改用 Ghidra provider，或自己用 PowerShell 解析 PE |
| Frida spawn 时 GameAssembly.dll 未加载 | 全部 IL2CPP 导出解析为 0 | 两阶段架构 + 轮询 |
| 假定"大 DLL = 反作弊" | 误判可行性 | 看 `.managed` 节与导出种类；崩溃上报器很常见 |
| 默认"整文件都被加密" | 头对、数据全错；改包写回后 Unity 报 `File may be corrupted` | 用 N 扫描 + "所有压缩块都能解出"当判据定边界（实测常是前 32KB），见 references §16 |

## 交付物清单

一次完整侦察应产出：
1. 引擎版本 + 脚本后端 + 图形 API
2. 资源存放拓扑（哪里有明文、哪里有密文、体积分布）
3. 加密方案定性（是否加密、算法族、密钥来源、能否离线复现）
4. 保护层定性（是保护/反调试还是无关组件）
5. **推荐 hook 点 + 备选 + 各方案的能力边界**
6. 明确的未知项

## 参考

- [UnityFS 与混淆恢复](references/asset-format-and-obfuscation.md)
- [SerializedFile 内改 Mesh（换衣服/换网格）](references/sf-mesh-editing.md) — §21 绑定空间重定向、§22 批量提贴图、§23 骨骼只有哈希
- [Frida for IL2CPP 运行时手册](references/frida-il2cpp-runtime.md)
