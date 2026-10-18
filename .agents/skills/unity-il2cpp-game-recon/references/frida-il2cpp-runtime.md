# Frida + IL2CPP 运行时手册（Windows）

针对「已发行的 Unity IL2CPP 游戏，要在 Windows 上做动态分析」的场景。

## 1. 环境自举

**系统 `python.exe` 可能是 0 字节的 Microsoft Store 存根。** 检验：

```powershell
(Get-Item "C:\...\WindowsApps\python.exe").Length   # 0 表示存根，不可用
```

改用 DSH 自带运行时（`load_workspace_dependencies` 可取得路径）：

```powershell
$PY = "<dsh-runtimes>\dependencies\python\python.exe"
& $PY -m pip install frida frida-tools
& $PY -c "import frida; print(frida.__version__, [d.name for d in frida.enumerate_devices()])"
```

REA 的 Frida 适配器可能因缺 `frida_binding.node` 不可用（`Adapter frida could not load`）。
此时**直接走 Python frida 绑定**，不要卡在修适配器。

## 2. 注入可行性先验证

用一个无害进程做冒烟测试，确认「注入链 + 脚本运行时 + 消息管道 + 优雅失败」四件事都通：

```powershell
& $PY run.py attach-name PCLauncher.exe --seconds=6
```

脚本首行应检查目标模块是否存在，缺失则打印后 return，而不是抛异常。

## 3. 两阶段 payload 架构（关键）

`device.spawn()` 的注入点在 **loader 初始化之后、入口点之前**。后果：

- **静态导入的保护 DLL 的 `DllMain` 已经跑完** —— 它的 hook 已装好，
  你装的是在其上层还是下层取决于谁先 patch。
- **`GameAssembly.dll` 尚未加载** —— 任何 IL2CPP 初始化必然失败。

因此必须分两阶段：

```
阶段1（脚本加载即执行）：不依赖 IL2CPP 的原生 hook
  - kernel32 CreateFileW  → 建立 HANDLE→路径 映射（"哪个文件在被解密"的唯一线索）
  - kernel32 ReadFile     → 抓密文，用 handleMap 归属文件名
阶段2（轮询到 GameAssembly.dll 后）：IL2CPP / 托管边界 hook
  - setTimeout 每 100ms 轮询 Process.findModuleByName('GameAssembly.dll')
```

阶段 1 必须最靠前，因为它能捕获 Unity **启动期**对 `globalgamemanagers` /
`resources.assets` 的解密——这是攻击面最窄、价值最高的窗口。

## 4. 用调用栈证明"谁在做解密"

发行版 `UnityPlayer.dll` 通常**只导出 `UnityMain`**，于是 `DebugSymbol.fromAddress`
会把成百上千个内部函数全标成 `UnityMain`，毫无区分度。

**但相邻帧里的第三方 DLL 导出仍然能解析。** 实测这种栈极具说服力：

```
UnityMain → 0x7ff8fd83dbf1 → NEP_Unknown (NEP2.dll 的唯一导出) → UnityMain
```

⇒ 证明保护 DLL 在 Unity 的文件读取路径上做了 detour。

## 5. 内存扫描：慢，且必须双重校验

### 性能
`Memory.scanSync` 扫全部可读区间非常慢（实测 5.3 GB 可用内存、单次约 15-20 秒）。
不要在每个 tick 里全量重扫。改进：
- 先 `Process.enumerateRanges` 统计总量，判断可行性
- 设扫描字节上限，超限即停并报告已扫量
- 精确化 pattern（4→8 字节）以降低误报

### 校验
**魔数命中只能说明"可能有"，紧跟其后的结构字段才说明"是"。**

```javascript
Memory.scanSync(base, size, '55 6E 69 74 79 46 53 00');   // "UnityFS\0"
// 命中后读 +0x1E 处的 Int64BE 声明大小，合理才 dump

Memory.scanSync(base, size, 'AF 1B B1 FA');               // IL2CPP 元数据
// 命中后读 +4 处的 version，合法范围(1..64)才说明是解密后的真元数据
```

实测反例：扫 `"UnityFS\0"` 得到 20 个命中，`minPlayer` 全是乱码、`declared` 巨大无意义
→ 全部是随机内存误报。只有 `minPlayer = "m/GF2 Ga"`（内存里的文件路径串）勉强可解读。

### dump 必须按区间裁剪
`addr.readByteArray(len)` 越过所在区间末尾会抛 `access violation`。
正确做法：

```javascript
const avail = range.base.add(range.size).sub(addr).toInt32();
const len = Math.max(16, Math.min(want, avail));
```

### MemoryAccessMonitor 的实际行为
- 在 `ReadFile` 的 **onLeave** 里布监视，看到的几乎都是 `op:'read'`（消费方读取），
  写已经发生过了。
- 要捕获"谁写了明文"，必须在缓冲区被填充**之前**布监视，或用 Stalker 追踪。
- 页面级粒度，会与内核写入（如 `ReadFile` 填充缓冲区）冲突，需谨慎。

## 6. 「按需解密」的识别与应对

若运行时扫到魔数、但**结构字段仍是垃圾值**，且命中区间大小恰好等于磁盘文件大小
（注意有 `r--` 与 `rw-` 两份）⇒ 游戏把加密文件 **mmap 进内存、按需解密单个结构**，
不存在解密后的完整 blob。

此时不要继续找"完整明文"，改为：
1. 用同版本托管程序集取名称（见下节），从 `GameAssembly.dll` 的
   `Il2CppCodeRegistration.methodPointers` 补原生地址
2. Hook 元数据访问函数，逐结构捕获
3. 放弃名称，直接 Ghidra + 字符串 xref

## 7. 读 .NET 程序集的三种工具与选择

实测三种方案的性能/健壮性差异极大：

| 工具 | 32MB 程序集读取耗时 | 对畸形 IL 的容忍度 | 适用 |
|---|---|---|---|
| **Mono.Cecil** | **40 ms** | **高**（只按操作码长度走，不解析令牌） | **首选**，枚举/dump/改写 |
| dnfile（Python） | 约 10 分钟 | 中 | 仅当必须用 Python |
| ILSpy CLI | 数秒/类型 | **低**（`RequiredNamespaceCollector` 会因越界令牌崩溃） | 出高质量 C#，但需先修 IL |

### Mono.Cecil 的正确用法（PowerShell 宿主）

ilspycmd 的 nupkg 里自带 `Mono.Cecil.dll`，可直接 `Add-Type` 使用：

```powershell
Add-Type -Path "<ilspy-extract>\tools\net6.0\any\Mono.Cecil.dll"
$asm = [Mono.Cecil.AssemblyDefinition]::ReadAssembly($dll)
$mod = $asm.MainModule
# 遍历类型/方法/指令，操作数按类型格式化
```

注意 dnfile 的坑（如果非用不可）：`TypeDef.MethodList` 是 **list**，
其 `[0].row_index`（1-based）才是方法区间起点；`MethodDef` 行本身不带行号，
需 `enumerate(md.MethodDef, 1)` 自行编号。

### 无 dotnet SDK 时装 ilspycmd

`dotnet` 只有运行时没有 SDK 时 `dotnet tool install` 失败。变通：

1. 查运行时版本：`dotnet --list-runtimes`
2. 按运行时选 ilspycmd 版本（**ilspycmd 7.2 → .NET 6**；8.x → .NET 8）
3. 直接下 nupkg：
   `https://www.nuget.org/api/v2/package/ilspycmd/7.2.1.6856`
4. **`.nupkg` 是 zip，但 `Expand-Archive` 不认这个扩展名** → 先复制成 `.zip` 再解压
5. `dotnet <extract>\tools\net6.0\any\ilspycmd.dll --version`

### 修掉畸形 IL 让 ILSpy 可用

若确定某些方法含 token 无效的 `calli`/`jmp`（ILSpy 报 `Invalid token` /
`Read out of bounds`），可用 Cecil 把它们整体替换为 `nop`：

```powershell
$ill = $m.Body.GetILProcessor()
$ill.Replace($instr, [Mono.Cecil.Cil.Instruction]::Create($OpCodes::Nop))
```

**但要注意**：改了 IL 之后 `$asm.Write()` 可能因元数据重建不一致而抛
`IndexOutOfRangeException`；且 19 万方法级的替换耗时可达 5 分钟。
**如果目的只是"读代码"，直接用 Cecil dump IL 更快，不要走改写路线。**

## 8. 先判断"托管方法体是否可信"

**这是最容易踩的坑**：看到游戏发托管 DLL，就以为能直接反编译出逻辑。

判据（都能快速做）：

1. 抽样看方法体 IL 是否有意义。若出现 `bgt.s` 但跳转目标为 null、
   `ldc.i4 <随机大数>` + `nop` 等组合 → 是垃圾填充。
2. 看 async 状态机的 `MoveNext` —— 真有逻辑的必定非空。
3. **对比不同类的同名方法**：若两个无关类的 `get_Current` 具有完全相同的乱码序列，
   只能是填充（不同类不可能同实现）。

若判定为填充 ⇒ 该 DLL 是 **IL2CPP 的元数据载体程序集**（类型/字段/签名完整、方法体剥离），
真实逻辑在 `GameAssembly.dll` 原生代码中。

**但它仍极有价值**：提供完整的 API 地图（类型/方法/字段名 + 字符串字面量）。
IL2CPP 元数据与托管元数据信息等价，所以**名称可以从托管 DLL 免费获得**，
只需另外补上"名称 → 原生地址"的映射。

## 9. 检查清单

- [ ] 确认 python/frida 真实可用（避开 Store 存根）
- [ ] 冒烟测试注入链
- [ ] payload 分两阶段，阶段 1 不依赖 IL2CPP
- [ ] HOOK `CreateFileW` 建 handle→路径映射
- [ ] 内存扫描有字节上限 + 双重字段校验 + 区间裁剪
- [ ] 判定元数据是"整体解密"还是"按需解密"
- [ ] 判定托管方法体可信还是填充
- [ ] 读 .NET 用 Mono.Cecil，不要默认上 ILSpy

## 10. ❗ 对 Unity **已加载**的资产：全内存扫描必然 0 命中

这是一个代价很大的坑，本项目为此浪费了两整轮。

**现象**：进程正常工作、窗口在渲染，但全内存扫描
`"UnityFS\0"`（5.3 GB / 3,300 个区间）命中 **0**；
扫描 SerializedFile 头（用已知磁盘大小 + version 作 8 字节模式）同样命中 **0**。

**原因**：Unity 加载 AssetBundle / SerializedFile 时，**解析完头部就丢弃原始文件映像**，
只保留目录表、block 表等结构，之后按需解压 block。
因此**内存里不存在连续的、以 `UnityFS\0` 开头的原始文件字节**。

同理，若游戏把加密文件 `mmap` 进内存并按需解密单个结构（常见保护手法），
也找不到"解密后的完整明文"。

**正确的判断顺序**：

1. 先问：**目标数据会被持久保留在内存里吗？**
   - 会不会被"解析后丢弃"？（bundle/SerializedFile 头 → 会）
   - 会不会"按需解密、不落地完整明文"？（元数据类结构 → 会）
2. 若会被丢弃/按需解密 ⇒ **扫描无效，必须 hook 读取/解密/解压那一刻**。
3. 只有确认数据会被持久保留（如解压后的 Mesh/Texture 常驻堆）才值得扫描。

**判据**：如果扫描 0 命中但进程明明在渲染/工作，**先怀疑方法，不要先怀疑目标没加载**。

## 11. 三重指标区分「活着但在空转」vs「真在做资源加载」

在花时间做内存扫描或 hook 之前，先用三个廉价指标判断进程实际状态：

| 指标 | 取法 | 判读 |
|---|---|---|
| 主窗口标题/句柄 | `Get-Process -Id N` 的 `MainWindowTitle`/`MainWindowHandle` | 非空 ⇒ 引擎已起、在渲染 |
| CPU 累计 | `(Get-Process -Id N).CPU` | 对角秒数 ≈ 持续占用核数；>1 核说明在渲染 |
| **`CreateFileW` 调用次数** | hook `kernel32!CreateFileW` 计数 | **0 = 完全没开文件，肯定没加载资源** |

配套：同时 hook `ReadFile` 计数。若 `opens=0` 而 `reads>0`，
说明读的是**早已打开的句柄**（心跳/日志/遥测），不是资源加载。

**实测样例**（停在标题界面的 Unity 游戏，50 秒窗口）：

```
窗口标题 = "少女前线2：追放"   CPU 累计 = 3132 秒（约 1.1 核）
opens = 0    reads = 46    assetReads = 0
```

⇒ 引擎在渲染（所以有窗口、有 CPU），但**零资源加载**。
此时任何"扫描内存找资产"的尝试都注定为空——问题不在方法，在于**游戏压根没走到那一步**。

**必须先让游戏进入真正加载资源的状态**（过登录、进对局），再做捕获。
这一步无法靠逆向绕过，属于外部前提。

## 12. ⭐ 无需解密元数据就能拿到 Unity API 地址：icall 静态槽

**这是绕开 `global-metadata.dat` 加密的通用解法**，适用于任何 IL2CPP 游戏。
不用 Il2CppDumper、不用解元数据、不用方法指针表。

### 原理

IL2CPP 调用 Unity 引擎 API（icall）的生成代码长这样：

```asm
mov  rax, [rip + D1]        ; 读缓存槽
test rax, rax
jne  <已解析则跳过>
lea  rcx, [rip + SIG]       ; rcx = icall 签名字符串
call <resolver>             ; 解析器
test rax, rax
je   <失败>
mov  [rip + D2], rax        ; ★ 把真实函数指针存进静态槽（D1、D2 指向同一槽）
call rax
```

⇒ **槽里就是 `UnityPlayer.dll` 内的真实地址。运行时读槽即得。**

### 三步定位

**第 1 步：在 `GameAssembly.dll` 的 `il2cpp` 段里搜签名字符串**

签名带完整参数类型，很好认：

```
UnityEngine.AssetBundle::LoadFromMemory_Internal(System.Byte[], System.UInt32, System.UInt64)
UnityEngine.AssetBundle::LoadFromFile_Internal(System.String, System.UInt32, System.UInt64)
UnityEngine.AssetBundle::LoadAsset_Internal(System.String, System.Type)
```

**坑：这些串不会被 `.text` 里的 `lea` 引用，只在 `il2cpp` 段内被引用。**
只扫 `.text` 会得到 0 命中（本项目踩过）。`il2cpp` 段常占几十 MB。

**第 2 步：从 `lea` 往后找 `mov [rip+D], rax`，算槽 RVA**

```python
# 找 lea rcx, [rip+SIG]
for i in range(text_start, text_end - 7):
    if data[i] in (0x48, 0x4C) and data[i+1] == 0x8D and (data[i+2] & 0xC7) == 0x05:
        disp = struct.unpack_from("<i", data, i + 3)[0]
        if o2rva(i + 7) + disp == sig_rva: ...
# 然后从该 lea 起反汇编 ≤96 字节，找 mov qword ptr [rip+D], rax
#   slot_rva = (insn_addr + insn_size + D) - image_base
```

**第 3 步：运行时读槽**

```javascript
const ga = Process.findModuleByName('GameAssembly.dll');
const fn = ga.base.add(SLOT_RVA).readPointer();   // 指向 UnityPlayer.dll
Interceptor.attach(fn, { onEnter(a) { /* ... */ } });
```

### 关键：icall 是**惰性解析**的

槽**只在对应 icall 被真正调用过之后才被填上**。实测（GF2，游戏停在标题界面）：

```
LoadFromFileAsync_Internal -> 0x7ff8ec1e7650 @ UnityPlayer.dll   ✅ 已解析
LoadAsset_Internal         -> 0x7ff8ec1e74a0 @ UnityPlayer.dll   ✅ 已解析
LoadFromFile_Internal      -> null                                ❌ 从未被调用
LoadFromMemory_Internal    -> null                                ❌ 从未被调用
```

⇒ **只读槽不够**，必须配合 hook 解析器。

### 推荐的三层策略

| 层 | 手段 | 作用 |
|---|---|---|
| **L1** | hook 解析器（`lea` 后面那个 `call` 的目标） | 入参 `rcx` = 签名字符串，返回 `rax` = 函数指针；**任何 icall 一解析就捕获**，不必等槽填上 |
| **L2** | 每 1~2 秒轮询槽 | 捕捉惰性解析结果 |
| **L3** | 拿到地址即刻 `Interceptor.attach` | 采集业务参数 |

解析器 hook 的核心：

```javascript
Interceptor.attach(ga.base.add(RESOLVER_RVA), {
  onEnter(args) { this.sig = args[0].readCString(256); },
  onLeave(ret) {
    if (!this.sig || ret.isNull()) return;
    // 用 this.sig 的签名子串匹配你要的 API
  }
});
```

### 顺带：读 IL2CPP 的 `System.String` 参数

```javascript
// Il2CppString: [klass(8)][monitor(8)][length(4)][utf16 chars...]
function il2cppString(p) {
  if (!p || p.isNull()) return null;
  const len = p.add(0x10).readS32();
  if (len < 0 || len > 8192) return null;
  return p.add(0x14).readUtf16String(len + 1);
}
```

### 这套方法能拿到什么

- 任意 Unity 引擎 API 的**真实地址**（AssetBundle、Resources、Texture、Mesh…）
- 这些 API 的**参数**（路径、CRC、资产名、bundle 指针）
- 完全**不需要**：Il2CppDumper、解密的 `global-metadata.dat`、方法指针表

**这是"要 hook Unity 引擎层"时的首选方案**，比破元数据加密便宜一个数量级。

### ⚠ 致命坑：icall 签名是**拼接 blob**，不能用子串匹配识别身份

`GameAssembly.dll` 里的 icall 签名不是一个个独立字符串，而是
**长度前缀拼接的大 blob**，一次 `readCString` 会读到好几个名字连在一起：

```
"UnityEngine.AssetBundle::LoadFromFileAsync_Internal(...)\xNN\xNN
 UnityEngine.AssetBundle::LoadFromFile_Internal(...)\xNN\xNN
 UnityEngine.AssetBundle::UnloadAllAssetBundles(System.Boolean)"
```

**后果**：用 `sig.indexOf('LoadFromMemory_Internal(') >= 0` 判身份，
会把 blob 里**任何位置**出现的该名字都算命中——
本项目因此把 `LoadFromStreamInternal` 误判成 `LoadFromMemory_Internal`，
导致按 `byte[]` 布局去解析、读出一堆垃圾（`len:0`、magic 全零）。

**正确做法**（任选）：
1. 用**完整签名含参数表**匹配：`'::LoadFromMemory_Internal(System.Byte[],System.UInt32)'`
2. 或先记录签名在 blob 内的**精确字节偏移**，只认该偏移
3. 或 hook 后在 `onEnter` 里**验证参数形态**（是 Il2CppArray 还是 Stream 对象），
   对不上就立刻报错而不是继续按错误布局解析

**自检**：若 hook 里读出的长度是 0、magic 全是 0、或指针看着像另一个对象，
**先怀疑 icall 身份认错了**，别怀疑目标数据。

## 13. 判断「资源完整性校验」是否真的生效：看调用点的 CRC 实参

不要猜，直接反汇编调用点。`LoadFromFile/LoadFromStream` 系列都带 `uint crc` 参数，
**传 0 就是关闭校验**（Unity 约定：crc==0 表示不校验）。

实测样例：

```asm
; CRC = 0 → 不校验
0x20511BC  mov r8d, esi          ; 缓冲大小
0x20511BF  xor edx, edx          ; ★ rdx = 0  ← CRC 参数
0x20511C1  mov rcx, rdi          ; Stream
0x20511C4  call rax              ; LoadFromStreamInternal(stream, 0, size)
```

对照带真实 CRC 的调用（`LoadFromFile`，4 次）：

```
crc=0x175fd061  aotres_assets_all_*.bundle
crc=0xfc18c503  aotres_scenes_all_*.bundle
```

**结论模式**：
- 启动期 / StreamingAssets 的少量包 → 常**带 CRC**，会校验
- 热更新下载的大量加密包 → 常**CRC=0**，不校验

**为什么这条最重要**：它决定"替换资源会不会被拦"。
若目标资源所在的那批 bundle 是 CRC=0，则**替换不会触发任何完整性校验**，
Mod 可行性直接成立——不需要去逆向校验函数本身。

**做法**：hook 加载 API → 记录每一次的 `(路径/长度, crc)` → 看你要动的那批的 crc 是不是 0。
