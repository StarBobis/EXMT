---
name: tauri-winui-shell
description: Build a Windows-first Tauri 2 desktop shell with a WinUI 3 / Fluent look — custom title bar (decorations off), NavigationView-style sidebar, light/dark tokens — and configure capabilities so the app can read and write any path and call any http(s) URL. Use when scaffolding a Tauri desktop app on Windows, when a custom title bar will not drag or maximize, when a WinUI-style sidebar needs expand/collapse, or when fs/http plugin calls are rejected by the configured scope.
metadata:
  version: "2"
---

# Tauri 2 + WinUI 3 外壳

面向"把已有项目变成 Windows 桌面应用，界面要像 WinUI 3"的任务。目标是产出一个**看起来像系统原生**的外壳：
自定义标题栏、NavigationView 侧栏、明暗主题、放开任意路径读写与任意 URL 访问。

具体到本仓库（EXMT Modbox）的实现与踩坑见 `references/windows-shell-lessons.md`。

## 环境（一次到位）

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --accept-source-agreements --accept-package-agreements `
  --override "--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"

# rustup：清华镜像（直连 static.rust-lang.org 在国内常超时）
$env:RUSTUP_DIST_SERVER='https://mirrors.tuna.tsinghua.edu.cn/rustup'

# bun：GitHub release 直连基本下不动，用 npmmirror 的二进制镜像
# https://registry.npmmirror.com/-/binary/bun/bun-v<ver>/bun-windows-x64.zip
```

- cargo 换源：`~/.cargo/config.toml` → `[source.crates-io] replace-with="rsproxy-sparse"` + `registry="sparse+https://rsproxy.cn/index/"`
- npm/bun 换源：`BUN_CONFIG_REGISTRY=https://registry.npmmirror.com`
- 脚手架：`bun create tauri-app <dir> -t vue-ts -m bun --tauri-version 2 -y`，然后把文件挪到仓库根（别留在子目录）
- 首次 `cargo build` 约 5 分钟（≈532 crate）；之后改配置增量 ~20 秒

## 自定义标题栏（`decorations: false`）

- `tauri.conf.json`：`"decorations": false, "shadow": true, "transparent": false`
- 拖动：`data-tauri-drag-region` **只对该元素本身生效**。子元素（logo、标题文字、图标）必须 `pointer-events: none`，否则点在图标上不拖。
- 双击最大化：Tauri 内核已处理（`data-tauri-drag-region` 上双击）；窗口按钮走 `appWindow.minimize() / toggleMaximize() / close()`。
- 标题栏高度 36–40px，左右各留 12px；按钮 46×32，`hover` 用 `#e9e9e9 / #2d2d2d`。

## NavigationView 侧栏（展开 / 折叠）

- 两档宽度：展开 **272px**、折叠 **48px**（WinUI 的 compact pane 就是 48）。
- `☰`（Segoe MDL2 `&#xE700;`）放在侧栏头部，**与导航图标共用同一个 40×40 盒子**、`margin: 2px auto`，否则折叠时会偏。
- 折叠时：图标水平居中、文字标签宽度归零并淡出（`opacity` + `width` 过渡），不要 `display:none`（会跳）。
- 过渡曲线用 `cubic-bezier(0.2, 0, 0, 1)`（Fluent 的 standard easing），时长 150–200ms。
- 折叠状态存 `localStorage`，刷新后保持。
- **选中态**：左侧 3px 强调条 + `--fill-selected` 背景，**不要**蓝框。

## 焦点与点击反馈（最容易做丑的地方）

WebView2 在鼠标点击后会触发 `:focus-visible`，于是按钮上出现系统蓝框，和 WinUI 完全不搭。

```css
.nav-item:focus,
.nav-item:focus-visible { outline: none; }
.nav-item:focus-visible { background: var(--fill-subtle-hover); }
```

规则：**鼠标点选不出现外框**，改用填充表示焦点；键盘用户靠填充变化感知。

## 页面与栅格（宽窗口下的两个经典坑）

1. **不要 `max-width + margin:auto`**。桌面窗口可以很宽，居中会在两侧留死白；窗口不够宽时又把内容顶出屏。
   用 full-bleed：`.page { max-width: none; margin: 0; padding: 18px 24px; }`
2. **栅格用 `minmax(0, Nfr)`**。裸 `1fr` 或 `70% 30%` 会被子元素的 min-content 撑宽，产生横向溢出/右侧被切。
   两栏页面（如设置页 7:3）：`grid-template-columns: minmax(0, 7fr) minmax(0, 3fr);` + `.settings > * { min-width: 0; }`

## 放开任意路径与任意 URL

`src-tauri/capabilities/default.json`：

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "fs:default",
    "fs:allow-read-file",
    "fs:allow-write-file",
    { "identifier": "fs:scope", "allow": [{ "path": "**" }] },
    { "identifier": "http:default", "allow": [{ "url": "http://**" }, { "url": "https://**" }] }
  ]
}
```

- `fs:scope` 允许 `**`（**含盘符**：Windows 上写成 `"**"` 即可，不要写 `"/**"`），配合 Rust 侧命令尤其实用。
- 只用 Rust 命令读写文件时，其实不需要 `fs` 权限；但前端直接调 `@tauri-apps/plugin-fs` 时必须要。
- `http` 插件同理；`fetch` 走插件而不是 WebView 才有跨域豁免。

## 让前端跑 Python 核心（本仓库做法）

Rust 侧一个通用命令：

```rust
#[tauri::command]
async fn run_core(args: Vec<String>) -> Result<CoreOut, String> { /* Command::new(python).args(...) */ }
```

- 用 `python.exe -m <pkg>` + `current_dir(core)`，避免依赖 PATH 里的 `python`。
- stdout/stderr/退出码**原样回传**，前端只负责展示；解析在 Python 侧打印 JSON 行，前端取最后一行 `JSON.parse`。
- 长任务（提取 240 张贴图）要能显示进度：Python 每完成一批 `print(json.dumps({...}), flush=True)`，前端按行读。

## 截图与自动化验证（Windows）

- `PrintWindow` 在窗口被遮挡时返回**旧帧**；正确做法：`SetWindowPos(HWND_TOPMOST)` → `SetWindowPos(HWND_NOTOPMOST)` → `SetForegroundWindow` → `CopyFromScreen`。
- 点击坐标是**客户区物理像素**：`client = CSS × DPI缩放`（150% 时 ×1.5）。
- 不要用 `pwsh` 的 `Set-Content` 改 `.vue`/`.ts`（BOM/GBK 往返会写坏文件）。

## 别截图点按钮：用 CDP 程序化驱动 WebView2

人工截图 → 找坐标 → 点击 → 再截图，一轮 1–2 分钟且坐标错了不自知。WebView2 支持远程调试，
**一次起进程，之后全部用脚本读状态/点 DOM/抓图**：

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
# 必须在同一个进程环境里启动 app；放到长驻后台任务里跑（app 退出任务才结束）
& '.\src-tauri\target\debug\app.exe'
```

之后 `http://127.0.0.1:9222/json` 列出 page target，用 Node 24 自带的全局 `WebSocket` 直连
`webSocketDebuggerUrl`，只发三条命令就够：

| 目的 | CDP 命令 |
| --- | --- |
| 读页面状态/DOM/几何 | `Runtime.evaluate {expression, returnByValue:true, awaitPromise:true}` |
| 点按钮（不用坐标） | 在 evaluate 里 `[...document.querySelectorAll('button')].find(n=>n.innerText.includes('文字')).click()` |
| 截图 | `Page.captureScreenshot {format:'png'}` → base64 写文件 |

要点：

- **点击按可见文字而不是坐标**：布局变了也不会点错；截图里的像素坐标还要减 client 原点、乘 DPI，易错。
- **抓图按 CSS 尺寸换算**：`captureScreenshot` 是设备像素，`devicePixelRatio` 通常 1.25/1.5；
  要算预览区饱和度/颜色就把 `getBoundingClientRect()` 一起取回来，按 `图宽/innerWidth` 缩放。
- **app 必须活着**：把启动命令放长驻后台任务（阻塞式跑 exe），不要 `Start-Process` 后让 shell 退出——
  进程树被回收后 CDP 端口和窗口一起消失。
- 前端改动走 Vite HMR，脚本里 `location.reload()` 后重抓即可，不用重启 app。

