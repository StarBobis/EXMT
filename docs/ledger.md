# 台账 · EXMT Modbox

> 结论 + 证据 + 未决问题。**不写流水账**（不记"先跑 X 再跑 Y"）。
> 每轮开始读这里，结束前更新这里。目标状态只在这里，不在 AGENTS.md。

最近更新：2026-10-18

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
| 默认窗口 = 工作区宽 72%、16:10、上限高 88%，下限 920×600 → 本机 1229×768 逻辑（居中物理 347,182） | 实测 `GetWindowRect` 1866×1165 / client 1844×1152，右/左留白各 347、上/下各 181 |
| `set_size(LogicalSize)` 设的是**客户区**（tao inner size），无边框窗口外面还有 ~22×13 物理像素不可见边框 | client 1844×1152 恰好 = 1229×768×1.5 |
| 居中要用 `outer_size()`，用客户区居中会左右各差 11px | 旧写法 (358,188) → 新写法 (347,182) 左右对称 |
| pwsh 是 DPI 不感知进程，`GetWindowRect` 返回**虚拟化**坐标；量真实像素要先 `SetThreadDpiAwarenessContext(-4)` | 同一窗口两种读法差 1.5 倍 |

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
