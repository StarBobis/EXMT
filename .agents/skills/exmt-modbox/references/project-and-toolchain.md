# EXMT Modbox · 项目地图、工具链与验收

## 1. 仓库地图

| 位置 | 是什么 | 怎么跑 |
| --- | --- | --- |
| `src/` | 前端（Vue 3 + TS + Vite）；视图 `src/views/`，样式 `src/styles/` | `bun run dev` |
| `src-tauri/` | Rust 外壳（Tauri 2），命令 `run_core` / `show_main_window` | `bun run tauri dev` |
| `core-rs/` + `src-tauri/src/` | Rust 资产核心：容器/序列化/网格/贴图/动画/GameCache；运行时代码 | `cargo test --manifest-path core-rs/Cargo.toml`；命令经前端 `runCore` |
| `C:\Dev\EXMTBlender\` | Blender 插件：导入/导出 `.exmtmesh`、传权重、蓝图、生成 Mod | Blender → 偏好设置 → 安装 `__init__.py` |
| `C:\Dev\EXMTDoc\` | VitePress 2 文档站（独立 git 仓库，无 remote） | `bun run build` / `bun run preview` |
| `.agents/skills/` | 能力：换目标也能用的判据 / 手法 | — |
| `C:\DevCache\<任务名>\` | 一次性脚本、临时产物、截图 | — |

技术栈：Vue 3.5 / TypeScript 6 / Vite 8 / Tauri 2.12 / Rust 1.99 / Bun 1.4 / MSVC 2022。
不用 `window-vibrancy` 做真 Mica：需要 `transparent: true`，Win10 与部分驱动会黑底/穿透；改用 CSS 双层径向渐变模拟。

## 2. 本机工具链

| 工具 | 路径 / 说明 |
| --- | --- |
| Bun | `C:\Users\Administrator\.bun\bin\bun.exe`（**不在 PATH**，一律全路径） |
| Rust 工具链 | `cargo`（`%USERPROFILE%\.cargo\bin\cargo.exe`）；`texconv.exe` 在 `src-tauri/bin/` |
| Blender | `C:\Program Files\Blender Foundation\Blender 5.2\blender.exe`；headless = `--background --factory-startup --python x.py` |
| 游戏 | `C:\GF2Exilium\GF2 Game`，资源在 `GF2_Exilium_Data\LocalCache\Data\AssetBundles_Windows\` |
| 窗口驱动 / 截图 | `C:\DevCache\gf2\app2.ps1 click <cx> <cy>` / `app2.ps1 shot [-out p.png]`（DPI-aware；老的 `app.ps1` 会错位） |
| 质量门 | `bun run check` → `scripts/check.ps1` |

`beforeDevCommand` 是 `node scripts/dev.mjs`：1420 上已有 Vite 就直接复用（打印 `reusing it` 并 exit 0），端口空才自己起，被别的程序占则 exit 1。**不要把残留 dev server 当错误**，但收尾要关掉自己起的。

## 3. 常用命令

```powershell
$bun = 'C:\Users\Administrator\.bun\bin\bun.exe'
& $bun run tauri dev                   # 跑应用（1420 上已有 Vite 会自动复用）
node scripts/dev.mjs                   # 只起前端
& $bun run check                       # 质量门
cargo test --manifest-path core-rs/Cargo.toml          # 资产核心测试
cargo test --manifest-path src-tauri/Cargo.toml        # 外壳/命令测试
# analyze 在应用内跑（设置里选游戏目录 → 重建缓存），全量约 3.5 分钟（debug），可随时停止
C:\DevCache\gf2\app2.ps1 shot -out p.png
C:\DevCache\gf2\app2.ps1 click <cx> <cy>
& 'C:\Program Files\Blender Foundation\Blender 5.2\blender.exe' --background --factory-startup --python x.py
```

## 4. 质量门 `bun run check`

依次执行，任一步失败即整体失败：

1. `npx vue-tsc --noEmit`（前端类型）
2. `cargo check`（`src-tauri/`，找不到 cargo 才跳过）
3. `cargo test`（`core-rs/`，样例缺失时相关测试自动跳过）
4. `vitepress build docs`（`C:\Dev\EXMTDoc` 存在时）

任何一步失败都不许说「完成」。

## 4.1 别造轮子：已有的轮子

| 需求 | 用什么 | 不要做什么 |
| --- | --- | --- |
| 读 Unity 资产对象名（贴图等） | UnityPy 的 `o.peek_name()` | `read_typetree()` 全解析（15 分钟 → 41 秒） |
| 解码 BC1/BC7 贴图 | `texture2ddecoder` | 手写 BC 解码 |
| 贴图编码成目标格式（含 mips） | 内置 `EXMTBlender/bin/texconv.exe`（DirectXTex） | 手写 BC7 编码器 |
| 3D 预览交互 | three.js；`enableDamping` 逐帧 `controls.update()`，滚轮按 `exp(-delta/400)` 步进并关掉 `enableZoom` | 自研相机数学 |
| Blender 插件 UI 无窗口验证 | 假 layout 真调 `draw_buttons()` | 靠肉眼 / 只跑 `py_compile` |
| 加载/保存 bundle、网格、贴图 | `core/exmt_core/*` 现有命令与函数 | 在脚本里重写一遍解析 |
| 判断哪些包里有动作 | 只解元数据里的 type list（class id 74）——9613 包 63 秒 | 把截断切片喂 `UnityPy.load`（会判成 0 个对象或抛 EOFError） |
| 动作的骨骼层级 / 真名字 | 同包 prefab 的 `Transform.m_Father` + `GameObject.m_Name`，路径按后缀哈希 | 指望 clip 自带层级（它只有 `crc32(路径)`） |
| Blender 5.2 写动作 | `slots.new` + `layers.new` + `strips.new(type="KEYFRAME")` + `channelbag(slot).fcurves.new`，同时设 `action_slot` | `action.fcurves`（4.4 起已移除） |

## 5. GameCache 数据流与启动三态

`<缓存目录>\GameCache\` 是唯一的「游戏知识」来源；主页 / 模型页 / 动作栏都只读它，不再各自扫包。

| 文件 | 内容 | 谁写 |
| --- | --- | --- |
| `manifest.json` | format / built_at / bundle_dir / 计数 / 耗时 | `analyze`，最后写（存在 = 完整） |
| `names.json` | 网格名 → bundle 列表，贴图名 → bundle 列表 | `analyze` 扫描阶段 |
| `skins.json` | 皮肤 → 部件 + 贴图/动作计数 | `analyze` |
| `clips.json` | AnimationClip 名 → bundle | `analyze` 扫描阶段 |
| `bones.json` | 骨骼路径（crc32 → 路径） | `analyze` 扫描阶段 |
| `skins/<key>/` | 每个皮肤已解好的 meshes / geometry / thumbs / diffuse / meta | `skinpack.build`，8 线程并行 |
| `progress.json` | 当前阶段 / 已完成阶段 / 耗时 + `live:{done,total}` | `analyze`，边跑边写 |

**阶段只有三个**：`scan`（一次读包里同时收网格/贴图/动作名 + 骨骼路径）、`skins`、`packs`。UI 的 `AnalyzeProgress.vue` 与 i18n 的 `setup.phase.*` 必须与之一致。

**分析只依赖游戏本体，不依赖旧 GameCache**：唯一的输入是 `bundle_dir`，旧缓存只在成功后原子替换。`manifest.json` 的 `bundle_dir` 用 `analyze::display_dir` 写（去掉 Windows 的 `\\?\` verbatim 前缀），前端用 `normalizePath` 比较；两处任缺其一，分析成功也会被误判成 `elsewhere`。

**关键不变量**：`skins/<key>/meta.json` 的 `cache_built_at` 必须等于 `manifest.json` 的 `built_at`。整个 `analyze` 只取一次时间戳，manifest 和所有包共用；前端只认「两者相等」的包。

**启动三态**（`src/lib/boot.ts`）：`loading → ready | setup | error`。`ready` = setup 标记 + 游戏路径可用 + `cacheState() === "ready"`（manifest 可解析、format 对、`bundle_dir` 与当前游戏目录一致、三个数据文件在）。不满足进向导，第 4 步分析完才进主界面。没有游戏路径时 `boot()` 会先自动探测一次。

改这些地方会互相影响：

- 改缓存目录 / 游戏目录 → 旧缓存立即失效（`elsewhere`）；设置页两个入口都接 `boot()`。
- 改 analyze 产物格式 → 给 `analyze::FORMAT` 加版本，否则旧缓存被当成 `outdated`。
- 只解一个皮肤 / 复测 pack 速度：`cargo run --example skinpack_cli -- <spec.json>`。
- 只重扫索引 / 测扫描速度：`cargo run --example analyze_cli -- <bundle_dir> <cache_dir> <limit|0> <workers> [bones]`。

## 6. 验收方法（可复用）

### 6.1 真窗口截图

- WebView2 是 GPU 合成，`PrintWindow` 抓到黑图 → 用 `CopyFromScreen` 抓客户区。
- 抓图进程必须 DPI-aware（`SetProcessDpiAwarenessContext(-4)`），否则 150% 缩放下坐标与像素空间不一致。
- 点击/拖拽用 `ClientToScreen + SetCursorPos + mouse_event`，坐标用 CSS 像素。
- **截图前关掉所有 `exmt-modbox`**：两个窗口叠在一起时，点击和截图会落在旧窗口上。

### 6.2 CDP 驱动（无人工点击）

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
& .\src-tauri\target\debug\exmt-modbox.exe   # 后台作业运行
# Page.navigate / Runtime.evaluate / Page.captureScreenshot
```

- 页面 target 的 URL 是 devUrl（`http://localhost:1420`），不是 `file://`。
- 小 UI 改动优先复用已在运行的 dev 窗口，不要冷启动新的 exe；验证前用 `document.querySelector` 读真实 DOM 宽度/选中态。
- PowerShell 调快照脚本时，**整体表达式用双引号包、内部用单引号**；单引号包整体会被原生参数传递吞掉双引号。

### 6.3 证明「没有在后台扫包」

在点进模型页的同时用 250 ms 轮询 `Get-CimInstance Win32_Process -Filter "name='python.exe'"`，过滤 `exmt_core`，应当一条都没有（只有 prefetch 那次会有 `skinfo/geoset/thumbs/maps`）。

### 6.4 四关自检（交付前必做）

1. **反证**：写下这条检查能抓住哪种坏，把那个坏故意造一次，检查必须变红；变不红就换检查。
2. **敏感指标**：按产物类型选（见下表），不许用假验收。
3. **前提**：把结论依赖的输入假设用真实数据各测一次（坐标/姿态、骨数、单位、计数自洽）。
4. **用户视角**：用用户的方式再打开一次（真窗口 / 渲染图），自己先看一眼像不像。

| 产物 | 必须敏感的指标 | 假验收 |
| --- | --- | --- |
| 蒙皮 / 动作 | 摆到 `frame_end//2`，看边缘拉伸的绝对毫米（1.7 m 人体：中位 <0.5 mm、p95 <5 mm）+ 渲染图 | 「首帧 vs 不挂动作 0 变化」「顶点动没动」 |
| 网格 / 贴图往返 | 打包回 bundle 再解出来逐位相等（或角度分位数） | 「没报错」「数组长度一样」 |
| UI | 真窗口截图，改动处肉眼可见 | 「类型检查过了」「组件挂载了」 |
| 文件 / 配置 | 回读内容 + 字节级 diff | 「写成功了」 |
| 游戏内 | 进游戏截图 | 「Mod 已加载」 |

空结果、零变化、空图一律当失败。

## 7. 本机 Windows 坑

| 现象 | 根因 / 做法 |
| --- | --- |
| `npx.ps1` 无法加载，禁止运行脚本 | 执行策略限制；用 `npx.cmd`，或 `node node_modules/<tool>/...` |
| 原生命令参数里的双引号消失 | PowerShell 重新解析原生命令参数；整体用双引号包、内部用单引号 |
| `vite build` 输出 `✓ built` 却报 exit 1 | stderr 的警告被 PowerShell 当错误；用 `*> $null; $LASTEXITCODE` 取真实退出码 |
| `Start-Process` 长时间无输出/进后台 | 直接 `& exe` 放后台作业，或 `Start-Process` 后立刻查进程与端口 |
| WebView2 `HRESULT(0x8000FFFF) 灾难性故障` | 工作区内 exe 以 Low IL 启动，`%LOCALAPPDATA%\com.exmt.modbox\EBWebView` 建不出来；`icacls <appdata>\com.exmt.modbox /setintegritylevel (OI)(CI)LW`（详见 skill `tauri-winui-shell`） |
| `bun` 不在 PATH | 一律 `C:\Users\Administrator\.bun\bin\bun.exe run <script>` |
| 1420 端口已有 Vite | 正常复用；不要杀掉别人的 dev server，只关自己起的 |
| `PrintWindow` 截图为黑 | WebView2 GPU 合成；改 `CopyFromScreen` 抓客户区 |
| 高 DPI 坐标偏移 | 抓图进程必须 `SetProcessDpiAwarenessContext(-4)` |

## 8. 窗口、标题栏与权限

- `tauri.conf.json`：`decorations: false`、`shadow: true`、`transparent: false`，最小 920×600。
- `data-tauri-drag-region` 只对带该属性的元素本身生效；子元素（图标、标题文本）必须 `pointer-events: none`。
- 双击最大化由 Tauri 内核处理，前端不要再绑 `dblclick`；最大化图标切换监听 `onResized` 后 `isMaximized()`。
- 权限放开了任意路径读写与任意 http(s) URL。**`http://**` 不匹配非默认端口**：必须同时写 `http://*:*` / `https://*:*`；IP 地址用 `*` 即可。
- `capabilities/default.json` 里 `fs:scope` 用 `{ "path": "**" }` / `**/*` / `*` 覆盖任意盘符。
