# 台账 · EXMT Modbox

> 结论 + 证据 + 未决问题。**不写流水账**（不记"先跑 X 再跑 Y"）。
> 每轮开始读这里，结束前更新这里。目标状态只在这里，不在 AGENTS.md。

最近更新：2026-10-19（§2.10 骨骼合并与绑定修复）

---

## 1. 目标

| # | 目标 | 状态 | 验收 |
| --- | --- | --- | --- |
| G1 | Tauri 2 + Vue3 + TS 的 Modbox 外壳（WinUI 风格） | ✅ 完成 | 侧栏可折叠、自定义标题栏、任意路径读写、**窗口初始尺寸按分辨率自适应（见 §2.4）** |
| G2 | 全角色目录 + 立绘 + 卡牌视图 | ✅ 完成 | 78 角色 / 240 皮肤 / 169 立绘，卡片不重复 |
| G3 | `.exmtmesh` 无损往返（导出→Blender→打包） | ✅ 完成 | 不改动直接打包，23 个字段 + 顶点流 + 索引**逐字节一致** |
| G4 | 换衣服（外部服装替换 cloth 槽） | ✅ 可用 | 夹克 / 宿舍装 / 校服 均在游戏内生效 |
| G5 | **独立身体替换（body 槽）** | ❌ 未完成 | 需要绑定空间重定向，见 §4 |
| G6 | VitePress 2 文档站 + 替换模型图文教程 | ✅ 完成 | `bun run build` 通过，9 步带截图 |
| G7 | 导出流程只保留自定义格式 | ✅ 完成 | glTF 代码/命令/按钮全部删除 |
| G8 | 设置页：中/英双语 + 全局缓存文件夹 | ✅ 完成 | 语言切换立即生效并跨重启保持；缓存默认 `C:\EXMTCacheFolder`，可改、可在资源管理器中打开 |
| G9 | 模型工作台：3D 预览 + 贴图墙 + 工作空间导出 | ✅ 完成 | 选立绘卡片 → 只看该皮肤的网格/贴图；three.js 预览可旋转缩放；「开始制作」导出 `<cache>\workspace\<皮肤>\`，Blender 插件一键导入（见 §2.6） |
| G10 | 首页 = 角色列表 + 立绘卡牌；模型页 = 网格/3D/贴图工作台 | ✅ 完成 | 首页 78 角色（2 列头像）+ 卡牌轮播；点卡选中皮肤，「进入模型页」进工作台；模型页顶栏显示当前角色/皮肤/数量，可「换皮肤」回首页；选择持久化（见 §2.7） |
| G11 | **骨骼完整 + 正确绑定**（用户报的两个 bug） | ✅ 完成 | workspace 合成一副完整骨架（Alva 226 根，跨网格并集）；Blender 只建一副骨架、每个网格 100% 绑上（0 未绑定组 / 0 未蒙皮顶点）；不改动往返权重与骨骼索引逐位一致；打包回 bundle 与原包全等（见 §2.10） |
| G12 | **改完 → 生成 Mod → 一键进游戏** | ✅ 完成 | Blender 面板「生成 Mod」把工作空间里所有网格打成一个 Mod 写到 `<工作空间>/GeneratedMod/`；App 模型页「加载到游戏」一键装进游戏（先备份原始包）、「卸下 Mod」一键还原；实测安装/卸下往返，游戏包字节与备份全等（见 §2.11） |

---

## 2. 已确认的关键事实（带证据）

### 2.1 游戏资源

| 事实 | 证据 |
| --- | --- |
| Unity 2019.4.40f1 / IL2CPP / Addressables | `globalgamemanagers` 版本串；`gf2-exilium-recon.md` |
| bundle 是 UnityFS v7，**只加密前 32768 字节** | 重打包后整文件 XOR 会 `Failed to load CAB-`，只加密头部正常 |
| 加密密钥 = `head4 ^ "Unit"` + PAD + 翻转位 + PAD（16 字节循环） | `core/exmt_core/bundle.py` |
| 网格顶点流：s0(40B) pos/normal/tangent，s1(20B) color/uv0(f16)，s2(32B) weights/bones | 通道表实测 |
| `m_IndexBuffer` 是字节数组，`SubMesh.firstByte` 是**字节**偏移 | 索引翻倍 bug 的根因 |
| `body` 槽只有头/手/脚（2283 顶点），躯干在 `cloth`（18194） | 隐藏衣服后大厅只剩头+手 |
| 骨骼只有 CRC32 哈希，**没有名字、没有父子关系** | 网格里只有 `m_BindPose` + `m_BoneNameHashes`；MST 重建层级会得到错误链条 |

### 2.2 立绘 / 皮肤

| 事实 | 证据 |
| --- | --- |
| 皮肤立绘 = `Img_ChrSkinPic_<皮肤码>`，380×752 透明底 | 解码实拍 |
| **默认立绘 = 皮肤码无数字后缀**（`VepleySSR`） | 与 `VepleySSR01` 对比 |
| `Avatar_Gacha_*` 是抽卡长条（256×1600），**不是立绘** | 尺寸 + 画面 |
| 169 张立绘 / 75 角色；240 皮肤里 71 个无独立立绘（多为宿舍装） | `src/data/art.json` 统计 |
| 立绘→模型映射：默认→同稀有度最短名；带数字 D→同稀有度以 D 结尾且未占用 | 黛烟 `SSR03 → DaiyanSSR0103` |

### 2.3 工具链坑

| 坑 | 结论 |
| --- | --- |
| 批量读贴图 | `read_typetree()` 会连 image data 一起解析，240 张要 15 分钟；`peek_name()` 后 41 秒 |
| Blender 导入 | `bpy` 自定义属性存 uint32 会 `OverflowError`，要转 JSON 字符串 |
| 截图 | `PrintWindow` 在窗口被遮挡时返回旧帧；必须 HWND_TOPMOST→NOTOPMOST + `CopyFromScreen` |
| pwsh 改文本 | `Set-Content` 会写 BOM/GBK，`.vue` 直接损坏 |
| `tauri dev` 突然退出 | 编辑工具的临时文件触发 Vite watcher `EBUSY`，`bun run dev` 以 255 退出 → `beforeDevCommand terminated`；重启即可，见 skill `windows-shell-lessons.md` §8 |

### 2.4 窗口尺寸 / DPI

| 事实 | 证据 |
| --- | --- |
| 本机屏 2560×1600 @150%：物理工作区 2560×1528，逻辑工作区 1707×1019 | `GetDpiForWindow`=144；pwsh 读 `Screen.WorkingArea`（虚拟化后）1707×1019 |
| 默认窗口 = **16:9（1920×1080 的比例）+ 工作区宽 2/3**，高 = 宽×9/16，上限高 90%，下限 920×600 → 本机客户区 1707×960 物理（1138×640 逻辑），外框 1729×973 居中于工作区（偏差 0.5 px） | 4K/2K/1366 四档单测；实测 `GetWindowRect` 416,278 1729×973 / client 1707×960，`center dx/dy = -0.5` |
| `set_size(LogicalSize)` 设的是**客户区**（tao inner size），无边框窗口外面还有 ~22×13 物理像素不可见边框 | 外框 1729×973 − 客户区 1707×960 = 22×13 |
| 居中要用 `outer_size()`，用客户区居中会左右各差 11px | 旧写法 (358,188) → 新写法 (347,182) 左右对称 |
| pwsh 是 DPI 不感知进程，`GetWindowRect` / `SetCursorPos` 都返回**虚拟化**坐标；截图与点击前必须 `SetProcessDPIAware()`，否则点哪都偏 1.5 倍 | 未声明 DPI 时点击模型页导航无效、截图抓错窗口；声明后同一坐标即命中 |

### 2.5 设置页 / i18n / 缓存目录

| 事实 | 证据 |
| --- | --- |
| 语言字典在 `src/lib/i18n.ts`，中文是源、`Record<keyof typeof zh, string>` 强制英文同 key；默认 `zh-CN`，存 localStorage `exmt.locale` | 切到 English 后整窗变英文；触发 Rust 重建重启后仍为英文（截图 `persist-en.png`） |
| 缓存目录默认 `C:\EXMTCacheFolder`，存 localStorage `exmt.cacheFolder`；`modbox.cacheDir()` 改为返回该目录，打包/隐藏的临时 bundle 都写这里 | `settings.ts` 的 `ensureCacheFolder`；ModelsView 三处 `join(await cacheDir(), …)` 未改 |
| 「打开」用 `@tauri-apps/plugin-opener` 的 `openPath`，capabilities 必须给 `opener:allow-open-path` + path scope `{path:"**"}`；只给 `open-url` 会静默失败 | 实测资源管理器打开 `C:\EXMTCacheFolder`（空目录）|
| 目录选择器标题是自绘的，跟随 i18n（本机系统按钮仍是中文，那是 Windows 自己的文案） | 英文界面下弹窗标题 "Choose a cache folder" |
| 该机器 shell 是 High Mandatory（管理员），所以 `C:\EXMTCacheFolder` 能建；非管理员机器可能被拒绝 → 设置页会红字报错而不是静默失败 | `whoami /groups` = S-1-16-12288；`useCacheFolder()` 先 mkdir 成功才落盘 |

### 2.6 模型工作台（3D 预览 / 贴图 / 工作空间）

| 事实 | 证据 |
| --- | --- |
| 网格与贴图**不在同一个 bundle**：mesh bundle 只有 `Mesh`+`AssetBundle`（抽样 30 个全部无 Texture2D），贴图在独立的 Texture2D bundle；skin prefab 引用的 material cab 有 2/3 不在本机 9613 个 bundle 里 | 470d/872f/287d 等逐个 `open_bundle` 统计类型；`proto3.py` 扫 60 个 bundle |
| 因此按「皮肤前缀」用**贴图名索引**反查 bundle：扫全部 bundle 的 node0（≤512 KiB 才可能是贴图包），14 进程 **9.4 s** 得到 31 869 个贴图名 / 5 027 个贴图包，缓存为 `texindex.json`（2.5 MB） | `python -m exmt_core texindex <AB> <cache>\texindex.json` → `{"bundles":9613,"textures":31869,"seconds":9.4}` |
| 扫描只解 node0，不解 `.resS`；node0 > 512 KiB 的多为 mesh 包，直接跳过 → 全量 50 GB 不必读 | `max_sf_for_index`，`_peek_texture_names` |
| 皮肤范围 = 名字前缀 `c_<Skin>_`：Alva 默认皮肤的 bundle（`cddc7a5b`）里有 **7 个网格**（body / hair / cloth1-4 / emitter），全要；catalog `parts` 只写了 hair/body，**不能当全集** | `entry.bundle = cddc7a5b`；`list_meshes` 实测 7 个，`parts` 只有 2 个 |
| 整皮肤几何一次返回：`geoset` 只解析一次 Bundle，7 个网格 2.8 MB JSON（核心 0.1 s），前端一次 IPC + 一个 three.js 场景全部画出来 | `python -m exmt_core geoset <spec.json>`；UI 截图里是完整角色 |
| 网格与贴图在**数据层平级**：贴图仍按皮肤前缀整组取（Alva 20 张），不按 bundle 过滤；UI 自 2026-10-19 起改为按子网格查看（§2.8），导出只含勾选的那批 | 20 张实测；`shot_061017.png` |
| 游戏贴图都是正方形：格子用 `padding-top:100%` 固定 1:1（不依赖 `aspect-ratio`），横向胶片条，别把页面撑高 | 截图；1:1 由 padding 保证 |
| 整套导出：7 网格 + 20 贴图 = 4.4 MB；Blender headless 导入 7 个 mesh、**1 副共用骨架**、11 张贴图进材质 | `make_workspace` + `EXMT_OT_import_workspace` 实测 |
| `make_workspace` 的 spec 必须把**贴图所在的 bundle** 也放进 `bundles`，否则每张贴图都 `failed_textures`（曾 20/20 失败） | 第一次 CLI 验证只放网格包；UI 侧同时收集网格与贴图的 bundle |
| 贴图索引的 bundle 名带 `.bundle` 后缀，传给核心前必须去掉，否则路径变成 `xxx.bundle.bundle`（曾导致贴图 0 张且错误被“无贴图”覆盖） | `bundlePathOf()` 与 `texturesFor()` 都做了 `replace(/\.bundle$/)` |
| 自动化验证：pwsh 用 `PrintWindow(PW_RENDERFULLCONTENT)` 抓窗口不受遮挡影响；但合成鼠标点击在窗口被别的程序盖住时会失效（`SetForegroundWindow` 被拒），只能靠真实前台 | `pwshot.ps1` 截图成功；点击需先置顶/前台 |
| 皮肤级持久缓存：`<cache>/skins/<skinKey>/{meshes,geometry,thumbs,meta}.json`（Alva 默认 geometry 2.76 MB + thumbs 876 KB + meshes 831 B + meta 251 B）；签名 = 相关 bundle 的 size+mtime | `C:\EXMTCacheFolder\skins\c_AlvaSSR01\*`；meta 里 `signature` 三个包 |
| 冷启动（新进程）里 2 秒内出完整皮肤并显示「本地缓存 · 2026-10-18 23:51」；同一会话内切走再切回不重新解析 | 截图 `cold-cache2.png`、`cache-back.png`（切回 2 秒内已渲染） |
| 页面状态用 `<KeepAlive>` 保留：首页→模型来回切，选中皮肤/网格可见性/贴图都不丢 | 截图 `keepalive.png` 与切走前逐像素一致 |
| Tauri 的**同步**命令跑在主线程：`run_core` 原来是 `fn`，导出 8 秒里窗口会假死；改成 `async fn` + `tauri::async_runtime::spawn_blocking` 后，同一次导出期间 24 次 `Get-Process exmt-modbox).Responding` 采样**全部 True** | `src-tauri/src/lib.rs` 的 `run_core` / `run_core_blocking`；导出 00:12:35 完成、采样间隔 300 ms |
| 导出完成后自动打开工作空间：`openPath(result.dir)`，实测资源管理器停在 `file:///C:/EXMTCacheFolder/workspace/c_AlvaSSR01` | `Shell.Application.Windows().LocationURL` |
| 勾选 = 最小改动：模型/贴图默认**全不勾**；只勾 `slg_cloth1_lod0` + `slg_cloth1_d` 时，工作空间里就只有 1 个 `.exmtmesh` + 1 张 PNG | 实测导出后 `meshes/` 1 个文件、`textures/` 1 个文件；状态行「已导出 1 个网格、1 张贴图」 |
| 工作空间每次写入前先删 `meshes/` `textures/`，否则上一轮的旧文件会混进来，"最小改动"就不成立 | `make_workspace` 里 `shutil.rmtree` 两个子目录 |
| 选择状态与 3D 一致：勾选的网格在场景里是亮蓝、没勾的是灰色；眼睛按钮只切显示/隐藏，不影响导出 | 截图 `pick-v5.png`（只勾 cloth1 时只有外套/袜子是蓝的） |
| 3D 滚轮缩放原来不生效：`enableDamping` 的相机必须在 `requestAnimationFrame` 里逐帧 `controls.update()`；另外 WebView2 的 `wheel.deltaY` 单位/步长不统一，交给 OrbitControls 会“滚了没反应” → 关掉 `enableZoom`，自己按 `exp(-delta/400)` 指数步进 dolly | `MeshViewer.vue` 的 `tick()` / `onWheel()`；UIA 点 `＋`×2 → 144%、`－` → 120%、重置 → 100% |
| 视图工具：右上角重置、右侧 `＋/－`、双击复位、左下角「滚轮缩放 · 拖拽旋转 · 双击复位 · 百分比」 | 截图 `viewer-final.png` |
| 线框模式（`material.wireframe` + `needsUpdate`）实测画面不变，已删除；要再做需换 `WireframeGeometry`/`LineSegments` | 对比图 `wire-compare.png` 平均差 0.67/255（等于没变） |
| `_d`/`_da` 是 diffuse：网格名去掉 `_lodN` 再加 `_d`/`_da`。Alva 默认皮肤 7 个网格里 body/hair/cloth1/cloth2/emitter 命中 `_d`，cloth4 只有 `_da`，cloth3 没有贴图（保持灰） | `diffuseNameFor()`；核心 `maps` 输出 mesh→texture 对应表 |
| `maps` 命令一次解整个皮肤的 diffuse（上限 1024px，PNG base64），按 bundle 只解析一次；Alva 7 张 → `diffuse.json` 5.39 MB，缓存进皮肤目录 | `C:\EXMTCacheFolder\skins\c_AlvaSSR01\diffuse.json`（5389692 B） |
| 预览光照：ACES Filmic（曝光 1.2）+ `RoomEnvironment` 环境反射（0.75）+ 三点布光（暖主光 4.2 / 冷补光 1.8 / 轮廓光 4.5 / 顶光 1.1 / 半球 0.7）+ 径向渐变软接触阴影 | `MeshViewer.vue`；截图 `light1.png` |
| 贴图朝向无需翻转：`TextureLoader` 默认 `flipY=true`，Unity 的 v 轴向上，实测外套/腰带/靴子图案位置正确 | 截图 `tex-zoom.png` |
| 预览里“头发像人偶”根因：`cddc7a5b…` 被 `c_AlvaSSR01-hair-basebody` 替换过（该包 mtime 20:55，目录里其他包都是 15:04）；旧版替换流程在 `%APPDATA%\com.exmt.modbox\backups\cddc….bundle` 留了原包（3 055 685 B，现包 2 819 348 B） | 目录 mtime 对比 + `%APPDATA%\com.exmt.modbox\mods\c_AlvaSSR01-hair-basebody.json` |
| 用旧备份恢复 `cddc…` 后头发恢复正常（白长发、大衣、靴子都对），说明不是渲染/贴图映射 bug | 截图 `hair-fixed2.png`（恢复前后对比） |
| 新增游戏路径设置：手填/浏览/自动查找（`GF2_Exilium.exe` → `…\AssetBundles_Windows`）/打开文件夹；未设置时模型、资源库、工具只显示「先设置游戏路径」占位 | 截图 `gate2.png`、`settings7.png` |
| 首次设置后自动备份 643 个资源包 / 6.48 GB，用时 6.62 s（NVMe）；写 `done.json` 标记完成，未完成前预览不读半套备份 | `C:\EXMTCacheFolder\game\done.json`：`copied 642, skipped 1, seconds 6.62` |
| 模型页、贴图索引只读 `<cache>\game\bundles`；恢复功能把备份覆盖回游戏目录；旧版 `%APPDATA%` 备份作为 `prefer` 优先于游戏目录（能救回已被 Mod 覆盖的包） | `bundlePathOf()` / `indexSourceDir()`；`store.py backup --prefer` |
| KeepAlive 的坑：`v-if` 必须放在 `<KeepAlive>` 外层，且动态组件保留 `:key`；写在内部会导致切导航内容不更新（表现为一直停在首页） | `App.vue`；修好后 `settings7.png` 能正常切页 |
| 模型页顶部的角色头像区/立绘卡（**旧布局，已被 §2.7 取代**）：角色头像区两列、44px 头像、**只留头像+名字**，列宽 200px；立绘区 = **卡牌轮播**：永远一行，当前卡居中（卡面比例 380:752，1138 逻辑宽窗口下约 193×380），其余按环形距离叠在左右，点侧卡即切换 | 截图 `shot_045148.png`（5 卡）、`shot_045201.png`（切到第 3 张仍是一行） |
| 立绘卡既不能固定高度、也不能 4 张平铺：固定高度把卡压成 138×320（比 0.505 更瘦），4 张平铺又太小 | 用户两次纠正；最终 `aspect-ratio: 380/752` + deck（`translateX 64/96/118%`、`scale .86/.74/.64`、`rotateY ±10~20°`、z-index 递减） |
| 侧卡点击会改 `selected` → 触发 `openStudio`；给按钮加 `@mousedown.prevent` 后，点卡不再把 `.app__content` 滚走（原来焦点会让页面跳 ~80px） | 点击前后截图对比：加之前 header 被滚出视口，加之后停在原位 |
| 启动时若已存游戏路径，`App.vue` 必须先 `checkGamePath`，否则新进程直接点"模型"会误显示"先设置游戏路径"占位（实际路径可用） | 本轮重启后复现，修好后页面直接显示真实路径状态 |

### 2.7 页面分工：首页 = 角色 + 立绘，模型页 = 子网格/贴图/3D（2026-10-19）

| 事实 | 证据 |
| --- | --- |
| 首页删掉全部营销/环境内容，改为 **左侧 200px 角色列表（2 列 44px 头像，搜索框+数量）+ 右侧立绘卡牌轮播**；页面无标题、无页头，直接 `height: calc(100vh - titlebar - 50px)` 撑满，列表在列内滚动 | `src/views/HomeView.vue`；截图 `C:\DevCache\tauri-logs\final_home.png`（1138×640 客户区） |
| 首页卡牌仍是 380:752、环形叠放；尺寸 `--deck-h: clamp(380px, 56vh, 600px)`，卡片宽度由高度算；当前卡居中，侧卡可点选 | 同上；Edge 1650×921 复查截图 `C:\DevCache\tauri-logs\edge_home.png` |
| 皮肤选择只有一处：`src/lib/selection.ts` 的 `selectedChar`/`selectedSkin` 共享；写 `localStorage["exmt.skin"]`，重启恢复；`characters`/卡牌整理移入 `src/lib/roster.ts` | 重启后首页直接停在上次皮肤；leveldb 里可见 `exmt.skin` 写入 |
| 首页「进入模型页」与模型页「换皮肤」用 `src/lib/nav.ts` 的 `currentView` ref 往返；`App.vue` 不再自己持有当前页 | 点击两按钮实测切换，回到首页时卡牌仍停在该皮肤 |
| 模型页只剩三个工作区（**旧布局，已被 §2.8 取代**）：网格列表 / 3D 预览 / 贴图胶片条；顶栏一行显示角色、皮肤标签、`N 网格 · N 贴图`、皮肤码，右侧「换皮肤 / 开始制作」 | 历史截图 `C:\DevCache\tauri-logs\final_models.png` |
| 旧布局：`height: calc(100vh - titlebar - 50px)`；grid `232px + 1fr`，行 `1fr + 172px`；贴图行 172px——158px 时 100px 贴图卡+说明+横向滚动条放不下 | 历史截图 `models_alva_default2.png`；现已被 §2.8 的三栏取代 |
| 模型页只在**被激活时**才读共享皮肤（`onActivated`），避免在首页连点卡牌时后台逐个解析/解码；页内激活期间由 `watch(selectedSkin)` 兜底 | `ModelsView.vue` 的 `sync()`/`active` |

### 2.8 模型页按子网格查看贴图 + 9:16 竖版 3D（2026-10-19）

| 事实 | 证据 |
| --- | --- |
| 模型页三栏：**左子网格列表 / 中当前子网格关联贴图 / 右 9:16 竖版 3D**；底部贴图胶片条删除；三栏顺序固定 | 终版截图 `C:\DevCache\tauri-logs\shot_061200.png`（cloth1 3 张方形贴图）、`shot_061017.png`（共享贴图组） |
| 3D 容器 `height:100%; aspect-ratio:9/16`；1729×960 物理窗口下实测约 410×730，人物站立占满高度 | 同上；CSS `.studio__viewer` |
| 贴图归属按名称前缀：子网格名去掉 `_lodN`/`_Dorm`/`_Fight` 后，贴图名以该前缀开头即归属；`cloth` 会吃到 `cloth01/02`，`Dorm`/`Fight` 两个变体共用同一组贴图 | `texturesForMesh()`；AlvaSSR0101 的 `slg_cloth_lod0_Dorm`/`_Fight` 各 7 张，`cloth1` 4 张、`cloth2` 3 张 |
| 没有任何子网格认领的贴图进“共享贴图”组（AlvaSSR01 20 张里 17 张归子网格，3 张 `face_d`/`eye_d`/`eyeblend` 进共享）；没有材质引用，无法证明它们真属于哪个子网格 | `unassignedTextures()`；`shot_061017.png`；blender 侧材质缺失见 §2.6 |
| 行点击 = 查看（active，3D 淡蓝），复选框 = 导出；勾贴图自动勾当前子网格，避免“只勾贴图不能制作” | 截图 `shot_060748.png`（Dorm + Fight 两行与对应贴图同时勾上） |
| 贴图方框 1:1：`padding-top:100%`，**边框只包方框，名称/尺寸在框外**；实测 162×162 CSS | 固定高度网格只写 `align-content:start` 会把行压扁、`overflow:hidden` 裁掉说明；加 `align-items:start; grid-auto-rows:max-content` 后 162×162，说明完整 |
| UI 术语统一为“子网格”（`models.studio.meshes` = 子网格 / Submeshes）；左行 meta 显示 `N 贴图`；顶部计数变成 `N 个子网格 · N 张贴图` | `src/lib/i18n.ts`；截图 `shot_060900.png` |
| 顶栏“全选 / 取消全选”：一次勾上全部子网格 + 全部贴图；全选后自动打开第一个有贴图的子网格（Alva 默认是 emitter），按钮变“取消全选”；再点清空并把“开始制作”重新禁用 | 截图 `shot_062826.png`（全选 7 子网格 + 20 贴图）、`shot_062837.png`（清空） |
| 旧布局（grid 232px + 1fr，行 1fr+172px，贴图底部胶片条）已被本节取代 | `final_models.png` 仅作历史对照 |

**未决**：目前贴图归属是名称前缀推断，不是材质引用；如果后续能从 prefab/material 读到真实 slot，应替换 `texturesForMesh()`，尤其是 `face`/`eye` 这类进共享组的贴图。

### 2.9 工作空间选择 + Blender 一键导入（2026-10-19）

| 事实 | 证据 |
| --- | --- |
| App 实时把“当前选中的工作空间”写到 `%APPDATA%\com.exmt.modbox\EXMTConfig.json`（Tauri `appDataDir()/EXMTConfig.json`），格式 `exmt-config-1`，含 `workspace_root`、`updated_at`、`workspace`（id/label/dir/entry/created_at）或 `null` | 实测文件 425 B；内容 UTF-8 正常（`阿尔瓦·2` / `10-19 06:41:53`）；路径 `C:\Users\Administrator\AppData\Roaming\com.exmt.modbox\EXMTConfig.json` |
| 选择工作空间 = 选择立绘：下拉选 `c_AlvaSSR0102` 后模型页切到“阿尔瓦·2”、12 子网格/12 贴图，并恢复该工作空间导出时的勾选；首页共享 `selectedSkin` 也会跟着变 | 截图 `C:\DevCache\tauri-logs\shot_064125.png` |
| 一个工作空间只属于一个立绘，一个立绘可有多个工作空间：每次“开始制作”新建 `<cache>/workspace/<皮肤>__<YYYYMMDD-HHmmss>/`，不覆盖旧目录；旧版按皮肤命名的 `c_AlvaSSR01`、`c_AlvaSSR0102` 仍作为 legacy 工作空间列出并可选中 | 新目录 `c_AlvaSSR0102__20261019-064153`；旧目录保留；截图 `shot_064209.png`（状态行显示新工作空间 12+12） |
| 工作空间下拉标签用 `MM-DD HH:mm:ss`（秒保留，年份/完整日期省略）以适配 230px 下拉框；列表按 `created_at` 倒序；旧 manifest 没有 `workspace_id`/`label` 时用目录名和 `created_at` 兜底 | `src/lib/workspaces.ts`；选中标签 `阿尔瓦·2 · 10-19 06:41:53` |
| 原生 `<select>` 的弹出层由 Windows/WebView2 系统绘制，暗色主题下白底白字、且只显示一项；改为全局 `src/components/Dropdown.vue`：Teleport 到 body、fixed 定位、300px 菜单、token 配色、选中勾/悬浮/键盘（↑↓/Enter/Esc），模型页工作空间下拉先接入 | 修复前截图见用户反馈；修复后 `shot_065452.png`（4 个工作空间全部可读）、`shot_065506.png`（点选后切回“阿尔瓦”并恢复 7 子网格/20 贴图） |
| 写 `EXMTConfig.json` 串行化（promise chain），避免快速切皮肤时后到达的 `null` 覆盖新选择；写配置的同时维护旧 `<cache>/workspace/current.json`，删掉时移除 | `ModelsView.vue` 的 `workspaceWrite` / `persistWorkspace()` |
| Blender 插件优先读 `EXMTConfig.json`（候选 `%APPDATA%\com.exmt.modbox\…`，其次是 `%APPDATA%\…` 兜底），只有配置文件完全不存在时才回退旧 `<workspace_dir>/current.json`；面板显示“当前：<label>”，按钮一键导入 | 修改 `C:\Dev\EXMTBlender\__init__.py`；`bl_info` 0.2.0 |
| Blender headless 实测：读配置 → `bpy.ops.exmt.import_workspace()` → `导入 12 个网格 · 12 张贴图 · 阿尔瓦·2 · 10-19 00:09:55`，场景 12 个角色网格 + 默认 Cube，28 个材质 | 命令：`blender.exe --background --factory-startup --python C:\DevCache\exmt-workspace\blender_import_test.py` |
| 已安装插件目录 `%APPDATA%\Blender Foundation\Blender\5.2\scripts\addons\EXMTBlender` 原本还是 10-18 的 0.1.0；已备份旧目录到 `C:\DevCache\exmt-workspace\EXMTBlender-installed-backup-20261019-064326` 并同步 dev 版（0.2.0） | 安装目录 `__init__.py` 18622 B / `mesh_io.py` 11284 B；重启 Blender 或 Reload Scripts 后生效 |
| 从**已安装目录**加载 0.2.0 复测：config 指向新工作空间 `c_AlvaSSR0102__20261019-064153`（label `阿尔瓦·2 · 10-19 06:41:53`），导入 12 网格/12 贴图，13 Mesh（含默认 Cube）/28 材质 | 命令：`blender.exe --background --factory-startup --python C:\DevCache\exmt-workspace\blender_installed_test.py`；输出 `RESULT {'FINISHED'}` |
| `开始制作` 的按钮 title 明确“每次制作都会新建一个工作空间，不覆盖旧的”；创建后新工作空间自动成为当前，状态行显示 label 与绝对目录 | `models.makeHint`；截图 `shot_064209.png` |

**未决**：工作空间还没有删除/重命名 UI；“开始制作”只新建、不更新已有工作空间（如果以后要“覆盖当前工作空间”，需要显式按钮）；工作空间列表只扫当前缓存根目录，换缓存后旧工作空间需要靠配置里的绝对 `dir` 或手动切回旧缓存才能看到。

### 2.10 骨骼：合并完整骨架 + 真正绑定（2026-10-19）

用户报的两个 bug：① 导入后骨骼没绑定到模型；② 「开始制作」导出的骨骼不完全（Blender 里只有一部分）。

| 事实 | 证据 |
| --- | --- |
| **每个 Mesh 只带自己用的那几根骨**：同一个 bundle（`0e834224…`）里 Alva 的 12 个网格分别带 3 / 4 / 8 / 13 / 26 / 28 / 32 / 35 / 46 / 56 / 70 根，**并集 226 根** | `C:\DevCache\exmt-bones\inspect_meshes.py` 逐个 `.exmtmesh` 统计 `bindpose`/`bone_hashes`/权重里出现的最大骨骼索引 |
| 所以“骨骼不完全”不是导出丢字段，而是**按网格看**：每个 `.exmtmesh` 只有它自己那张表；而老的 `import_workspace` 只留第一副骨架（body，56 根）并把它给所有网格用 → 别的网格 57…70 号骨在骨架里根本不存在 | 老 workspace 实测 + `EXMT_OT_import_workspace` 的 `first_arm` 复用逻辑 |
| “骨骼没绑定”的根因是**顶点组名 ≠ 骨骼名**：骨骼 `b%03d`（`b000`），顶点组 `b%d`（`b0`）→ Armature 修改器找不到同名骨骼，**0 个顶点**受骨骼驱动 | Blender 5.2 headless 复现：12 个网格 `groups=56..70 / matched=0 / weighted=0`（`blender_repro.py`） |
| **骨骼哈希 = `crc32(完整 transform 路径)`**，不是叶子名：`c_<Char>_slg_skinAvatar` 的 `m_TOS` 里 `(hash, path)` **30/30 全中**；武器网格同理（`cw_NikketaSSR01_WL_lod0` 的 rootHash = `crc32("Weapon")`，但其余 4 根骨 ≠ `crc32(叶子名)`） | `hash_scheme.py`、`hash_fn.py` |
| 本地 9613 个 bundle 里 **53 个带 `Avatar`**、418 个带 `SkinnedMeshRenderer`；`Avatar.m_TOS` 直接给“哈希 → 真名 + 父链”，prefab 的 Transform 树给同样的路径（去掉 prefab 多加的祖先） | `scan_types.py`（只解压 SF 元数据前缀即可列类型，9613 个 **14 s**）、`find_avatar.py`、`find_prefab.py` |
| Alva 自己的 prefab/Avatar **不在本地**（53 个 Avatar 里没有 `c_Alva*`），但跨角色共享的人形主链路径字符串相同 → 226 根里 **104 根**拿到真名 + 父链（`Spine1_M`→`Root_M`、`Head_M`→`Neck_M`、`Knee_L`→`HipPart1_L`…） | `bonenames.json`（50005 条，**13.2 s** 建成，冲突 0）；workspace 实测 `named=104, parented=103` |
| 修法：workspace 端先按哈希**求并集**合成一副骨架（`skeleton.json`），每个 `.exmtmesh` 里带“局部索引 → 骨架索引”映射；Blender 端只建**一副** 226 根骨的骨架，按映射给每个网格建**同名**顶点组 + 父级 + Armature 修改器 | `core/exmt_core/skeleton.py`、`preview.make_workspace`；`EXMTBlender` 的 `build_armature` / `_apply_weights` / `EXMT_OT_import_workspace` |
| 验证：12 个网格 **0 个未绑定组、0 个未蒙皮顶点**；`Spine1_M` 转 25° 时 body **4618/4618** 顶点跟着动（老代码是 0） | `blender_verify.py`，截图 `C:\DevCache\exmt-bones\shots\rig_skeleton.png` / `rig_posed.png` |
| 不改动直接导出：**权重与骨骼索引逐行逐位一致**（`diffW=0 diffB=0`，含只有 2 个影响槽的网格）；导出的文件自带骨骼表（`bone_hashes`/`bindpose`），再导入能重建骨架（226 根 / 组全命中 / 13768 顶点全受驱动） | `blender_verify.py` |
| 往返逐位一致的两个坑：等权重的影响槽必须按**索引升序**排（游戏数据如此，老代码 `sort(reverse=True)` 把并列项换了位）；权重和已经 ≈1 时**不要再归一化**（否则差 1 ULP）。修好前 579 个元素差 1.2e-7 | 对比 `src` / `got` 的 `bones`、`weights` |
| 打包回 bundle：从重打包的 bundle 再解一次，**数组 + typtree 全等**（`m_BindPose` / `m_BoneNameHashes` 原样保留） | `compare_mesh.py`：`all arrays identical`、`tree differing fields=[]` |
| 外部衣服（glTF 进来的物体，没有自己的骨骼表）：`bonetable` 给目标网格的骨骼表（带同一个名字索引），导出时先把顶点组名还原成哈希、再映射到目标索引 | `blender_foreign.py`：`dropped_influences=0`，权重与索引全等 |
| 骨骼名称索引存在 `<cache>/bonenames.json`，App 首次「开始制作」时自动建（约 13 s，之后复用）；索引路径写进 `EXMTConfig.json` 的 `bone_names`，Blender 插件据此调 `bonetable` | `src/lib/preview.ts` 的 `ensureBoneNames()`、`src/lib/workspaces.ts`、`__init__.py` 的 `target_bone_table()` |

**名字/层级的边界**：本地没有该角色 prefab 时，只有跨角色共享的骨骼能命名（Alva 104/226），其余保持 `bone_<8 位哈希>` 且**不猜父级**（宁可平铺也不乱接链）。在 Blender 里把骨骼改名后导出，会按“不属于该网格的权重”计入 `dropped_influences` 并在面板上报数量。

---

### 2.11 改完 → 生成 Mod → 一键进游戏（2026-10-19）

用户要的闭环：「如果我想修改完之后，点击生成 Mod，游戏里直接就能看效果」。做法：**Blender 侧把工作空间里所有网格打成一个 Mod 写到工作空间目录，App 侧一个键装进游戏**（不直接写游戏目录，先落盘、可检查、可重装）。

| 事实 | 证据 |
| --- | --- |
| 在此之前 App **根本没有安装入口**：`src/lib/modbox.ts` 里的 `installBundle` / `writeMod` / `ensureBackup` 全是被死代码，`src/` 没有任何地方调用；「资源库」只能从旧备份还原（那条路是给老的替换流程用的） | 全仓库 `grep installBundle` 只命中定义处 |
| 新核心命令 `modpack <spec.json>`：按 bundle 分组，**一个包只写一次**（同包里改了几个网格不会互相覆盖），输出 `<outdir>/<bundle hash>.bundle` + `mod.json`（`format: exmt-mod-1`，含每个包的哈希/字节数/网格名/来源路径） | `core/exmt_core/modpack.py`、`cli.py` 的 `cmd_modpack`；`patch_mesh()` 从 `cmd_pack` 抽出来，两条路共用 |
| 打包必须从**原始字节**开始：工作空间的 `bundles` 映射现在只写原始副本（`<cache>/game/bundles` → 旧备份 → 游戏目录），否则连点两次「生成 Mod」会把上一次的改动烤进第二次；App 的 `packSource()` 与备份用同一套优先级 | `src/lib/preview.ts` 的 `packSource()`、`modbox.ensureBackup()`；实测用户那份 workspace 的 `spec.json` 原本指向游戏目录（因为 `backup.ready` 当时为 false），已改成优先原始副本 |
| 工作空间现在自带 `bundles`（`workspace.json`）+ 核心写一份 `spec.json` 进工作空间 → **Blender 侧不依赖 App 在跑** | `preview.make_workspace`；`C:\EXMTCacheFolder\workspace\c_AlvaSSR0102__20261019-084927\spec.json` |
| Blender 面板新增「生成 Mod」：按对象上的 `exmt_source` 找到本工作空间的网格 → 导出到临时目录 → 调 `modpack`；场景里被删掉的网格**不打包**并报数（原包保持不动） | `EXMTBlender/__init__.py` 的 `EXMT_OT_generate_mod` / `generate_mod()`；headless 实测：12 个网格 → 1 个包 3.7 MB，删掉 `cloth4` 后 → 11 个网格 + `skipped: ['…cloth4_lod0']` |
| App 模型页新增「加载到游戏」：读 `<工作空间>/GeneratedMod/mod.json` → 备份原始包（**只备份一次**）→ 覆盖游戏包 → 写一条记录 `%APPDATA%\com.exmt.modbox\mods\ws-<工作空间>.json`；装过之后按钮变成「卸下 Mod」，一键还原 | `src/views/ModelsView.vue`、`src/lib/modbox.ts` 的 `readGeneratedMod/installGeneratedMod/uninstallBundles` |
| **实测（真实 App + 真实游戏目录）**：点「加载到游戏」→ 游戏包 `0e834224….bundle` 从 3642020 → 3695969 字节，备份 `3642020` 出现在 `%APPDATA%\com.exmt.modbox\backups\`（`source` 指向 `<cache>/game/bundles` 原始副本），状态行 `✓ 已装进游戏：1 个包 / 12 个网格 / 3.5 MB · 重启游戏生效`；点「卸下 Mod」→ 游戏包回到 3642020 字节且**与备份逐字节相同**（SHA256 相等），记录删除 | `app2.ps1` 驱动 + 截图 `C:\DevCache\tauri-logs\installed.png` / `uninstalled.png`；文件哈希比对 |
| 打包保真 ①：**每子网格 `localAABB` 以前是按“索引区间”算的**（`pos[min:max+1]`），把区间里没被这个子网格引用的顶点也算进去 → 盒体偏大、和游戏自己写的数不一样（12 个网格里 2 个因此不相等）。改成**按被引用的顶点**算后，未改动的工作空间 12/12 网格**字段零差异、顶点数据逐字节相同** | `C:\DevCache\exmt-bones\verify_modpack.py`（`fields differing=0`） |
| 打包保真 ②：只改一个网格（头发 X +5 cm）→ 只有它的顶点数据变，其余 11 个仍然逐字节相同；包里的几何确实是改后的 | `verify_modpack_edit.py`（`changed == [hair]`、`X center 0.0016 → 0.0516`） |
| 打包保真 ③：**Blender 往返会重算法线/切线**（实测中位数偏 2.26° / 11.40°，切线还有 2 个符号翻转），也就是"没动过也会变一点"。修法：导入时把游戏法线设成 **custom split normals**，切线存成网格属性（`exmt_tangent` + `exmt_tangent_w`），导出优先读它们（顶点数变了才回退到重算）→ 重建后 **p90 法线 0.01°、切线 0.00°、w 翻转 0**，权重/骨骼/UV 逐位不变 | `EXMTBlender/mesh_io.py`；`verify_mod_angles.py`（角度分布，不看裸 float 差） |
| 踩坑：`foreach_get` 写进 `out[:, :3]` 这种**非连续视图**等于写进临时拷贝，读回来全是 0（切线全零）。必须用连续临时数组再赋值 | 同上（切线从全 0 变回逐位一致） |
| 位置仍有 1…2e-7 的差（float32 最后一位，来自 `matrix_world @ v.co` 的那次乘法），肉眼与游戏都不可见；权重/骨骼索引/UV/颜色/索引表是逐位相等 | `verify_mod_angles.py` 的 `positions: max 2.38e-07` |

**没有验证的部分**：游戏内实际渲染效果（本次只验证到"包被替换、字节可还原、几何/属性保真"），以及游戏运行时是否锁定包文件（写入失败会直接抛错，尚未遇到）。

---

### 2.12 多个 Mod 整合 + 同名网格的所有拷贝（2026-10-19）

用户问：「加载多个不同角色的 Mod 会不会冲突，到底加载哪个包？」——会，而且原因是**文件级覆盖**，不是加载顺序。

| 事实 | 证据 |
| --- | --- |
| 643 个备份包里 **68 个装着多个角色的网格**（`0e834224…` 里同时有 `c_AlvaSSR01` / `c_AlvaSSR0101` / `c_AlvaSSR0102`）；两个 Mod 改到同一个包时，后装的把先装的整包覆盖 | 扫包统计 + `C:\DevCache\verify_merge.py`：先装 A（头发 +5 cm）再装 B（身体 +3 cm）→ 头发回到 0.0016，A 白改了 |
| 改法：**Mod 不再等于"打好的包"**，而是**补丁集合**——`modpack` 现在把每个补丁的 `mesh / file / bundles` 写进 `mod.json`，Blender 把导出的 `.exmtmesh` 留在 `<工作空间>/GeneratedMod/patches/`。运行时状态 = 原始包 + **所有启用 Mod 的补丁**，一次打包一次写入（`applyMods`） | `core/exmt_core/modpack.py` 的 `patches`、`EXMTBlender.gener_generate_mod`、`src/lib/modbox.ts` 的 `applyMods/patchFile/setModEnabled`；状态文件 `<cache>/applied.json` |
| **实测（真 App）**：仓库里同时启用「测试 Mod：头发 +5 cm」和「阿尔瓦·2（12 个补丁）」→ 点「重新整合」→ `applied.json` 的同一个包下同时列出两个 Mod，游戏文件里头发 X `0.0016 → 0.0516` | 截图 `C:\DevCache\tauri-logs\library1.png`/`library3.png`；游戏包几何量测 |
| 停用只去掉它自己的改动：取消勾选测试 Mod → 游戏文件头发回到 `0.0016`，`applied.json` 只剩工作空间那份 | 同上 |
| 全部停用 → 游戏包回到 `3642020` 字节且**与备份 SHA256 相等**；`applied.json` 变成 `{bundles:{}}` | 文件哈希比对 |
| 资源库页现在是 Mod 管理页：每行一个勾选框（启用/停用即重算）+「重新整合」+「删除记录」，并显示「当前游戏状态：N 个包被改写 · 整合于 …」。旧版记录（没有补丁文件，装于本轮之前）显示 `0 个补丁`、勾选框置灰、保持原状不动 | `src/views/LibraryView.vue` |
| 状态行数字只数**真正参与整合**的 Mod（`ApplyResult.mods`）；旧记录单独计 `legacy`，避免"3 个 Mod 整合成 1 个包"这种假数 | `modbox.applyMods` |

**同名网格的所有拷贝**（上一轮 §4-9 的问题，本轮修完）：

| 事实 | 证据 |
| --- | --- |
| `meshidx` 扫 643 个备份包建 `网格名 → [所有包]`：**13.5 s**，2544 个网格名里 **240 个有多份拷贝**（9.4%）；`c_AlvaSSR0101_slg_cloth3_lod0` 同时在 `0e834224…` 和 `a75eb9c24873…` | `python -m exmt_core meshidx <backupDir> <json>`，结果 `<cache>/meshindex.json` |
| 工作空间现在写 `mesh_bundles`（每个网格的所有包），Blender 生成 Mod 时对每个拷贝各打一行 → 改一次，所有副本一起变 | `preview.make_workspace`、`generate_mod`、`modpack.build`（同网格多包） |
| **跨包不能照抄整棵树**：两个拷贝的布局可能不同（实测同一网格：一个 UV0 是 float16 + `m_IsReadable=false`，另一个 float32 + 可读；`m_BindPose`/`m_BonesAABB` 也有末位差）。`patch_mesh` 改为：只有在「通道布局 + 可读标志 + 骨骼哈希表」全同的时候才用文件自带的 tree，否则**以目标包的 tree 为底**，只写几何，并把骨骼索引按哈希重映射到目标的表（`dropped_bones` 报告丢失） | `core/exmt_core/modpack.py` 的 `_compatible/_fit_channels/_remap_bones`；`C:\DevCache\verify_multicopy2.py`：两个包都拿到 +4 cm，两边的 UV 格式与 `m_IsReadable` **都保持原样** |
| 多拷贝打包实测：一个网格打两个包 → `bundles=2, meshes=2, missing=0`，每个包里都是改后的几何 | `C:\DevCache\verify_multicopy.py` |

**没有验证的部分**：游戏内实际效果（本轮仍只到文件/几何层）；两个 Mod 改**同一个网格**时谁赢（当前按 `installed_at` 升序、后装的覆盖，未在 UI 上提示）。

---

### 2.13 Blender 侧：集合 + Submesh 索引 + 蓝图（2026-10-19，插件 0.4.0）

用户要求把"物体身份"从**物体上的自定义属性**搬到**蓝图里的连线**：

| 事实 | 证据 |
| --- | --- |
| 导入时创建一个集合 `EXMT · <工作空间 label>`，本次导入的网格 + 骨架**全部**放进去（并从别的集合解链），两个工作空间可以并存不混 | `EXMTBlender/__init__.py` 的 `_workspace_collection`/`_link`；实测集合里 13 个对象（12 网格 + 骨架） |
| Submesh 索引由**本体**在导出时分配：`workspace.json` 的每个 mesh 现在带 `index`（从 0 自增）+ `alias`（`c_AlvaSSR0102_slg_cloth1_lod0` → `cloth1_lod0`） | `core/exmt_core/preview.py` 的 `make_workspace`/`_alias`；实测 `0. body_lod0` … `11. hair_lod0` |
| 新增蓝图（自定义 NodeTree `EXMT_BlueprintTree`）：`物体` 节点（选物体 + 选 Submesh，下拉显示 `0. body_lod0`）+ `生成 Mod` 节点（**每个 Submesh 一个输入槽**，槽名就是 `index. alias`） | `EXMTBlender/blueprint.py`；headless 实测：12 槽、12 个物体节点、全部预连 |
| 一键导入会新建一个蓝图并**预连**所有槽位（导入的网格 → 自己的槽位），改完直接点「生成 Mod」即可；想换模型就把物体节点的物体换成自己的模型，槽位不变 | `_prewire_blueprint`；实测把 `11. hair_lod0` 换成 8 顶点的立方体 → patch 变成 8 顶点/12 面、bbox 0.5³，其余 11 个不变 |
| 断开的槽位 = 这个 Submesh 不打包（游戏里保持原样），面板/报告里列出来；没被引用的旧补丁文件会清掉 | 实测断连 `body_lod0` → `packed: 11`、`skipped: ["body_lod0"]` |
| 面板新增「蓝图」组：下拉选择 + 4 个图标按钮（新建 / 在节点编辑器打开 / 同步槽位 / 删除），下面显示 `N 个 Submesh · 已连 M 个槽位` 和「生成 Mod」；生成节点上也有「同步槽位 + 生成 Mod」按钮；节点编辑器右键菜单里有两个"新建节点"入口 | `EXMT_PT_panel`、`EXMTNodeGenerate.draw_buttons`、`_menu_add`（挂 `NODE_MT_context_menu`/`NODE_MT_add`） |
| **Blender 5.2 的坑**：`SpaceNodeEditor` 已经没有 `ui_type`，`edit_tree`/`id` 变成只读；要把自定义 NodeTree 显示出来必须写 **`space.tree_type = <bl_idname>` + `space.node_tree = <tree>`** | `C:\DevCache\exmt-bones\probe_uitype.py`：列出可写属性后确认；截图 `C:\DevCache\tauri-logs\blender_blueprint.png`（12 个物体节点连到「生成 Mod（12 个已连接）」） |
| 物体上的 `exmt_source`/`exmt_workspace` 自定义属性**已删除**（身份只由蓝图决定） | 同文件；`generate_mod` 改为遍历蓝图里的连线 |

**没有验证的部分**：蓝图在 Blender 3.x/4.x 上的表现（只在本机 5.2 实测；`edit_tree` 那条分支未跑过）；多个蓝图同时指向同一个工作空间时"后处理的蓝图覆盖前面的"（代码按此实现，未做 UI 提示）。

---

## 3. 交付物清单

| 产物 | 位置 |
| --- | --- |
| 应用 | `C:\Dev\EXMT`（`src/` 前端、`src-tauri/` Rust） |
| 核心 | `core/exmt_core/`（CLI：list/meshes/extract/extract-all/bonetable/pack/**modpack**/hide-mesh/subdivide/retarget/copy-texture/set-texture/texindex/bonenames/skinfo/geo/geoset/thumbs/maps/workspace） |
| Blender 插件 | `C:\Dev\EXMTBlender\{__init__.py, mesh_io.py, blueprint.py}`（**0.4.0**：一键导入工作空间 → 建集合 + 蓝图 / Submesh 索引 / 生成 Mod；已同步到 `%APPDATA%\Blender Foundation\Blender\5.2\scripts\addons\EXMTBlender`；**该目录不是 git 仓库**，只能靠快照：`C:\DevCache\exmt-bones\EXMTBlender-0.*`） |
| 文档站 | `C:\Dev\EXMTDoc`（VitePress 2，独立仓库） |
| 本仓库 docs | `exmt-modbox.md`、`exmt-model-pipeline.md`、`exmt-characters.md`、`gf2-exilium-recon.md` |
| skill | `.agents/skills/unity-il2cpp-game-recon/`（§18–§23）、`.agents/skills/tauri-winui-shell/` |

---

## 4. 未决问题（按优先级）

1. **body 槽独立身体（G5）**
   - 现状：外部 VRoid 身体（T-Pose）装进 body 槽后炸模；三种权重策略都失败（最近顶点 / 最近骨骼 envelope / +拉普拉斯平滑）。
   - 结论：必须做**绑定空间重定向** `v' = Σ w_i (GameBind_i⁻¹ · SourceBind_i) v`。
   - 新条件（2026-10-19）：骨架现在**完整**了（226 根，含名字与部分父链），所以 Blender 侧可以把外部骨架摆成游戏 bind pose → 烘焙 → DataTransfer 权重 → 导出；比之前"没有骨架"时可行得多。
   - 下一步：在 Blender 里用 `skeleton.json` 的 `rest` 对齐外部骨架，再走 `传递权重并绑定骨架`。
2. **骨骼命名（部分解决）**：`crc32(路径)` 已被证明，索引已能给出 104/226（Alva）。要更高覆盖需要有该角色 prefab/Avatar 的 bundle；本地没有就只能保持 `bone_<hash>`。**改名后导出会丢权重**（面板报数量）。
3. **贴图写入只支持 DXT1**：法线/RMO 是 BC7，换贴图需要 BC7 编码器（或整段替换同尺寸 BC7）。
4. **导入不做轴向转换**：游戏是 Y-up，导入 Blender（Z-up）后模型沿 +Y 躺着（bbox 实测高 1.589 在 Y 轴）。要做就得同时改导出端（`matrix_world` 会把旋转烘进顶点），本轮没动。
5. **设置页右侧 about 面板**：已实现（7:3 + sticky），但截图里没确认渲染，需要用干净窗口复验。
6. **文档站链接**：`SettingsView` 的 `LINKS` 仍是占位（本地 5200 / GitHub 仓库）。
7. **文档站旧页**：`faq.md`（“导出的 glTF 没有贴图”）、`reference/layout.md`（`gltf.py`）、`guide/index.md`、`reference/cli.md`（`glb` 命令）、`install.md`（点「导出 glTF」）仍写着已删除的 glTF 流程，需要一轮清理。
8. **`C:\Dev\EXMTDoc` 没有 remote**：只能本地 commit，push 需要用户提供仓库地址。
9. ~~**同名网格的多份拷贝只打了一份**~~ → **已修（2026-10-19，见 §2.12）**：`meshidx` 建 `网格名 → 所有包`
   索引（13.5 s / 643 个包），工作空间写 `mesh_bundles`，生成 Mod 时每个拷贝都打；跨包差异用"目标包的 tree 为底 + 按哈希重映射骨骼"处理。

---

## 5. 本轮之后的检查点

- 每次改 `AGENTS.md` / `docs` / skill：在本文件顶部更新"最近更新"。
- 每次得到新的"游戏事实"：追加到 §2，带证据来源。
- 每次解决/新增未决：更新 §4。
