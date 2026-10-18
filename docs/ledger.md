# 台账 · EXMT Modbox

> 结论 + 证据 + 未决问题。**不写流水账**（不记"先跑 X 再跑 Y"）。
> 每轮开始读这里，结束前更新这里。目标状态只在这里，不在 AGENTS.md。

最近更新：2026-10-19

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
| 网格与贴图**平级**：贴图按皮肤前缀整组取（Alva 20 张），不做"某网格的贴图"过滤；导出也是整套 | 20 张实测；截图里贴图条与网格列表并列 |
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
| 模型页顶部：角色头像区两列、44px 头像、**只留头像+名字**（不再显示"几立绘几皮肤"），列宽 200px；立绘区 = **卡牌轮播**：永远一行，当前卡居中（卡面比例 380:752，1138 逻辑宽窗口下约 193×380），其余按环形距离叠在左右，点侧卡即切换 | 截图 `shot_045148.png`（5 卡）、`shot_045201.png`（切到第 3 张仍是一行） |
| 立绘卡既不能固定高度、也不能 4 张平铺：固定高度把卡压成 138×320（比 0.505 更瘦），4 张平铺又太小 | 用户两次纠正；最终 `aspect-ratio: 380/752` + deck（`translateX 64/96/118%`、`scale .86/.74/.64`、`rotateY ±10~20°`、z-index 递减） |
| 侧卡点击会改 `selected` → 触发 `openStudio`；给按钮加 `@mousedown.prevent` 后，点卡不再把 `.app__content` 滚走（原来焦点会让页面跳 ~80px） | 点击前后截图对比：加之前 header 被滚出视口，加之后停在原位 |
| 启动时若已存游戏路径，`App.vue` 必须先 `checkGamePath`，否则新进程直接点"模型"会误显示"先设置游戏路径"占位（实际路径可用） | 本轮重启后复现，修好后首页直接显示"打开游戏目录" |


---

## 3. 交付物清单

| 产物 | 位置 |
| --- | --- |
| 应用 | `C:\Dev\EXMT`（`src/` 前端、`src-tauri/` Rust） |
| 核心 | `core/exmt_core/`（CLI：list/meshes/extract/extract-all/pack/hide-mesh/subdivide/retarget/copy-texture/set-texture） |
| Blender 插件 | `C:\Dev\EXMTBlender\{__init__.py,mesh_io.py}` |
| 文档站 | `C:\Dev\EXMTDoc`（VitePress 2，独立仓库） |
| 本仓库 docs | `exmt-modbox.md`、`exmt-model-pipeline.md`、`exmt-characters.md`、`gf2-exilium-recon.md` |
| skill | `.agents/skills/unity-il2cpp-game-recon/`（§18–§22）、`.agents/skills/tauri-winui-shell/` |

---

## 4. 未决问题（按优先级）

1. **body 槽独立身体（G5）**
   - 现状：外部 VRoid 身体（T-Pose）装进 body 槽后炸模；三种权重策略都失败（最近顶点 / 最近骨骼 envelope / +拉普拉斯平滑）。
   - 结论：必须做**绑定空间重定向** `v' = Σ w_i (GameBind_i⁻¹ · SourceBind_i) v`。
   - 缺：游戏侧骨骼无名字，需要"人形骨骼链 + 左右对称 + 静止位置"反推对应；或在 Blender 里用约束烘焙。
   - 下一步：在 Blender 中把 VRM 骨架摆成游戏 bind pose → 烘焙 → DataTransfer 权重 → 导出。
2. **骨骼命名**：可用候选词表（`Bip001 Pelvis` / `J_Bip_C_Hips` / `Hips`…）算 CRC32 与 `m_BoneNameHashes` 对撞。
3. **贴图写入只支持 DXT1**：法线/RMO 是 BC7，换贴图需要 BC7 编码器（或整段替换同尺寸 BC7）。
4. **设置页右侧 about 面板**：已实现（7:3 + sticky），但截图里没确认渲染，需要用干净窗口复验。
5. **文档站链接**：`SettingsView` 的 `LINKS` 仍是占位（本地 5200 / GitHub 仓库）。

---

## 5. 本轮之后的检查点

- 每次改 `AGENTS.md` / `docs` / skill：在本文件顶部更新"最近更新"。
- 每次得到新的"游戏事实"：追加到 §2，带证据来源。
- 每次解决/新增未决：更新 §4。
