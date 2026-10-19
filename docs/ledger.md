# 台账 · EXMT Modbox

> 结论 + 证据 + 未决问题。**不写流水账**（不记"先跑 X 再跑 Y"）。
> 每轮开始读这里，结束前更新这里。目标状态只在这里，不在 AGENTS.md。

最近更新：2026-10-20（AGENTS.md 拆成「规则层」+ `docs/agents-playbook|ui-spec|corrections.md` 细节层；§2.19 动画解码在 G13）

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
| G13 | **动作（AnimationClip）：立绘 → 模型页关联查询 → 随「开始制作」导出 → Blender 里播** | ✅ 完成（挂件未决） | 全库索引 63 秒 / 39974 个 clip；模型页「动作」栏按皮肤前缀查（Vepley 168 个）；「开始制作」导出 `animations/*.exmtanim`；Blender 导入成 Action + NLA，首帧 vs 不挂动作 **0 顶点移动**、中间帧正常变形。已知缺口：零权重挂件（脸/武器）不跟动作走（见 §2.20 未决①） |

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

### 2.14 蓝图 v2：槽位去含义 + 分组 + Frame + 贴图替换节点（2026-10-19，插件 0.5.0）

用户四条要求：**槽位不要有意义**（哪些生成由物体节点自己的 Submesh 决定，没连的=只参考不打包）、**一键导入把所有原物体装进 `<工作空间>_original` 的 Frame**、**加分组节点**（物体→分组→生成）、**加贴图替换节点**（按勾选的贴图自动建，能选替换图，并自动转成目标贴图的格式）。

| 事实 | 证据 |
| --- | --- |
| 生成节点的槽位改成**无含义**的 `网格 1..N`：打包集合 = 从生成节点出发（可穿过任意 `分组` 节点）能走到的所有**物体节点**，目标 Submesh 取该物体节点自己的 `submesh` | `blueprint.walk_objects()` + `generate_mod`；headless 实测 12 物体 → 12 Submesh |
| **没连线的物体节点不打包**（这就是"只导入做参考"的用法） | 实测：新加一个 `reference_cube` 物体节点（自带 submesh 0）不连线 → 仍然 `meshes: 12` |
| 把某个物体节点从分组上断掉 → 那个 Submesh 消失、旧补丁文件被清掉 | 实测断开 `body_lod0` → `{"meshes": 11, "skipped": ["body_lod0"]}`，`patches/c_AlvaSSR0102_slg_body_lod0.exmtmesh` 已删除 |
| 新增 `分组` 节点（N 个输入 + 1 个输出），导入时自动建立：12 个物体 → 分组 → 生成 Mod（生成节点**只连了 1 根线**） | 截图 `EXMTDoc/docs/public/img/blender-blueprint.png`；headless：`group: 12 inputs linked, 1 outputs linked`、`generate node: 1 links, 2 slots` |
| 导入时把 12 个物体节点装进 Frame `Alva2_original`（= `<label>_original`），贴图节点装进 `Alva2_textures`；Frame 的尺寸是**按子节点包围盒算**的（`shrink` 只在你手动拖动时才重新贴合，Python 建出来永远是 160×100） | `_frame_around()`（5.2 的 `NodeFrame.width/height` 可写）；headless：`frame labels: ['Alva2_original', 'Alva2_textures']`、`objects parented to frame: 12` |
| 新增 `贴图` 节点：一键导入按本体里勾选的贴图各建一个，节点上显示 `2048x2048 · BC7 · 12 mips` + 一个图片选择器；**选了图就替换，不选就跳过** | 截图同上；实测 12 个贴图节点、`texture_meta` 缓存 12 条（导入时用一次 `texinfo` 问本体） |
| 贴图替换走 **texconv.exe**（内置 `EXMTBlender/bin/texconv.exe`，DirectXTex 2026.5.8.1）：`-f <目标格式> -w -h -m <mips>`，把 PNG 编码成目标贴图**原本的格式/尺寸/mip 数** | `_encode_texture`；实测 BC1 2048² 12 mips → 2796216 字节，与资产 `m_CompleteImageSize` 完全一致 |
| 核心不碰图像：`modpack` 只写字节（`texpatch.apply`），Mod 里存**已编码**的 `patches/*.bin` + 一条 `textures[]` 记录，App 侧合并也只是把 bin 交给 modpack | `core/exmt_core/{texpatch,ddsinfo}.py`、`modbox.ts` 的 `ModTexture`/`texRows` |
| **UnityPy 的坑**：Texture2D 的像素在 typetree 里叫 **`image data`**（带空格）；写 `m_ImageData`/`image_data` 会被当新键静默丢弃（实测包只涨 140 字节、像素没变） | `texpatch._tree_payload`/`apply`、`sf.texture_info` |
| **方向坑**：Unity 的 Texture2D 行序是**自下而上**，texconv 对 PNG 输出是自上而下 → 必须把每个 mip 的块行整体翻一次；不翻时替换图与原图差 10.1 dB，翻后 **59.2 dB** | `texpatch.vertical_flip`；`C:\DevCache\verify_texpatch.py` |
| **色彩坑**：texconv 把 Blender 写的 PNG 当 sRGB，输出 UNORM 时会转成线性（实测 31→4、219→181，整体变暗）→ 固定加 `-srgbi -srgbo`（恒等）让数字原样落盘 | `_encode_texture`；实测 `-srgbo` 后 mean `[127.45,32,222]` vs 源 `[127.5,31,219]`，最大差 4（BC1 量化） |
| App 侧：`ModRecord.textures`、`applyMods` 把贴图行并进 spec（**只被贴图碰过的包也要进 `targets`**，否则整包被漏掉）、`applied.json` 记 `textures`、模型页读 `GeneratedMod/mod.json` 时**重新读一遍**（避免页面加载后又在 Blender 里生成的补丁被丢掉） | `src/lib/modbox.ts`、`src/views/ModelsView.vue`；实测 `applied.json` 两个包（11 网格 / 1 贴图） |

**端到端实测（真实数据）**：Blender 里给 `c_AlvaSSR0102_slg_cloth1_d`（BC1 2048² 12 mips）选一张渐变图 → 生成 Mod（11 网格 + 1 贴图）→ App「加载到游戏」→ 从**游戏目录的包**里解出该贴图：`[127.5, 32, 222]` vs 目标 `[127.5, 31, 219]`（**42.17 dB**，最大差 4），原图对比 9.43 dB；格式仍是 BC1、2048²、12 mips、2796216 字节。卸下后两个包 SHA256 与备份**逐位相等**。

**没有验证的部分**：游戏内实际观感（没进游戏看）；BC6H/BC4/BC5 目标格式（映射表里有，但手上没有这种贴图可试）；`分组` 多层嵌套（只测了一层）。

**复盘补测（2026-10-19 评估轮，不改代码只量数据）**：

| 量到的事实 | 数字 | 意味着什么 |
| --- | --- | --- |
| 全库贴图重名 | **3824 个贴图名里 72 个有多份拷贝**（`RampMap_Linear_RGBAHalf` 542 份、`Handmade_base_d`/`_rmo` 各 39、`cw_ColphneSR01_WL_d` 5） | 贴图补丁现在只打 manifest 记的那**一个** bundle；换成上面这些共享贴图就会漏拷（网格侧早有 `mesh_bundles` 打全拷贝，贴图没对齐）。本皮肤 12 张全是单份，所以端到端验证没暴露这条 |
| texconv 一张 2048² 12 mips | BC7 **5.0 s**、BC1 0.2 s | 12 张全换成 BC7 ≈ 60 s；`generate_mod` 是同步跑的，这段时间 **Blender 界面无响应**、没有进度、不能取消 |
| 重新导入同一工作空间 | `new_blueprint()` 会 `bpy.data.node_groups.remove(同名)`；`_workspace_collection()` 是同名复用 | 再点一次「一键导入当前工作空间」会**删掉用户连好的蓝图**（连带 12 个贴图节点的选图），旧物体留在集合里变成 `…001` 重复。用户明确说过"导入后要能自己连线再导出"，这条最容易踩 |
| BC7 走插件的全链路 | 未测 | BC7 只在核心层用**无 sRGB 标记**的 PNG 验过 59.2 dB；插件写出的 PNG 带 sRGB 标记，`-srgbi -srgbo` 之后只有 BC1 端到端验过 42.17 dB。原理上恒等，但没有实测 BC7 |

---

### 2.15 评估轮四个缺口的修复（2026-10-19，插件 0.6.0）

上一轮评估量到的缺口（上表 + §4 第 10–13 条），这一轮全部修掉并复测：

| 缺口 | 修法 | 实测 |
| --- | --- | --- |
| **重新导入会删掉蓝图**（`new_blueprint()` 同名直接 remove） | `new_blueprint()` 改为**同名复用**（缺生成节点才补）。`_prewire_blueprint()` 变成"更新"：`<label>_original` Frame 里的物体节点按 `submesh` **重新指向新导入的网格**，连线/自定义节点/贴图选图全保留；Frame 外的用户节点一律不动 | headless：同一棵树、自定义节点还在、**贴图选图还在**、连线数 `13 → 13` 不变 |
| 重新导入攒重复物体（`…_body_lod0.001`） | 导入**前**先 `_forget_previous_import()`：删掉上一批 Frame 内节点指向的网格（连带 mesh 数据块——否则名字还被占着，新导入只能叫 `.001`），并回收没孩子的旧骨架；骨架按"骨骼名集合相同"**复用**（姿势也保住） | 再导入后 `.001` 列表 **`[]`**，mesh 数据块 14 → 14（不增长） |
| **贴图只打一个包** | `meshidx` 一次扫描同时收 Mesh 与 Texture2D（格式 `exmt-meshindex-2`，`{meshes, textures}`）；工作空间写 `texture_bundles`（与 `mesh_bundles` 对称）；生成时每张贴图**每个拷贝各出一行**。旧索引缺 `textures` 会自动重建 | 全库 **3824 个贴图名里 72 个有多份**；新工作空间 `texture_bundles` 12 条；合成测试：同名贴图放两个包 → **两个包都打上**（`summary.textures: 1`） |
| **生成时界面卡死**（texconv 一张 2048² BC7 = 5 s，12 张 ≈ 60 s 同步阻塞） | ① 编码缓存：源图（路径+mtime+size，脏数据块不算）与目标都没变就复用 `.bin`；② texconv **并行**（线程池 ≤6）；③ 生成拆成 `Generation.start()/step()`，操作符改成 **modal**，每个 tick 收一个编码结果，进度写状态栏并**画在「生成 Mod」节点标签上**，Esc 可取消（`execute()` 保留同步路径给 headless） | 12 张贴图：**冷启动 34.9 s**（原 ~60 s）、**热缓存 9.1 s（encoded=0）**；截图 `win_20261019_133334.png` 里节点显示 `生成 Mod · 编码贴图 5/12 cloth4_n`，模态运行中窗口仍在重绘 |

**补测（评估时列的"真空白"）**：BC7 走插件全链路（Blender 写的 sRGB PNG + `-srgbi -srgbo` + 翻行）从**打包后的 bundle** 解回来 **54.94 dB / 最大通道差 2**（此前只在核心层用无 sRGB 标记的 PNG 验过 59.2 dB）；一次换两张（BC1 + BC7）同时生成，两条 `mod.json` 行的格式/尺寸/mips/字节数都对（2796216 / 5592432）；App 把这份 2 贴图 Mod 装进游戏后，两张贴图在**游戏目录的包**里都变成测试图，卸下后两个包 SHA256 与备份逐位相等。

**仍然没做的**：游戏内实际观感（没进游戏）；BC6H/BC4/BC5 目标格式；`分组` 多层嵌套；Blender 4.x。

**顺手修的两处 UI**：贴图节点的图片选择器改成 `layout.template_ID(node, "image", open="image.open")`（多一个浏览按钮，截图 `blender-screen.png`）；枚举条目的 label/description 去掉非 ASCII——**Blender 5.2 会把带 `·` 的枚举字段整格画成乱码**，而同样文字放 `layout.label()` 完全正常（先用 PrintWindow 抓、再用屏幕抓排除了"截图伪影"，换成 ASCII 空格后立刻正常）。

### 2.16 外部模型权重被静默丢光 + 空骨架 + 多副本骨骼表分裂（2026-10-19）

用户报告："游戏内完全不显示我的模型"，并质疑"是不是没绑定到骨骼上"。

**症状 → 根因链**（全程数据，不是推断）：

| 环节 | 观测 | 判定 |
| --- | --- | --- |
| 安装 | 6 个包 mtime 16:36:13…18，size 与 `mod.json` 的 `bytes` 逐字节相等 | 安装**成功**，不是没装上 |
| Blender | `Sally_outfit` 有 126 顶点组、**Armature 修改器指向 `Sally_causal_2.0_arm`，该骨架 `bones=0`** | 预览坏（但这不是游戏不显示的原因） |
| 顶点组 | `Sally_outfit` 的 126 组名 vs 目标 `cloth_lod0` 的 126 组名 | **完全一致**（交集 126）；但 `Sally_outfit` **没有 `exmt_bones` 属性** |
| 导出的 patch | `bone_hashes=0`、无 `bindpose`、**权重行和全 0.0**、越界索引 50936、实际生效骨骼 **0** | **网格没有任何蒙皮** → Unity 画不出来 |
| 根因 | `__init__.py` 生成 Mod 的循环调用 `mesh_io.export_mesh(obj, path, {...})` **不传 `bone_table`**；`_bone_slots()` 的 `own` 来自物体的 `exmt_bones`，外部导入的物体没有 → `target=[]` → `resolve()` 返回 None → **每条权重都被 `dropped += 1`** | 插件 bug，不是用户操作错 |

**新增可复用判据 —— 判"权重有没有真的进导出的文件"看四个数**（任一为 0 就是废数据）：

1. `weights.sum(axis=1)` 全部 ≈ 1.0（行和）
2. `bones.max() < len(bone_hashes)`（索引不越界）
3. `bone_hashes` 非空、`bindpose` 存在
4. 参与加权的**不同骨骼槽数** > 0（本例修复后 96）

**修法**（三处，都在插件侧）：

| 改动 | 内容 |
| --- | --- |
| `mesh_io._bone_slots` | `target` 为空时从**场景里已导入的 EXMT 网格**借一张骨骼表（`_scene_bone_table()` 取最大的那张）。实测场景回退 = 126 项、完整覆盖 Sally 的 126 组 |
| `__init__.py` 生成循环 | 显式 `bone_table = target_bone_table(row["bundle"], mesh_name)`（物体无 `exmt_bones` 时）；导出后若 `groups > 0 and influenced == 0` **直接报错**，不再静默产出空壳 |
| `mesh_io.export_mesh` | `dropped` 原来写在顶点循环**内部**（每轮重置 → 永远 0），提到循环外；返回值新增 `influenced` / `groups` |

**新增 `mesh_io.reuse_rig()`**：把 Armature 修改器指向真正含这些骨骼的骨架（场景里已有 `c_VepleySSR01`（163 根）就直接复用；没有才按骨骼表新建）。导出不读修改器，改它只为让视口说真话 —— 用户正是被"转骨骼模型不动"误导成"没绑定"。

**修复验证**（桌面 `维普蕾.blend`，headless）：

| | 修复前 | 修复后 |
| --- | --- | --- |
| 权重行和 | 0.0000 | **1.0000** |
| 有效顶点 | **0 / 12734** | **12734 / 12734** |
| `bone_hashes` | 0 | **126** |
| `bindpose` | 无 | **(126,4,4)** |
| 参与骨骼槽 | 0 | **96** |
| 修改器指向 | `Sally_causal_2.0_arm`（0 根） | **`c_VepleySSR01`（163 根）** |

`dropped_influences=1777` 属正常：MMD 模型很多顶点带 4 根以上骨骼影响，游戏只吃 4 根。

**装回游戏后从游戏目录的包里解顶点流复验**：

```
470d4627 c_VepleySSR01_slg_cloth_lod0  verts=12734  weight rowsum=[1.0000,1.0000]  zero_rows=0     bone_table=126  OK
65c50de0 c_VepleySSR01_slg_cloth_lod0  verts=12734  weight rowsum=[1.0000,1.0000]  zero_rows=0     bone_table=126  OK
2d3d2990 c_VepleySSR01_slg_cloth_lod1  verts=12734  weight rowsum=[0.0000,1.0000]  zero_rows=1954  bone_table=124  WARN
```

**新事实（比 UV/可读性分裂更狠，必须记）——同一网格在不同包里烘焙成两套骨骼表：**

| 网格 | 副本 | 骨骼表 | 差异 | 按哈希重映射后 |
| --- | --- | --- | --- | --- |
| `cloth_lod0` | `470d4627` / `65c50de0` | 126 | 与 patch 全等 | 权重 100% 保留 |
| `cloth_lod0` | `872f1c4f` | 126 | 与 patch **差 15 根** | 保留 69.6%，**1954 顶点（15.3%）全影响丢失 → 塌到原点** |
| `cloth_lod1` | `470d4627` / `fc7bed44` | 124 | 与 patch 全等 | 权重 100% 保留 |
| `cloth_lod1` | `2d3d2990` | 124 | 与 patch **差 15 根** | 同上 |

这 15 根骨的差异**不是骨骼不同，是层级重组**（用 `bonenames.json` 解出全路径）：

```
旧表: root/Root_M/Hip_M/Hip_R/HipPart1_R/Knee_R/...        （Hip_M 是 Hip_R/Hip_L 的父）
新表: root/Root_M/Hip_R/HipPart1_R/Knee_R/...              （Hip_M 被删，Hip_R/Hip_L 直接挂 Root_M）
```

短名（`Hip_R`/`Knee_L`/`Toe_L`…）相同，但**按短名匹配反而更差（60.0% < 69.6%）**——校验时不要想当然用短名兜底。
丢失权重高度集中在 `Hip_M`（1809/3871）与腿部链 → 成衣下半身。
**当前处理**：优先保证 4 个匹配副本 100%，`872f1c4f` / `2d3d2990` 暂保留 WARN（若游戏加载到它们，需针对该副本的表另出一份 patch）。

### 2.17 材质槽 / 顶点缓冲布局 / 骨骼表代际（2026-10-19 第二轮）

用户看着游戏截图问："是不是我的手臂没绑定对位置？"——**手臂和权重都没问题**，
是另外两件事叠在一起。

#### (1) 材质槽决定 Unity 画几个子网格（这才是手臂消失的原因）

导出的子网格编号 = 源模型的材质槽号。**渲染器的材质列表属于被替换的那个游戏网格**，
所以材质索引解析不到的子网格 Unity 直接不画——而 Blender 里一切正常，极具迷惑性。

| 对象 | 材质槽 | 结果 |
| --- | --- | --- |
| Sally PMX | 20 个（面落在 `[0,1,3,4,5,6,16,19]`） | 导出成 8 个子网格，索引 5/8/16/19… |
| GF2 `c_VepleySSR01_slg_cloth_lod0` | **4 个** | 索引 ≥4 的子网格全部不画 |

`KEPT (idx 0-3)` 7765 面 vs `DROPPED (idx ≥4)` **6803 面**；手臂/手区域 3011 面里
**2295 面在 ≥4 的槽位**（`Glove` 槽 4、`Skin` 槽 6）→ 游戏里两条前臂和双手消失，
而槽 3 的 `Bracelets` 活着 → **手环浮在空中**（和用户截图完全一致）。

**修法**：导出时把源模型的槽**合并到目标网格的槽位数**（`export_mesh(material_slots=N)`，
N 由新命令 `exmt_core matinfo <bundle> <mesh>` 读出，它返回目标网格的子网格数）。
按面数凑成 N 组，剩下的面一分为二没有损失。实测 8 槽 → 3 子网格、**14568 面一个不少**。

#### (2) `m_DataSize` 里是**三个连续流**，不是交错块

```
stream 0: count x 40   position(12) + normal(12) + tangent(16)
stream 1: count x 20   colour(16) + uv0(4)
stream 2: count x 32   weights(16) + bones(16)
          总长 = count x 92   ← 和 m_DataSize 一致
```

每个顶点在 **stream 0 内**的步长是 **40**，顶点 i 的位置在 `i * 40`——**不是** `i * 92`
（`len(m_DataSize) / vertex_count = 92` 是三个流之和，非常容易被当成 stride）。
踩坑记录：按 92 索引只会命中顶点 0，表现为"y 最小值变了、最大值没变、x/z 一点没动"，
即**只有第 1 个顶点被缩放**，剩下 2282 个原样——身体只塌了一半。
判据：用**解码后的顶点位置去 `raw.find()` 字节**，连续偏移的差值才是真 stride（实测 40）。

用途：把某个网格"隐藏"掉（缩到原点），保持拓扑/权重/索引全不变，只动 stream 0。

#### (3) "两套骨骼"其实是同一副骨架的两个版本

用户问：既然有两套骨骼，为什么「开始制作」只导出了一套？

答：工作空间导出的那**一副**骨架（163 根）是**各网格骨骼表的并集**，这是对的；
"两套"来自**游戏自己的包处于两个代际**，不是导出产生的。

| 代际 | `c_VepleySSR01_slg_cloth_lod0` 的骨路径 |
| --- | --- |
| 旧（`470d4627` / `65c50de0`） | `root/Root_M/`**`Hip_M`**`/Hip_R/HipPart1_R/Knee_R/...` |
| 新（`872f1c4f` / `2d3d2990`） | `root/Root_M/Hip_R/HipPart1_R/Knee_R/...`（`Hip_M` 被删，腿直接挂 `Root_M`） |

126 根里 **123 根的短名完全相同**，只有 3 个短名不同（`Hip_M` / `foot_*_Ctrl4_Jnt_skin` / `Root_M`）；
**12 个短名各对应两个哈希**，全是腿链（`Hip/Knee/Ankle/Toes/HipPart1/KneePart1` 的左右）。

**修法**：`modpack._remap_bones` 由"只按哈希"改成"**按哈希优先、短名兜底**"
（名字来自 `bonenames.json`，按 `"/"` 取末段）：

| 策略 | 保留权重 | 无驱动顶点 |
| --- | --- | --- |
| 只按哈希 | 69.6% | **1954 / 12734** |
| 哈希 + 短名兜底 | **85.8%** | **0** |

实测效果（装进游戏后的包）：**全部 6 个包的 `zero_weight` 都是 0**；
旧代际那两份权重和 `[0.077, 1.000]`（= 只剩 `Hip_M` 一根真被删的骨）。
⚠️ 早期我做过一次"只用短名"的测试，得到 60.0%（比 69.6% 还差）——那是**没有哈希优先**，
不能作为"短名无效"的依据；顺序必须是**先哈希、后短名**。

### 2.18 蓝图三个 UI bug + 贴图预览（2026-10-19 第三轮）

用户报「分组节点点加槽位报错」「不知道加槽位干什么」「生成 Mod 节点上没有生成 Mod 按钮」
「贴图节点为什么不显示预览」。四个都是真的，根因三个：

#### (1) 死操作符 / 死方法调用

| 按钮 | 代码写的 | 真相 |
| --- | --- | --- |
| `＋ 槽位` | `node.sync_slots()` | 方法早改名 `ensure_slots` → `AttributeError` |
| `生成 Mod` | `exmt.blueprint_generate` | **该 idname 从未注册**（真名 `exmt.generate_mod`）→ 点了没反应也不报错 |

后一个是**静默失败**，比报错更难发现：按钮画得出来，点下去什么都不发生。

#### (2) 假警告

生成节点原逻辑用 `_source_submesh()` 判断"这条线有没有指定 Submesh"，而它只认
**直接连过来的** `物体` 节点；用户实际是 `物体 → 分组 → 生成 Mod`，于是显示
「1 条连线没指定 Submesh」——**连线完全正确**。新增 `socket_objects()` / `socket_ok()`
穿透分组，误报消失。

**判据**：任何"检查/警告"必须和**真正干活的那条解析路径**用同一个函数，
否则警告骗的是自己。

#### (3) 贴图没有预览

`workspace.json` 的 `textures[].file` 已经指向解好的 PNG（13 张，512²/2048² 都在），
所以节点可以直接画原图：

- `texture_preview(tree, name)` → `bpy.data.images.load(path, check_existing=True)`
- 节点里 `layout.template_preview(image)`；失败降级为文字，**绝不让节点崩**
- 标签加**用途**：按后缀判 `_rmo` 粗糙/金属/遮挡、`_da`/`_dt` 漫反射+透明、`_d` 漫反射、
  `_n`/`_nt` 法线、`_bt`/`_e` 自发光
- 下拉条目 label 仍保持**纯 ASCII**（中文放 description），避开 §4 那条枚举乱码

#### 验证手法（可复用）

真实 `UILayout` 只在绘制时存在，headless 拿不到。写一个 `FakeLayout`
（`label/prop/row/column/box/operator/template_ID/template_preview`）直接调
`draw_buttons()`，`operator()` 返回可赋属性的 `FakeOp`，赋值时用
`bpy.ops.mod.fn.get_rna_type()` 校验属性名真存在。

实测一轮抓到 3 个 bug，最终 **25 个节点（1 生成 + 1 分组 + 10 物体 + 13 贴图）0 失败**，
13 个贴图节点全部画出真实预览（`c_VepleySSR01_slg_cloth01_d.png` 2048² 等）。

⚠️ `bpy.types.XXX.bl_rna.properties` **不列操作符的自定义属性**，
必须用 `bpy.ops.<mod>.<fn>.get_rna_type().properties`；`bpy.ops.x.y` 是函数包装器，
**不能直接赋值**（会 `AttributeError: 'BPyOpFunction' object has no attribute ...`）——
那是测试方法错，不是属性没注册。

### 2.19 动作（AnimationClip）：数据在哪、什么格式、能不能对上骨架（2026-10-20 第一轮）

用户诉求：点立绘 → 模型页能查到该皮肤的动作 → 「开始制作」把动作一起导出 → Blender 导入后播动作看蒙皮。

**结论：之前完全没做（核心/CLI/前端/插件里 0 处 animation 代码）；但地基已探明，能做，且解码器已写好并自证。**

| 事实 | 证据 |
| --- | --- |
| 动作在 bundle 里，且**命名跟着皮肤前缀**（和网格/贴图同一套规则）：`c_<皮肤>_slg_<动作>[_N]` | 643 个备份包里扫出 **224 个 AnimationClip**（9 个包含动作）；`ad1560ee…` 一个包 146 个，`c9150108…` 67 个 |
| 备份的 643 包里只有 GrozaSR0102 有整套动作（144 个）；游戏目录里其实有 **9619 个包 / 66.3 GB**，动作包大多没进备份 | `C:\EXMTCacheFolder\game\bundles` 643 vs `LocalCache\Data\AssetBundles_Windows` 9619 |
| 同一包里还有 `Avatar` 52 / `Animator` 51 / `AnimatorController` 6（含 `GrozaSR0102`、`GrozaSR0102_N`、`GrozaSR0102_Gen`） | 同一次扫描 |
| 动作是**通用（generic）Transform 曲线**，不是 humanoid 肌肉：`path = crc32(变换路径)`，`attribute` 1=position(3 条曲线) 2=rotation(4) 3=scale(3) 4=euler(3)，其余绑定各 1 条 | `m_ClipBindingConstant.genericBindings`；AssetStudio `AnimationClipBindingConstant.FindBinding` 同款定义 |
| 曲线是「一个扁平索引空间」：`[0, streamedCurveCount)` 走 streamed（稀疏三次键 `((a·dt+b)·dt+c)·dt+d`）、接着 `denseCurveCount` 条走 dense（帧优先）、剩下走 constant | **算术自证**：`Σ绑定曲线数 = 2455 = streamed 106 + dense 623 + constant 1726`（Groza `LowCoverRtoL_N`，一分不差） |
| 解码正确性 | 258 根骨 × 61 帧，**四元数模长与 1 最大差 3.2e-4**；scale 基本为 1（个别 0.61/3.06 的挤压骨） |
| **动作能直接驱动工作空间骨架** | 同一包里 13 个网格的骨骼表与动作路径哈希**逐骨 100% 命中**（hair 51/51、cloth1_lod1 136/136、Weapon 21/21、face 3/3 …） |
| 骨骼名可读度 | 258 条路径里 **126 条**已被 `bonenames.json` 解成 `root/Root_M/Spine1_M/…`（其余是武器/未扫到 prefab 的链，哈希照样能对上骨骼，不影响播放） |
| 根位移在 `root/Root_M` 骨上（`path == 0` 的绑定是 Animator 属性，不是 Transform） | 解码 x/z 位移与 clip 自带的 `m_StartX/m_StopX` 同向同量级；差值与 Root_M 的 bind 高度 0.36 一致 |
| 动作**不参与打包**（Mod 只换网格/贴图）→ 与「未连线物体=参考」同一性质，导出后只用于看蒙皮 | 现有 `GeneratedMod` 流程只吃网格 + 贴图 |

已落地：`core/exmt_core/anim.py`（`bindings_of` / `Clip.value` / `Clip.tracks` / `export` / `hierarchy` / `open_clips` / `clip_of`）
+ `core/exmt_core/animidx.py`（两遍法全库索引）+ CLI `anims` / `animexport` / `animidx`
+ 前端模型页底部「动作」栏（懒加载、搜索、勾选）+ 工作空间导出 `animations/*.exmtanim`
+ Blender 插件 0.7.0：导入工作空间时把每个动作做成一个 Action（muted NLA 轨、第一个设为当前）。
验证脚本留在 `C:\DevCache\anim-recon\`（scan_anims.py / validate_anim.py / crosscheck2.py / ws_test.py / workspace_anim_test.py）。

### 2.20 动作第二轮：全库索引、层级、导出、Blender 真播（2026-10-20 第二轮）

**结论：整条链路打通，实机验证过「点立绘 → 查到动作 → 勾选 → 开始制作 → Blender 里播」。**

| 事实 | 证据 |
| --- | --- |
| 全库动作索引 63 秒建完 | `python -m exmt_core animidx <游戏目录> <out>` → 9613 个包 → **39974 个 clip / 2675 个动画包**；索引 3.5 MB，落在 `<cache>\animindex.json` |
| 覆盖到皮肤级 | catalog 240 个皮肤里 **226 个**能查到动作（Vepley 168、Alka 170、Clukay 176…）；查不到的 14 个是指挥官/变体 |
| 第一遍为什么能这么快 | 只解包内 SerializedFile 的**元数据**（60 MB 的包 86 KB）读 type list，看有没有 class id 74；**不能把截断切片喂 UnityPy**（`check_file_type` 拒收 + 急切解析 AssetBundle 对象 → 静默 0 个对象或 EOFError） |
| 动作的父子层级从同包 prefab 的 Transform 树恢复 | 634 Transform + 634 GameObject → 路径后缀哈希 → **258 根骨里 257 根拿到父**、258 根拿到名字；只登记完整路径时覆盖率 0% |
| 假根会毁掉整段动作 | 后缀裁剪深度抢登记会让 5 根骨变成假根，200+ 根骨整体位移约 1 m；改成「有父优先 + 最浅裁剪」后归零（父子世界距离中位 0.0000 m、p90 0.0039 m） |
| 同一根骨只给四元数或只给欧拉（不会都给） | 258 个 Transform：180 四元数 / 78 欧拉 / 0 双给；欧拉是度、顺序 Z→X→Y，与 rest 局部比对中位 **0.0°** |
| 导出量级 | 30 Hz 采样 + 5 位小数：1 秒 258 根骨 ≈ **0.5 MB** JSON；4.7 秒 140 帧 |
| Blender 侧验收 | 首帧挂动作 vs 不挂动作：**0 个顶点移动（max 0.0000 m）**；中间帧 4602 顶点移动、max 0.09–0.15 m |
| Blender 5.2 没有 `action.fcurves` | 必须 `slots.new` + `layers.new` + `strips.new(type="KEYFRAME")` + `channelbag(slot).fcurves.new(...)`，并且要同时设 `action_slot` |
| 界面 | 「动作」默认收成一条 26px 的栏（不打扰三栏布局），展开后 168 个名字条 + 搜索 + 全选动作；计数显示 `2/168 个动作` |

未决：① **零权重挂件（脸/武器）播动作时留在原地**——它们在游戏里是挂在某根骨上的刚性挂件，网格权重全 0（face/weapon 实测全 0），看起来像"头飞了"；要给这类网格一个「挂到哪根骨」的约定。② 只勾一个子网格时骨架只有那几根骨（实测 body 32 根），动作只驱动这些骨；需要提示用户"动作要看全套就多勾几个网格"。③ 动作仍然只导出、不打包（设计如此）。④ streamed 三次键的键间 inSlope 修正没做（视觉够用）。


---

## 3. 交付物清单

| 产物 | 位置 |
| --- | --- |
| 应用 | `C:\Dev\EXMT`（`src/` 前端、`src-tauri/` Rust） |
| 核心 | `core/exmt_core/`（CLI：list/meshes/extract/extract-all/bonetable/pack/**modpack**/hide-mesh/subdivide/retarget/copy-texture/set-texture/texindex/bonenames/skinfo/geo/geoset/thumbs/maps/workspace/**texinfo**/**anims**/**animexport**/**animidx**；贴图替换 = `texpatch.py` + 无依赖的 `ddsinfo.py`；动作 = `anim.py` + `animidx.py`） |
| Blender 插件 | `C:\Dev\EXMTBlender\{__init__.py, mesh_io.py, blueprint.py, bin/texconv.exe}`（**0.7.0**：一键导入工作空间 → 集合 + 蓝图（Frame `_original`/`_textures`、物体→分组→生成、贴图替换节点）+ 生成 Mod + **把 `animations/` 做成 Action/NLA**；`C:\Dev\EXMTBlender` 是独立 git 仓库；`%APPDATA%\Blender Foundation\Blender\5.2\scripts\addons\EXMTBlender` 是安装副本，不是仓库） |
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
3. ~~**贴图写入只支持 DXT1**：法线/RMO 是 BC7，换贴图需要 BC7 编码器（或整段替换同尺寸 BC7）。~~
   → **已解决（2026-10-19，见 §2.14）**：内置 texconv，任选目标格式；BC1 端到端 42.17 dB。
   剩余：BC7 的插件全链路没实测；BC6H/BC4/BC5 未试。
10. ~~**重新导入会删掉同名蓝图 + 攒重复物体**~~ → **已修（2026-10-19，见 §2.15）**：同名蓝图复用，
    `_original` Frame 内的节点重新指向新网格，导入前清理上一批网格（含数据块）与空骨架；实测无 `.001`。
11. ~~**贴图只打一个 bundle**~~ → **已修（2026-10-19，见 §2.15）**：`meshidx` 同时索引贴图，
    工作空间写 `texture_bundles`，每个拷贝各打一行；合成测试两个包都打上。
12. ~~**生成时界面卡死**~~ → **已修（2026-10-19，见 §2.15）**：编码缓存 + 并行 texconv + modal 分步，
    12 张贴图冷启动 34.9 s / 热缓存 9.1 s，进度画在生成节点上，Esc 可取消。
13. **贴图的"用不用"没有连线语义**：物体是"连上才打包"，贴图是"选了图就打包"。
    两套规则并存容易记混；要么给贴图节点也接进图，要么在 UI 上把"清空图片 = 不换"写死。
    **2026-10-19 决定保留**：接进图意味着生成节点要多 12 根线（刚把网格侧收成 1 根），
    而且贴图节点上已经写着"未选择替换图（不替换）"，规则可读。真要改就等用户提。
4. **导入不做轴向转换**：游戏是 Y-up，导入 Blender（Z-up）后模型沿 +Y 躺着（bbox 实测高 1.589 在 Y 轴）。要做就得同时改导出端（`matrix_world` 会把旋转烘进顶点），本轮没动。
5. **设置页右侧 about 面板**：已实现（7:3 + sticky），但截图里没确认渲染，需要用干净窗口复验。
6. **文档站链接**：`SettingsView` 的 `LINKS` 仍是占位（本地 5200 / GitHub 仓库）。
7. **文档站旧页**：`faq.md`（“导出的 glTF 没有贴图”）、`reference/layout.md`（`gltf.py`）、`guide/index.md`、`reference/cli.md`（`glb` 命令）、`install.md`（点「导出 glTF」）仍写着已删除的 glTF 流程，需要一轮清理。
8. **`C:\Dev\EXMTDoc` 没有 remote**：只能本地 commit，push 需要用户提供仓库地址。
9. ~~**同名网格的多份拷贝只打了一份**~~ → **已修（2026-10-19，见 §2.12）**：`meshidx` 建 `网格名 → 所有包`
   索引（13.5 s / 643 个包），工作空间写 `mesh_bundles`，生成 Mod 时每个拷贝都打；跨包差异用"目标包的 tree 为底 + 按哈希重映射骨骼"处理。

5. **多副本骨骼表分裂（§2.16 发现）**：`c_VepleySSR01_slg_cloth_lod0/lod1` 各有 3 份拷贝，
   其中 `872f1c4f93f07d6ba65b103ae5d1a337`（lod0）与 `2d3d29900ee567995b28c489c4654abd`（lod1）
   的骨骼表与另两份差 15 根（`Hip_M` 被删、层级重组）。
   现状：4 份权重 100%，这 2 份 69.6%、1954 顶点塌到原点。
   待定：① 先确认游戏实际加载哪一份；② 要全覆盖就得**按副本的表各导一份 patch**
   （核心 `patch_mesh` 目前不支持一个 mesh 一份多表 patch）。

14. **旧代际包的残余权重（§2.17）**：`872f1c4f` / `2d3d2990` 这两份的权重和仍低到 `0.077`
    ——真被删掉的 `Hip_M`（1809/3871 的权重质量）无处可去，只能丢。
    要彻底解决得做"父级回退"（把它挂到 `Root_M`），代价是裙摆会跟着根骨动。
15. **材质合并不保真**：8 个槽压到 3~4 个，部位会用错材质（贴图/颜色不对）。
    根因是 GF2 每个网格只给固定数量的材质，没有别的办法；要保真只能改渲染器。
16. **`body` 塌陷是"隐藏"，不是"删除"**：这次用"顶点缩到原点"（extent 0）盖掉了游戏自带的
    头/双臂/脚，代价是它仍在渲染管线里（只是没有面积）。等有更好的做法再换。

17. **被覆盖的 basebody 记录（2026-10-19）**：用户 10-18 装过
    c_VepleySSR01-cloth-basebody（指向 872f1c4f，但 mods 目录里**只有指针、没有补丁文件**，
    "source": "basebody"）。本轮重打包时把 872f1c4f 整个覆盖了 → 那个素体身体没了。
    教训：**basebody 类 mod 只存指针、不存数据，被其他包覆盖后无法重建**。
    要么让 App 把 basebody 也录成补丁，要么安装前检查“该包是否已被其他 mod 占用”并拒绝。

---

## 5. 本轮之后的检查点

- 每次改 `AGENTS.md` / `docs` / skill：在本文件顶部更新"最近更新"。
  （写回位置：规则 → `AGENTS.md`；环境/质量门 → `docs/agents-playbook.md`；
  UI 与偏好 → `docs/agents-ui-spec.md`；逐条纠正 → `docs/agents-corrections.md`；通用手法 → `.agents/skills/`）
- 每次得到新的"游戏事实"：追加到 §2，带证据来源。
- 每次解决/新增未决：更新 §4。
