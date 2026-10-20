# 运行手册 · EXMT Modbox

> 这份是 `AGENTS.md` 的**细节层**：仓库地图、本机工具链、交付动作、质量门、写回半径。
> 可以长，但**不记流水账**：结论 / 证据 / 未决问题 → `docs/ledger.md`；
> 纠正 → `docs/agents-corrections.md`；换个目标也用得上的判据 → `.agents/skills/`。

---

## 1. 仓库地图

| 位置 | 是什么 | 怎么跑 |
| --- | --- | --- |
| `src/` | 前端（Vue 3 + TS + Vite）；视图 `src/views/`，样式 `src/styles/` | `bun run dev` |
| `src-tauri/` | Rust 外壳（Tauri 2），命令 `run_core` / `show_main_window` | `bun run tauri dev` |
| `core/exmt_core/` | Python 核心：`bundle.py` `sf.py` `mesh.py` `interchange.py` `textures.py` `preview.py` `bc1.py` `retarget.py` `subdivide.py` `anim.py` `cli.py` | `python -m exmt_core <cmd>`（cwd 必须在 `core`） |
| `C:\Dev\EXMTBlender\` | Blender 插件：导入/导出 `.exmtmesh`、传权重、蓝图、生成 Mod | Blender → 偏好设置 → 安装 `__init__.py` |
| `C:\Dev\EXMTDoc\` | VitePress 2 文档站（**独立 git 仓库**，无 remote） | `bun run build` / `bun run preview` |
| `docs/` | **台账**：结论 + 证据 + 未决问题（`ledger.md` 是入口） | — |
| `.agents/skills/` | **能力**：换目标也能用的判据/手法 | — |
| `C:\DevCache\<任务名>\` | 一次性脚本、临时产物 | — |

以仓库结构为准，别凭记忆：`AGENTS.md` 只给规则，细节在这里查。
下表与后面的 `bun run …` 都要按工具链那条走全路径：`C:\Users\Administrator\.bun\bin\bun.exe run …`。

---

## 2. 本机工具链（已验证）

- 应用：`bun run tauri dev`，窗口标题 `EXMT Modbox`
  ⚠️ **若报 `WebView2 error: HRESULT(0x8000FFFF) 灾难性故障`**：先别改代码。这是本机沙箱把工作区内的 exe
  以 Low IL 启动、导致 `%LOCALAPPDATA%\com.exmt.modbox\EBWebView` 建不出来。诊断与一次性修法见
  skill `tauri-winui-shell` §11（`icacls <appdata>\com.exmt.modbox /setintegritylevel (OI)(CI)LW`）。
  ⚠️ 本机 `bun` **不在 PATH 里**：一律用全路径 `C:\Users\Administrator\.bun\bin\bun.exe run <script>`
  ⚠️ `beforeDevCommand` 是 `node scripts/dev.mjs`：1420 上**已有 Vite 就直接复用**（打印 `reusing it` 并 exit 0），
  端口空才自己起，端口被别的程序占则 exit 1 并报出占用进程。所以**不要把残留的 dev server 当错误**；
  但收尾时要关掉自己起的（见铁律 13 与 skill `tauri-winui-shell` §9）。
- 核心 Python：`C:\Users\Administrator\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe`
  （UnityPy 1.25 / lz4 / numpy / PIL / texture2ddecoder）
- Blender：`C:\Program Files\Blender Foundation\Blender 5.2\blender.exe`
  （headless：`--background --factory-startup --python x.py`）
- 游戏：`C:\GF2Exilium\GF2 Game`，资源在 `GF2_Exilium_Data\LocalCache\Data\AssetBundles_Windows\`
- 窗口驱动 / 截图：`C:\DevCache\gf2\app2.ps1 click <cx> <cy>` / `app2.ps1 shot [-out p.png]`
  （**DPI-aware**：先把窗口摆到 (8,8)；点击坐标 = 截图物理像素 − client 原点。老的 `app.ps1`
  是 DPI-unaware 的，150% 缩放下会错位、窗口比屏幕大时右侧截不到）
- 质量门：`C:\Users\Administrator\.bun\bin\bun.exe run check` → `scripts/check.ps1`

---

## 3. 交付前固定动作

```powershell
& 'C:\Users\Administrator\.bun\bin\bun.exe' run check
```

- [ ] `bun run check` 通过
- [ ] **铁律 13 四关都过了**（反证 / 敏感点 / 前提 / 用户视角；做法见 §8），回答里写出反证结果
- [ ] 有可见变更 → 截图（`app2.ps1 shot`），并说明截图对应哪次改动
- [ ] `docs/ledger.md` 已更新（结论 / 证据 / 未决问题）
- [ ] 本轮新判据已进 `.agents/skills/`，或明确说明无需
- [ ] 改动过的仓库都 commit + push
- [ ] 回答末尾一行「沉淀」声明
- [ ] 任务收尾发微信推送（见 `agents-ui-spec.md` 的用户偏好）

---

## 4. 质量门 `bun run check`（`scripts/check.ps1`）

依次执行，任一步失败即整体失败：

1. `npx vue-tsc --noEmit`（前端类型）
2. `cargo check`（`src-tauri/`，找不到 cargo 才跳过）——外壳是 Rust，编译坏了不能等 `tauri dev` 才发现
3. `<核心 python> -m compileall -q core/exmt_core`（核心语法）
4. `python -m exmt_core meshes <样例 bundle>`（核心冒烟，断言输出含 `c_Vepley`；
   样例 `C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig` 不在则跳过）
5. `vitepress build docs`（若 `C:\Dev\EXMTDoc` 存在）

任何一步失败都不许说"完成"。

---

## 5. 别造轮子：已经有的轮子

| 需求 | 用什么 | 别做什么 |
| --- | --- | --- |
| 读 Unity 资产对象名（贴图等） | UnityPy 的 `o.peek_name()` | `read_typetree()` 全解析（实测 15 分钟 → 41 秒） |
| 解码 BC1/BC7 贴图 | `texture2ddecoder` | 手写 BC 解码 |
| 贴图编码成目标格式（含 mips） | 内置 `EXMTBlender/bin/texconv.exe`（DirectXTex 2026.5.8.1） | 手写 BC7 编码器 |
| 3D 预览交互 | three.js；`enableDamping` 逐帧 `controls.update()`，滚轮自己按 `exp(-delta/400)` 步进并关掉 `enableZoom` | 自研相机数学 |
| Blender 插件 UI 无窗口验证 | 假 layout 真调 `draw_buttons()` | 靠肉眼 / 只跑 `py_compile` |
| 加载/保存 bundle、网格、贴图 | `core/exmt_core/*` 现有命令与函数 | 在脚本里重写一遍解析 |
| 判断「哪些包里有动作」 | 只解元数据里的 type list（class id 74）——9613 个包 63 秒 | 把截断切片喂 `UnityPy.load`（会被判成 0 个对象或抛 EOFError） |
| 动作的骨骼层级 / 真名字 | 同包 prefab 的 `Transform.m_Father` + `GameObject.m_Name`，路径**按后缀**哈希 | 指望 clip 自带层级（它只有 `crc32(路径)`） |
| Blender 5.2 写动作 | `slots.new` + `layers.new` + `strips.new(type="KEYFRAME")` + `channelbag(slot).fcurves.new`，同时设 `action_slot` | `action.fcurves`（4.4 起已移除） |

---

## 6. 写回半径：新知识放哪

| 复用半径 | 写哪 | 例 |
| --- | --- | --- |
| **换目标也用得上（第一优先级）** | `.agents/skills/<skill>/references/*.md`（新领域新建 skill） | `peek_name()` 批量提贴图；UnityFS 只加密前 32KB；指令文件分层法 |
| 只对本项目有意义 | `docs/<目标>-dossier.md` 或 `docs/ledger.md` | 维普蕾的 bundle 哈希、哪个槽几根骨 |
| 用户纠正 / 协作方式 | `docs/agents-corrections.md`，再把结论折成规则进 `AGENTS.md` / `docs/agents-ui-spec.md` | 不要多余按钮、不要日志面板、布局要 WinUI |
| 目标状态 / 阻塞 / 未决 | `docs/ledger.md` | G13 动作导出进行中 |

---

## 7. 常用命令速查

```powershell
$bun = 'C:\Users\Administrator\.bun\bin\bun.exe'
& $bun run tauri dev                   # 跑应用（1420 上已有 Vite 会自动复用）
node scripts/dev.mjs                  # 只起前端（同样复用已有 Vite；--force 用 bun run dev:force）
& $bun run check                       # 质量门
cd core; & $py -m exmt_core <cmd>      # 核心 CLI（cwd 必须在 core）
cd core; & $py -m exmt_core analyze <游戏bundles> <缓存目录> 8   # 重建 GameCache（≈4 分钟）
cd core; & $py -m exmt_core cache <缓存目录>                     # 只查状态，不扫
C:\DevCache\gf2\app2.ps1 shot -out p.png
C:\DevCache\gf2\app2.ps1 click <cx> <cy>
& 'C:\Program Files\Blender Foundation\Blender 5.2\blender.exe' --background --factory-startup --python x.py
```

---

## 7.1 游戏缓存与启动流程（2026-10-20 起）

**数据流**：`<缓存目录>\GameCache\` 是唯一的"游戏知识"来源；主页/模型页/动作栏都只读它，
不再各自扫包（以前模型页第一次要 40–60 s、动作栏 63 s）。

| 文件 | 内容 | 谁写 |
| --- | --- | --- |
| `manifest.json` | format / built_at / bundle_dir / 计数 / 耗时 | `analyze`，**最后写**（存在 = 完整） |
| `names.json` | 网格名 → bundle 列表，贴图名 → bundle 列表 | 同上（`meshidx.build`） |
| `skins.json` | 皮肤 → 部件（每部件最低 lod 的网格，同 lod 优先非移动版）+ 贴图/动作计数 | 同上 |
| `clips.json` | 39 974 个 AnimationClip 名 → bundle | 同上（`animidx.build`） |
| `bones.json` | 50 005 条骨骼路径（crc32 → 路径） | 同上（`bonenames.build`） |
| `skins/<key>/` | **每个皮肤已解好的** meshes / geometry / thumbs / diffuse / meta | 同上（`skinpack.build`，8 进程并行） |
| `progress.json` | 当前阶段/已完成阶段/耗时 + `live:{done,total}`（解皮肤进度） | `analyze`，边跑边写 |

**关键不变量**：`skins/<key>/meta.json` 的 `cache_built_at` **必须等于** `manifest.json` 的 `built_at`。
整个 `analyze` 只取一次时间戳，manifest 和所有包共用；前端只认"两者相等"的包。
这条一旦写错（各自取时间），预解出来的包会被判定过期，又会退回现场解——2026-10-20 就是这么翻车的。

**启动三态**（`src/lib/boot.ts`）：`loading → ready | setup | error`。
`ready` 的条件：setup 标记 + 游戏路径可用 + `cacheState() === "ready"`（manifest 能解析、
format 对、`bundle_dir` 与当前游戏目录一致、三个数据文件在）。不满足就进向导，
向导第 4 步分析完才能进主界面。

**改这些地方会互相影响**：

- 改缓存目录 / 游戏目录 → 旧缓存立即失效（`elsewhere`）；设置页两个入口都已接 `boot()`。
- 改了 `analyze.py` 的产物格式 → 把 `FORMAT` 加版本，否则旧缓存会被当成 `outdated`。
- 改了**皮肤推导规则**（lod / 部件名）不用重扫全库：`cd core; & $py -m exmt_core packs <缓存目录> 8`
  复用 `names.json` 重算 `skins.json` 并重建全部皮肤包（约 150 s），manifest 时间戳不变。
- 只解一个皮肤：`& $py -m exmt_core skinpack <spec.json>`（spec 由前端在缺包时写，也可手写）。

**验证手法（可复用）**：

1. `Move-Item GameCache\manifest.json ...keep` → 启动必须进向导（模拟"没分析过"）。
2. 分析中途看 `progress.json`（阶段名 + 秒数），完成看 `manifest.json` 的 `built_at`。
3. 想证明"没再扫包"：在点进模型页的同时用 250 ms 轮询
   `Get-CimInstance Win32_Process -Filter "name='python.exe'"`，过滤 `exmt_core`，
   **应当一条都没有**（只有 prefetch 那次会有 `skinfo/geoset/thumbs/maps`）。
4. 关掉所有 `exmt-modbox` 再截图：**两个窗口叠在一起时，点击和截图会落在旧窗口上**（见 corrections）。

---

## 8. 收尾自检（铁律 13 的**执行版**，四关缺一不可）

> 本轮（动作第二轮）翻车的样子：首帧挂动作 = 0 顶点移动，看起来完美，用户一摆姿势整团扭曲。
> 教训不是"再仔细点"，而是**验收指标选错了**：首帧≈绑定姿势，任何误差在那里都抵消。
> 所以规则必须是可操作的：**每条结论都要能回答"这条检查能抓住哪种坏"。**

### 8.1 反证（最重要，5 分钟）

1. 写下这条检查"能抓住哪种坏"（一句：例如"骨架 rest 和游戏绑定不一致"）。
2. **把那个坏故意造一次**：回退修复（`git stash` 那一段 / 手动改回旧算式）、换一个输入、
   把姿势换到中间帧、把文件改坏一行。
3. 检查必须**变红**（数字变差 / 截图变坏）。变不红 = 这条检查对它是瞎子，**作废换一条**。
4. 回答里写清反证结果，例如："回退为按 rest 算 basis 后，边缘拉伸 p95 从 2.4 mm 变成 40+ mm，"
   "说明这条验收确实能抓住 rest 不匹配"。

### 8.2 敏感点：按产物类型选验收指标

| 产物 | 指标（必须对这个坏敏感） | 不许用的"假验收" |
| --- | --- | --- |
| 蒙皮 / 动作 | 摆到 **frame_end//2**，看**边缘拉伸的绝对毫米**（1.7 m 人体：中位 <0.5 mm、p95 <5 mm 算过）+ 渲染图 | "首帧 vs 不挂动作 0 变化"（首帧≈绑定姿势，误差全抵消）、"顶点动没动" |
| 网格 / 贴图往返 | 打包回 bundle 再解出来**逐位相等**（或角度分位数） | "没报错""数组长度一样" |
| UI | 真窗口截图（`app2.ps1 shot`），改动处肉眼可见 | "类型检查过了""组件挂载了" |
| 文件 / 配置 | 回读内容 + diff（字节级） | "写成功了" |
| 游戏内 | 进游戏截图 | "Mod 已加载" |

**空结果、零变化、空图一律当失败**（渲染空白 = 先查场景有没有挡住、相机对不对，别当"没东西"）。

### 8.3 前提：把结论依赖的输入假设测一遍

列一条公式/流程依赖的假设，用真实数据各测一次（不是验自己那层自洽）：

- DCC ↔ 游戏坐标/静止姿态：`bone.matrix_local⁻¹ · 游戏rest` 的旋转角 ≈0、位移 ≈0
- 骨架与动作的骨数：动作 `bones/matched` ≈1；不够就先补骨架，别硬套
- 单位：米 / 度 / 秒（欧拉是度、四元数分量序、帧率）
- 计数：`Σ曲线数 == streamed+dense+constant`（这类算术自证最便宜也最硬）

### 8.4 用户视角：用用户的方式再打开一次

headless / 脚本跑通 ≠ 用户能看见。交付前必做：

```powershell
# Blender：默认场景 → 导入工作空间 → 调到中间帧 → 渲染
& 'C:\Program Files\Blender Foundation\Blender 5.2\blender.exe' --background --factory-startup --python <check>.py
```
- 脚本里先清场（`--factory-startup` 自带 2 m Cube，会挡在镜头前被误判成"模型坏了"）
- 相机按**变形后**的包围盒取景，正交相机 `ortho_scale` = 竖版图的高度
- 把图贴进回答（`read_image`），自己先看一眼像不像人/像不像那个东西
