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

**量尺寸前先修 DPI 感知**：pwsh 是 DPI 不感知进程，`GetWindowRect` 给的是**虚拟化**坐标（150% 屏上比真实小 1.5 倍）。
量物理像素要先在**当前线程**开感知（不需要改宿主程序）：

```powershell
[DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr ctx);
SetThreadDpiAwarenessContext((IntPtr)(-4));   // PER_MONITOR_AWARE_V2，之后再 GetWindowRect / CopyFromScreen
```

`SetForegroundWindow` 之后 `Sleep(400)` 再抓图；整屏抓 `0,0,2560,1600` 能同时看到窗口四周留白，比只抓窗口更能说明"大小合不合适"。

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

## 8. 窗口初始尺寸：按工作区比例算，别写死

`tauri.conf.json` 里的 `width/height` 是**绝对逻辑像素**——同一份 1120×700，在 1366×768 笔记本上几乎占满，
在 2560×1600 上又显小。用户要的是"按分辨率算一个不大不小的尺寸"。

判据（放 Rust `setup` 里，不放 config）：

```rust
// Monitor::work_area() 是物理像素，除以 scale_factor 换成逻辑像素（= 用户看到的点数）
let scale = monitor.scale_factor();
let (work_w, work_h) = (area.size.width as f64 / scale, area.size.height as f64 / scale);

let mut w = work_w * 0.72;          // 宽度取工作区的 72%
let mut h = w / (16.0 / 10.0);      // 宽屏比例；超宽屏时这条会被下面截断
if h > work_h * 0.88 { h = work_h * 0.88; w = h * (16.0 / 10.0); }   // 先让高，别顶到任务栏
if w < 920.0 { w = 920.0; h = w / (16.0 / 10.0); }                   // 下限跟 config 的
if h < 600.0 { h = 600.0; w = h * (16.0 / 10.0); }                   // minWidth/minHeight 同步
window.set_size(LogicalSize::new(w.min(work_w).round(), h.min(work_h).round()));
```

三个必须知道的点：

1. **`set_size` 设的是客户区**（tao inner size）。无边框窗口外面还有一圈不可见 resize 边框（本机 150% 下 22×13 px），
   所以居中要用 `window.outer_size()`，拿 `width*scale` 当外框会让窗口左右各偏 ~11px。
2. **居中要贴工作区**（`work_area.position + (work_area.size - outer)/2`），不要 `center: true` 或整屏居中——
   任务栏会把窗口压到底边，多屏时也会跑到错的屏幕。
3. **在 `setup` 里改，用户看不到跳变**：config 窗口在 setup 之前创建但还没进消息循环，第一帧就是新尺寸。
   保持 config 的 `width/height` 当兜底（拿不到 monitor 时用）。

实测曲线（16:10，工作区已是扣掉任务栏的逻辑尺寸）：

| 屏幕 | 工作区 | 窗口 |
| --- | --- | --- |
| 1366×768 @100% | 1366×728 | 984×615 |
| 1920×1080 @100% | 1920×1040 | 1382×864 |
| 2560×1600 @150% | 1707×1019 | 1229×768 |
| 3440×1440 @100% | 3440×1400 | 1232 高封顶 → 1971×1232 |

要换成 16:9 只改一个常量；要更"小一号"就调 `0.72`，别再去 config 写死数字。

## 9. 双语界面 + 用户可配置目录

### 9.1 i18n：类型化字典 > JSON 消息

单窗口工具没必要上 vue-i18n：`Record<keyof typeof zh, string>` 能让**漏翻直接编译失败**，JSON 消息只会静默回落到 key。

```ts
const zh = { "nav.home": "首页", … } as const;
export type MessageKey = keyof typeof zh;
const en: Record<MessageKey, string> = { … };          // 少一个 key → TS 报错
export const locale = ref<Locale>(readLocale());        // localStorage，默认 zh-CN
export function t(key: MessageKey, params?: Record<string, string | number>) {
  let s: string = dictionaries[locale.value][key];
  for (const [k, v] of params ? Object.entries(params) : []) s = s.split(`{${k}}`).join(String(v));
  return s;
}
```

约束（踩过的）：

- **模板里不许写死中文**；`t()` 读 `locale.value`，切换即重渲染。数组型数据（features/tools）存 **key** 不存文案，否则切换语言不刷新。
- `String.replaceAll` 在 ES2020 lib 下不存在 → 用 `split/join`。
- 语言名保持各自语言（简体中文 / English），别翻译成 "Chinese (Simplified)"。
- 代码里拼好的消息（如 `已还原 ${hash}`）要在**动作发生时**调用 `t()`，不要存成已翻译字符串。
- 重启验证：改一个 `src-tauri` 文件的 mtime 就能让 `tauri dev` 自动重建重启，**localStorage 会保留**，是验证持久化最省事的方式。

### 9.2 用户可配置目录（Tauri 2）

打开文件夹不是 `open-url` 能干的事：

1. capabilities 里必须有 `opener:allow-open-path`，并给 path scope（`{"path": "**"}` 即可全放行；只写 `allow-open-url` 会报 "not allowed"）。
2. JS 用 `openPath(dir)`；`revealItemInDir(file)` 是"在资源管理器里选中"，给目录时行为不同。
3. **先建后存**：`useCacheFolder()` 先 `exists`/`mkdir(recursive)` 成功才写 localStorage，否则用户选到只读盘会留下一个永远打不开的路径。
4. 打包临时文件写到这个目录（`join(await cacheDir(), name)`），别再用 `%APPDATA%`——几百 MB 的 bundle 用户想放快盘。
5. 非管理员机器写 `C:\` 根目录会被拒绝，UI 要红字提示而不是静默失败。

### 9.3 验证 UI 时别用会改窗口的脚本

`app.ps1 shot` 里的 `Place()` 会 `SetWindowPos` 到 1680×1050，量到的尺寸/布局就不是打开时的样子。
自己做点击/截图脚本时：`SetThreadDpiAwarenessContext(-4)` → `ClientToScreen(css×scale)` → `SetCursorPos` + `mouse_event`，**不动窗口**；
原生文件夹对话框用 `WM_COMMAND + IDCANCEL(2)` 关闭，`FindWindow` 有时找不到（用 EnumWindows 按标题抓句柄）。

## 10. 把游戏资源做成 Blender 工作空间（Unity → three.js → .exmtmesh）

这一节是"桌面工具里预览 + 导出游戏资产"的可复用套路，跟具体游戏无关。

### 10.1 先分清"资源在哪个包"

Unity 的 AssetBundle 常常把**网格、贴图、材质拆到不同 bundle**，网格包里的 PPtr 还引用着没下载的 material 包。
所以别写"从网格包读贴图"这种代码，先抽样统计：

```python
b = open_bundle(path)
types = {}
for o in b.objects(): types[o.type.name] = types.get(o.type.name, 0) + 1
# {'Mesh': 31, 'AssetBundle': 1}  → 贴图不在这
```

结论：**按资源名前缀建反查索引**才是稳的（贴图名 `c_<Skin>_…` 与网格名同前缀）。

### 10.2 索引要快，只解"第一层"

贴图包的 SerializedFile（node0）很小；mesh 包很大。先读 64 KiB 头、解 blocksInfo，**node0 > 512 KiB 直接跳过**，
再按 block 表读够 node0 就停，`.resS` 一个字节都不用碰。14 个进程扫 9 613 个包 = **9.4 s**（单进程 445 s）。

```python
off, size, flags, cab = info.nodes[0]
if flags != 4 or size > 512 * 1024: return []
# 只解到 off+size，别解整个文件
```

> 另一个坑：`peek_name()` 比 `read_typetree()` 快一个数量级，批量取名字只用前者。

### 10.3 预览数据怎么传

- 几何：`positions/normals/uvs` 用 base64(float32)、索引 base64(uint32) 走一次 JSON。18 k 顶点 ≈ 1.1 MB，够快。
- 坐标：Unity 左手系 → three.js 右手系，**在核心层镜像 z 并翻转三角形绕序**，前端什么都不用改。
- 贴图：核心直接返回 base64 PNG（缩略图 160 px），`<img :src="'data:image/png;base64,'+png">`，省掉自定义协议和临时目录。
- 相机：按 `bounds` 的**最大边**算距离 `d = maxDim/2/tan(fov/2)*1.3`，别用包围球半径硬套，否则模型不是被裁就是缩成一点。

### 10.4 导出成"工作空间"而不是一堆文件

```
<cache>\workspace\<皮肤>\meshes\*.exmtmesh
<cache>\workspace\<皮肤>\textures\*.png
<cache>\workspace\<皮肤>\workspace.json     # 清单：文件、网格名、来源 bundle
<cache>\workspace\current.json              # 指针：{"dir": …}
```

Blender 端只读 `current.json` → 目录 → `workspace.json`，就能"一键导入当前工作空间"，用户不用记路径。
导入时注意：

- **骨架只留一副**：每个网格单独导入会各建一副重叠骨架；后续网格改绑到第一副，删掉新骨架。
- 材质按名字配：`c_X_slg_cloth1_d` 对应网格 `c_X_slg_cloth_lod0`（part token 匹配），`_d/_n` 分别接 BaseColor/Normal。
- 纹理色彩空间：`_d` = sRGB，`_n`/`_rmo` = Non-Color，否则法线会发灰。

### 10.5 两个"看起来像 bug"的坑

1. **索引里存的文件名带 `.bundle`**，拼路径前必须 `replace(/\.bundle$/i, "")`，否则变成 `x.bundle.bundle`，所有贴图静默失败。
2. 失败信息别被"空状态"覆盖：先记下第一个错误，`empty` 文案只在**没有错误**时显示，否则永远查不到原因。

## 8. `tauri dev` 会莫名其妙退出（Vite watcher EBUSY）

**现象**：`dev2.log` 反复出现

```
file watcher error: EBUSY: resource busy or locked, watch
  'C:\Dev\EXMT\src\views\.ModelsView.vue.7464.<uuid>.tmpdir\ModelsView.vue.tmp'
```

随后 `error: script "dev" exited with code 255` → `Error The "beforeDevCommand" terminated with a non-zero status code`，
`bun run tauri dev` 整个退出（窗口消失），但 Rust 进程可能还在。

**根因**：编辑工具用「写临时文件 → 改名」的方式保存 `.vue`，Vite(chokidar) 正在 watch 那个临时目录，
文件已改名/删除 → Windows 上句柄仍被占用 → 抛 EBUSY。偶发多次后 dev server 进程崩溃。

**处理**：
1. 看到 `beforeDevCommand terminated` 就是**前端 dev server 死了**，不是 Rust 编译失败；看日志末尾 20 行确认。
2. 直接重启：`bun run tauri dev > C:\DevCache\tauri-logs\dev3.log 2>&1`（后台 job），约 10–35 秒出窗口。
3. 想减少复发：在 `vite.config.ts` 的 `server.watch.ignored` 里加上 `'**/.*.tmpdir/**'`、`'**/*.tmp'`。
4. 判断窗口是否真的起来了：`Get-Process exmt-modbox | Select MainWindowTitle`，不要只看 job 状态。

**教训**：后台 job 显示 `completed` 不等于"应用在跑"；Tauri dev 的 job 结束 = 窗口没了。