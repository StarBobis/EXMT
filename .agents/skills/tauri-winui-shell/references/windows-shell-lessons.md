# Windows 桌面外壳 · 实战记录（EXMT Modbox）

> 从"网页"变成"像样子的 Windows 应用"时真正踩到的东西。都是可复用的判据，不是流水账。

## 1. 截图 / 点击自动化（验证 UI 的唯一可靠手段）

```powershell
# C:\DevCache\gf2\app.ps1
Place(h):
  ShowWindow(h, SW_RESTORE)
  SetWindowPos(h, HWND_TOPMOST,  40, 40, 1680, 1050, SWP_SHOWWINDOW)   # -1
  SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0,   SWP_NOSIZE|SWP_SHOWWINDOW)  # -2
  SetForegroundWindow(h); Sleep(500)
Shot(h): GetWindowRect → CopyFromScreen
Click(h, cx, cy): ClientToScreen(o) → SetCursorPos(o+c) → mouse_event down/up
```

**为什么必须 TOPMOST 一轮**：`PrintWindow` 对 WebView2 窗口在被遮挡时返回**上一次的帧**，
你会以为改样式没生效（实际生效了）。`CopyFromScreen` + 强制置顶才是真实画面。

**坐标**：这是**客户区物理像素**。窗口 1120 CSS × DPI 150% = 1680 px，所以 `click 30 145` 命中的是 CSS (20, 97)。
先截一张图，量出目标在**图片里的像素**，减去窗口原点，就是 click 的入参——不要拿 CSS 坐标直接点。

## 2. 布局：两个"看起来是 bug，其实是 CSS"的坑

| 现象 | 根因 | 修法 |
| --- | --- | --- |
| 内容整体右移、右侧被窗口切掉 | `.page { max-width: 1080; margin: 0 auto }` | full-bleed：`max-width: none; margin: 0` |
| 两栏（7:3）右栏跑到屏幕外 | `grid-template-columns: 7fr 3fr` 被左侧 min-content 顶宽 | `minmax(0, 7fr) minmax(0, 3fr)` + 子项 `min-width: 0` |
| 卡片一排放不下、最后一张被切 | `repeat(auto-fill, minmax(96px, 124px))` 的**上限**比可用宽度大 | 用 `minmax(96px, 1fr)`，让轨道按容器均分 |

一句话：**任何 `1fr` / 百分比栅格，都要假设子元素会反抗，用 `minmax(0, …)` 兜住。**

## 3. 焦点框

WebView2 对鼠标点击也会 `:focus-visible`。全局 `outline: 2px solid accent` 会在导航项、图标按钮上画蓝框，
和 WinUI 完全不搭。规则：

```css
button:focus, button:focus-visible { outline: none; }
button:focus-visible { background: var(--fill-subtle-hover); }   /* 只有键盘能看到 */
```

## 4. 折叠侧栏的对齐

- 展开 272 / 折叠 48；`☰` 与导航图标**同尺寸盒子**（40×40）+ `margin: 2px auto`。
- 折叠时文字用 `width: 0; opacity: 0`，不要 `display:none`（会重排跳动）。
- 过渡 `cubic-bezier(0.2, 0, 0, 1)` 160ms。
- 折叠状态写 `localStorage`，`App.vue` 挂载时读回。

## 5. 外链与版本号

- 外链用 `@tauri-apps/plugin-opener` 的 `openUrl()`；浏览器预览下 catch 后回落 `window.open`。
- 版本号 `getVersion()`（`@tauri-apps/api/app`）+ `getTauriVersion()`；浏览器里返回占位 `—`。
- 把链接集中成一个 `LINKS` 常量，换正式地址只改一处。

## 6. 前端 ↔ Python 核心的桥

- Rust：`Command::new(python).args(["-m", "exmt_core", ...]).current_dir(core)`，把 stdout/stderr/code 原样回传。
- 打印进度用「一行一个 JSON」：`print(json.dumps({...}), flush=True)`，前端取最后一行 `JSON.parse`。
- 大文件路径通过 `std::fs::copy` 在 Rust 侧完成，别让前端传 base64。

## 7. 已知的"别做"

- 别用 `pwsh` 的 `Set-Content/Out-File` 改 `.vue`/`.ts`/`.md`：会带 BOM 或转 GBK，Vue 编译器直接报错。
  要写就 `[IO.File]::WriteAllText($p,$t,[Text.UTF8Encoding]::new($false))`。
- 别把 `Get-Process | Select -First 1` 当成"那个应用"：Tauri dev 会有多个进程，认 `MainWindowTitle`。
- 别在窗口被遮挡时截图，然后下结论说样式没生效。
