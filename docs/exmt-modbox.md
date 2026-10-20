# EXMT Modbox —— 台账

> 目标：把 EXMT 从"文档仓库"变成可交付的 Tauri 桌面应用（Mod 工具箱）。
> 当前阶段：**只做外壳**——WinUI 3 风格侧边导航 + 自定义标题栏 + 明暗主题 + 放开权限；业务功能未接入。

## 1. 技术选型（已落地）

| 层 | 选型 | 版本（实测） |
| --- | --- | --- |
| 前端 | Vue + TypeScript + Vite | vue 3.5.44 / ts 6.0.3 / vite 8.3.4 |
| 外壳 | Tauri 2 + Rust | tauri 2.12.4 / rustc 1.99.1 |
| 包管理 | Bun | 1.4.3 |
| 插件 | opener / fs / http / dialog | 2.7.2 / 2.7.1 / 2.8.3 / 2.8.3 |
| 工具链 | MSVC | VS BuildTools 2022 17.14.42，MSVC 14.44.35207 |

**为什么不用 `window-vibrancy` 做真 Mica**：需要 `transparent: true`，在 Win10 与部分驱动上会出现黑底/穿透问题；
改用 CSS 双层径向渐变模拟，视觉接近且零平台风险。

## 2. 目录与职责

```
src/App.vue               外壳：TitleBar + SideBar + <component :is>
src/components/TitleBar.vue  48px 高；拖拽区 + 三个 caption 按钮（46×32，贴顶 32px）
src/components/SideBar.vue   272px；选中项左侧 3×16 圆角胶囊 + SubtleFill 背景
src/views/*.vue              四个导航页（首页/模型/资源库/设置）+ 首次引导页（SetupView）
src/lib/nav.ts               导航唯一数据源
src/lib/theme.ts             主题：localStorage + prefers-color-scheme
src/lib/tauri.ts             isTauri 守卫，浏览器里也能跑（截图/预览用）
src/styles/tokens.css        Fluent 令牌：文本 89.56%/60.63%/44.58%，描边 5.78%
src/styles/app.css           布局与组件样式
src-tauri/capabilities/default.json  权限（见 §4）
```

## 3. 窗口与标题栏（实测结论）

- `tauri.conf.json`：`decorations: false`、`shadow: true`、`transparent: false`，1120×700，最小 920×600。
- 拖拽：`data-tauri-drag-region` **只对带该属性的元素本身生效**；子元素（图标 SVG、标题文本）必须
  `pointer-events: none`，否则点在图标上不会拖。
- 双击最大化：Tauri 内核自己处理（`internal_toggle_maximize`），**前端不要再绑 dblclick**，否则两次切换相互抵消。
- 最大化/还原图标切换：监听 `onResized` 后 `isMaximized()`。
- 实测：拖拽 (±120, ±96) 窗口位置同步变化；最大化后窗口 2582×1550（物理，含 11px 阴影边框），
  与 2560×1600 屏幕的工作区一致。

## 4. 权限（用户明确要求：任意 URL + 任意文件夹读写）

`capabilities/default.json` 关键项：

```jsonc
{ "identifier": "fs:scope", "allow": [{ "path": "**" }, { "path": "**/*" }, { "path": "*" }] },
{ "identifier": "http:default", "allow": [
    { "url": "http://*" }, { "url": "https://*" },
    { "url": "http://**" }, { "url": "https://**" },
    { "url": "http://*:*" }, { "url": "https://*:*" },
    { "url": "http://*:*/*" }, { "url": "https://*:*/*" }
]}
```

**实测证据**（临时探针，验证后已删除）：

```
fs:write C:/DevCache -> ok
fs:read C:/Windows/win.ini -> ok (92 chars)
http:https://example.com -> 200
http:http://example.com -> 200
http:http://localhost:1420 -> 200
http:http://127.0.0.1:1420 -> 连接失败（Vite 只监听 ::1，非权限问题）
```

**坑（重要）**：`http://**` **不匹配带非默认端口的 URL**。
http 插件用 URLPattern 语义解析 scope：不带 port 的模式只匹配默认端口，所以必须同时写 `http://*:*`
（以及 `https://*:*`）才能覆盖任意端口。IP 地址（含点）用 `*` 即可匹配，不需要额外写法。

## 5. 验证方式（无人工点击）

- WebView2 是 GPU 合成，`PrintWindow` 抓到的是黑图 → 用 `CopyFromScreen` 抓**客户区**。
- 必须让抓图进程 **DPI-aware**（`SetProcessDpiAwarenessContext(-4)`），否则 150% 缩放下坐标与像素
  空间不一致，会抓偏半个窗口。实测屏幕 2560×1600 @150%（逻辑 1707×1067）。
- 点击/拖拽用 `ClientToScreen + SetCursorPos + mouse_event`，坐标用 **CSS 像素**（进程 DPI 不感知时
  OS 会一致地做虚拟化映射）。

## 6. 环境（本机）

Bun 与 Rust 都走了国内镜像：`registry.npmmirror.com`、`mirrors.tuna.tsinghua.edu.cn/rustup`、
cargo 用 `rsproxy.cn`。GitHub release 直连 300s 只下了 10/44 MB，**不建议直连**。

## 7. 未决问题

1. **打包未验证**：`bun run tauri build` 未跑（需要 WiX/NSIS 下载），首次打包可能耗时 10 分钟以上。
2. **窗口圆角**：`decorations: false` 下 Win11 的 DWM 圆角是否生效未确认（截图上接近直角）。
3. **高 DPI 下的窗口尺寸策略**：默认 1120×700 是逻辑像素；在 2560×1600@150% 上偏小，后续可记忆窗口尺寸。
4. **图标**：仍用 Tauri 默认图标，未替换为自制图标（`bun x @tauri-apps/cli icon` 需要一张 1024 PNG）。
5. **CSP**：当前 `csp: null`（放开）。接入外部资源后建议改成白名单。

## 8. 模型模块（本轮新增）

### 8.1 应用侧
- 新导航项 **模型**（`src/views/ModelsView.vue`），带角色预设卡片：
  角色名 / 模型 bundle / 贴图 bundle / 可替换贴图列表。
- 三个动作：**导出模型**（保存 .exmtmesh）、**导入并安装**（选 .exmtmesh → 打包 → 覆盖游戏包，
  首次自动备份到 `%APPDATA%/com.exmt.modbox/backups/`）、**还原原版**。
- 后端新增 Rust 命令 `run_core(python, core_dir, args)`：不经过 shell、带 `CREATE_NO_WINDOW`，
  只运行 `python -m exmt_core`；路径由前端传入。
- 已验证：UI 点“导出模型” -> 原生保存对话框 -> 写出 294 KB 的 .exmtmesh，日志区回显
  `exported c_VepleySSR01_slg_cloth_lod0 -> ... (5345 verts, 126 bones)`。

### 8.2 核心（Python）
- `core/exmt_core/`：bundle / sf / mesh / interchange / retarget / cli
- CLI：`list`、`meshes`、`extract`、`extract-all`、`pack`、`retarget`、`copy-texture`
- 交换格式 `.exmtmesh`（npz）：位置/法线/切线/顶点色/UV/权重/骨骼索引/索引/子网格/绑定矩阵/骨骼哈希

### 8.3 Blender 插件
- `C:\Dev\EXMTBlender`：`mesh_io.py`（导入/导出）+ `__init__.py`（面板与操作符）
- 导入会建 126 根骨骼的 Armature 与 `b0...b125` 顶点组；导出按顶点组解析骨骼索引
- 偏好设置可改核心目录与 Python 解释器；也能在 `--background` 下直接 import 使用

### 8.4 详细流水线
见 `docs/exmt-model-pipeline.md`。
