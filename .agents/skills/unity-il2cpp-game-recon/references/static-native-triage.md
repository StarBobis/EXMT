# 去符号 64 位 DLL 的秒级静态定位

## 适用场景

拿到一个**导出极少或没有导出**的原生 DLL（Unity 的 `UnityPlayer.dll`、
游戏保护层、闭源中间件），需要在可接受时间内回答：

- 「某个功能在哪个函数里」
- 「这个错误/日志字符串是谁打印的」
- 「这个 API 的真实实现地址」

**不适用**于需要完整调用图或类型恢复的场景——那仍然要上 Ghidra/IDA。

## 为什么需要专门的手法

实测：Ghidra 对 25 MB 的 `UnityPlayer.dll` 做全量自动分析会**直接超时**。
而下面的三件套在同一文件上**秒级**给出结果。

三件套 = **字符串提取 + 交叉引用扫描 + `.pdata` 函数边界**，最后用 **capstone** 局部反汇编。

## 1. PE 解析：拿到节表与镜像基址

```python
pe = struct.unpack_from("<I", data, 0x3C)[0]          # e_lfanew
nsec = struct.unpack_from("<H", data, pe + 6)[0]
optsz = struct.unpack_from("<H", data, pe + 20)[0]
magic = struct.unpack_from("<H", data, pe + 24)[0]    # 0x20B = PE32+
base = struct.unpack_from("<Q", data, pe + 24 + 24)[0]   # ImageBase（PE32+）
so = pe + 24 + optsz
# 每个节 40 字节：name[8], VirtualSize(+8), VirtualAddress(+12), SizeOfRawData(+16),
#                 PointerToRawData(+20), Characteristics(+36)
```

### 节标志常量（**极易搞错，本项目踩过**）

| 常量 | 值 |
|---|---|
| `IMAGE_SCN_CNT_INITIALIZED_DATA` | `0x00000040` |
| `IMAGE_SCN_MEM_EXECUTE` | `0x20000000` |
| `IMAGE_SCN_MEM_READ` | `0x40000000` |
| **`IMAGE_SCN_MEM_WRITE`** | **`0x80000000`** |

**坑**：把"可写"误写成 `0x2`（那是 `IMAGE_SCN_MEM_16BIT`）会让所有过滤条件静默失败，
表现为"扫描结果为空"却不报错。**先打印节表核对，再写过滤逻辑。**

## 2. `.pdata` → 全量函数边界

x64 PE 的 `.pdata` 是 `RUNTIME_FUNCTION` 数组，**每项 12 字节**：

```python
for i in range(section["VirtualSize"] // 12):
    st, en, unwind = struct.unpack_from("<III", data, raw + i * 12)
    if st and en > st:
        rows.append((st, en))     # 都是 RVA
rows.sort()
```

实测产出：

| 文件 | 函数记录数 |
|---|---|
| `UnityPlayer.dll` (25.9 MB) | **87,065** |
| `GameAssembly.dll` (93.4 MB) | 约 20 万（`0x230460 / 12`） |

有了它，任意指令地址 → 所属函数入口，只需二分查找：

```python
def func_of(rva, rows):
    lo, hi = 0, len(rows) - 1
    while lo <= hi:
        m = (lo + hi) // 2
        if rva < rows[m][0]:   hi = m - 1
        elif rva >= rows[m][1]: lo = m + 1
        else: return rows[m][0]
```

**这是"找到函数入口可直接下断点"的关键**——比用反汇编启发式切函数可靠得多。

## 3. 字符串提取（只在 `.rdata` 里找）

```python
i = 0
while i < n:
    if 32 <= blob[i] < 127:
        j = i
        while j < n and 32 <= blob[j] < 127: j += 1
        if j - i >= 8:
            yield (section_va + i, blob[i:j].decode("latin1"))
        i = j
    else:
        i += 1
```

长度阈值取 8：太短（4）会命中海量噪声，太长（16）会漏掉 `get_XX` 这类短名。

## 4. 交叉引用扫描：RIP 相对 `lea`

x64 里取字符串地址几乎都用 `lea reg, [rip+disp32]`：

```
48 8D <modrm> <disp32>      ; REX.W + LEA
4C 8D <modrm> <disp32>      ; REX.WR（r8-r15）
```

RIP 相对的 modrm 满足 `(modrm & 0xC7) == 0x05`。目标地址：

```python
for i in range(tb, tb + tsize - 7):
    if data[i] in (0x48, 0x4C) and data[i+1] == 0x8D and (data[i+2] & 0xC7) == 0x05:
        disp = struct.unpack_from("<i", data, i + 3)[0]
        tgt_rva = o2rva(i + 7) + disp          # 相对下一条指令
```

命中后把 `insn_rva` 经 `.pdata` 映射到函数入口，即得"引用该字符串的函数"。

## 5. 局部反汇编（capstone）

```python
from capstone import Cs, CS_ARCH_X86, CS_MODE_64
md = Cs(CS_ARCH_X86, CS_MODE_64)
for ins in md.disasm(code[off:], image_base + func_rva):
    print(hex(ins.address), ins.mnemonic, ins.op_str)
```

只反汇编目标函数（用 `.pdata` 的 `st..en` 截取），几十到几百条指令，秒级。

## 6. 顺手解出跳转表（switch 分发的常见形态）

编译器对 `switch (code)` 常生成"取相对偏移 + 加基址 + `jmp`"：

```asm
lea  rcx, [rip - X]              ; rcx = 某个锚点（常见就是镜像基址）
movsxd rax, edx                  ; edx = switch 变量
mov  edx, dword [rcx + rax*4 + TBL]   ; 读 32 位相对偏移
add  rdx, rcx                    ; 变绝对地址
jmp  rdx
```

由 `lea` 的 `rip+disp` 反算出锚点，再从 `TBL` 读 `N` 个 int32、加上锚点，
即得**每个 case 的绝对地址**。这样就能把「错误码 → 分支」完整对上，
从而知道**某个错误码对应哪条分支、哪个字符串**。

## 7. 判断"某函数是不是校验/加载器"的可靠做法

**不要**仅凭调用约定或"返回值被当成错误码"就下结论。本项目犯过这个错：
见某函数返回值经 `esi` 进入错误码参数，就断定它是校验函数；
反汇编后发现只有 248 字节、含 strlen 循环和 `0x4b`/`0x49` 这类
**std::string 小字符串优化容量常量** ⇒ 它只是字符串辅助函数。

**可靠判据（按强度排序）**：

1. 函数体内有**目标语义的算法特征**：CRC 有 `CRC32` 多项式常量
   （`0xEDB88320` 或其反射形式）与移位循环；哈希有对应初始值/常量。
2. 函数体内**直接 `lea` 到目标错误文案**（而不是通过分发表间接引用）。
3. 函数长度与复杂度符合目标（校验器通常不会只有 248 字节且只有字符串操作）。
4. 交叉验证：把候选地址下断点，看**运行时是否被期望的路径命中**。

## 8. 检查清单

- [ ] 打印节表，确认节标志常量正确（`MEM_WRITE = 0x80000000`！）
- [ ] 建 `.pdata` 函数边界表并二分查找
- [ ] `.rdata` 提字符串（阈值 8）
- [ ] 扫 `48/4C 8D ?? disp32` 且 `(modrm & 0xC7) == 0x05`
- [ ] 指令地址 → 函数入口（`.pdata`）
- [ ] 目标函数用 capstone 局部反汇编（不要全量）
- [ ] switch 分发：解相对偏移表
- [ ] **不要**仅凭参数流向断定函数职责；找算法常量或多重证据
- [ ] 最终以**运行时命中**验证地址正确性