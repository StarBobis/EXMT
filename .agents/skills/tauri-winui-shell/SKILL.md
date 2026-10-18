---
name: tauri-winui-shell
description: Build a Windows-first Tauri 2 desktop shell with a WinUI 3 / Fluent look — custom title bar (decorations off), NavigationView-style sidebar, light/dark tokens — and configure capabilities so the app can read and write any path and call any http(s) URL. Use when scaffolding a Tauri desktop app on Windows, when a custom title bar will not drag or maximize, or when fs/http plugin calls are rejected by the configured scope.
metadata:
  version: "1"
---

# Tauri 2 + WinUI 3 外壳

面向"把已有项目变成 Windows 桌面应用，界面要像 WinUI 3"的任务。产出一个可运行外壳：
自定义标题栏、侧边导航、明暗主题、放开任意路径读写与任意 URL 访问。

## 环境（一次到位）

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --accept-source-agreements --accept-package-agreements `
  --override "--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
# rustup（走清华镜像，直连 static.rust-lang.org 在国内常超时）
$env:RUSTUP_DIST_SERVER='https://mirrors.tuna.tsinghua.edu.cn/rustup'
# bun（GitHub release 直连 300s 只下 10/44 MB，用 npmmirror 二进制镜像）
# https://registry.npmmirror.com/-/binary/bun/bun-v<ver>/bun-windows-x64.zip
```

- cargo 换源：`~/.cargo/config.toml` → `[source.crates-io] replace-with="rsproxy-sparse"` + `registry="sparse+https://rsproxy.cn/index/"`
- npm/bun 换源：`BUN_CONFIG_REGISTRY=https://registry.npmmirror.com`
- 脚手架：`bun create tauri-app <dir> -t vue-ts -m bun --tauri-version 2 -y`，再把文件搬到仓库根（别留在子目录）
- 首次 `cargo build` 约 5 分钟、432 个 crate；之后改配置增量 20 秒

## 自定义标题栏（decorations: false）

- `tauri.conf.json`：`"decorations": false, "shadow": true, "transparent": false`
- 拖拽：`data-tauri-drag-region` **只对带该属性的元素本身生效**。子元素（logo SVG、标题文本、徽标）
  必须 `pointer-events: none`，否则点在图标上不拖。
- 双击最大化：Tauri 内核已处理（`internal_toggle_maximize`），**前端不要再绑 dblclick**，否则两次切换互相抵消。
- caption 按钮按 Win11 习惯：46×32、贴顶（只有顶部 32px 响应），close hover `#c42b1c` + 白字。
- 还原图标：`getCurrentWindow().onResized()` → `isMaximized()` 决定显示 E922/E923。
- 必需权限：`core:window:allow-start-dragging`、`core:window:allow-internal-toggle-maximize`、
  `…-minimize`、`…-toggle-maximize`、`…-is-maximized`、`…-close`。

## WinUI 3 设计令牌（照抄，别自己调色）

| 用途 | 亮色 | 暗色 |
| --- | --- | --- |
| 主文本 | `rgba(0,0,0,.8956)` | `rgba(255,255,255,.8956)` |
| 次文本 | `.6063` | `.6063` |
| 三级文本 | `.4458` | `.4458` |
| 描边 | `rgba(0,0,0,.0578)` | `rgba(255,255,255,.0698)` |
| hover / 选中填充 | `.0373` | `.0605` |
| accent | `#0067c0` | `#60cdff` |

- 圆角：控件 4、卡片 8；字体 `Segoe UI Variable Text`，标题用 `Display`。
- 侧边栏：宽 272，item 高 40、左右 8px 外边距、选中项左侧 3×16 圆角胶囊（accent）+ SubtleFill 背景。
- Mica：不要为了"真 Mica"去开 `transparent: true`（Win10/部分驱动会黑底）；用两层径向渐变模拟即可。

## 权限：任意路径 + 任意 URL

```jsonc
{ "identifier": "fs:scope", "allow": [{ "path": "**" }, { "path": "**/*" }, { "path": "*" }] },
{ "identifier": "http:default", "allow": [
  { "url": "http://*" },   { "url": "https://*" },
  { "url": "http://*:*" }, { "url": "https://*:*" },   // 端口必须显式写 *:*
  { "url": "http://*:*/*" }, { "url": "https://*:*/*" }
]}
```

- **最大的坑：`http://**` 不匹配非默认端口**。http 插件用 URLPattern 语义解析 scope，不带 port 的模式
  只匹配默认端口；失败信息是 `url not allowed on the configured scope: http://127.0.0.1:1420/`。
  加上 `http://*:*` 即可覆盖任意端口（IP 里有句点也能被 `*` 匹配）。
- `http:allow-fetch` 可以省略——`http:default` 已含该命令。
- fs 侧还要给命令权限：`fs:allow-read-text-file` / `-write-text-file` / `-read-file` / `-write-file` /
  `-read-dir` / `-mkdir` / `-remove` / `-rename` / `-exists` / `-stat` 等。
- 验证方式：临时在 `main.ts` 里跑探针（写任意目录、读 `C:/Windows/win.ini`、fetch 外网 + localhost），
  把结果写进文件再读，验证完删除探针。

## 无人工点击的验证（Windows）

- WebView2 是 GPU 合成，`PrintWindow` 抓到黑图 → 用 `CopyFromScreen` 抓**客户区**。
- 抓图/点击进程必须 **DPI-aware**：`SetProcessDpiAwarenessContext(-4)`，否则 150% 缩放下坐标与像素
  空间不一致，会抓偏半个窗口（本机屏幕 2560×1600 @150% = 逻辑 1707×1067）。
- 点击/拖拽：`ClientToScreen + SetCursorPos + mouse_event`，坐标传 **CSS 像素**。
- 拖拽验证看 `GetWindowRect` 前后差值是否等于鼠标位移。

## 坑速查

| 现象 | 原因 / 解法 |
| --- | --- |
| 窗口量出来 0×0 | 被 `ShowWindow(SW_RESTORE)` 或最小化；先 `ShowWindow(5)` 再量 |
| 截图抓偏半个窗口 | 进程 DPI 不感知，却用了物理坐标 |
| 拖拽只能拖空白处 | 子元素缺 `pointer-events: none` |
| 双击最大化没反应 | 前端和内核各 toggle 一次，互相抵消 |
| fs 写失败但路径合法 | scope 没给 `**`，或用相对路径（插件解析到 appdata） |
| fetch 本地服务失败但外网正常 | 服务只监听 `::1` 或 `127.0.0.1` 之一，先 `netstat` 确认 |
| `bun create tauri-app` 卡住 | 没设 `BUN_CONFIG_REGISTRY`，还在连 registry.npmjs.org |
