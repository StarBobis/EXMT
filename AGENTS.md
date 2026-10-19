# AGENTS.md

> 这份文件是**给未来的模型看的操作手册**，不是备忘。
> 目标是让在这个仓库里工作的模型**越干越快、越干越准**：每次犯错变成一条可执行约束，
> 每次学到的手法变成可复用资产。**一轮对话结束后如果这份文件、`docs/`、`.agents/skills/` 一点没变，
> 那这一轮就没有产生复利。**

---

## 0. 仓库地图（先看这里，别重新摸一遍）

| 位置 | 是什么 | 怎么跑 |
| --- | --- | --- |
| `src/` | 前端（Vue 3 + TS + Vite）；视图 `src/views/`，样式 `src/styles/` | `bun run dev` |
| `src-tauri/` | Rust 外壳（Tauri 2），命令 `run_core` / `show_main_window` | `bun run tauri dev` |
| `core/exmt_core/` | Python 核心：`bundle.py` `sf.py` `mesh.py` `interchange.py` `textures.py` `preview.py` `bc1.py` `retarget.py` `subdivide.py` `cli.py` | `python -m exmt_core <cmd>`（cwd 必须在 `core`） |
| `C:\Dev\EXMTBlender\` | Blender 插件：导入/导出 `.exmtmesh`、传权重 | Blender → 偏好设置 → 安装 `__init__.py` |
| `C:\Dev\EXMTDoc\` | VitePress 2 文档站（**独立 git 仓库**） | `bun run build` / `bun run preview` |
| `docs/` | **台账**：结论 + 证据 + 未决问题 | — |
| `.agents/skills/` | **能力**：换目标也能用的判据/手法 | — |
| `C:\DevCache\<任务名>\` | 一次性脚本、临时产物 | — |

**本机工具链（已验证）**

- 应用：`bun run tauri dev`，窗口标题 `EXMT Modbox`
- 核心 Python：`C:\Users\Administrator\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe`
  （UnityPy 1.25 / lz4 / numpy / PIL / texture2ddecoder）
- Blender：`C:\Program Files\Blender Foundation\Blender 5.2\blender.exe`（headless：`--background --factory-startup --python x.py`）
- 游戏：`C:\GF2Exilium\GF2 Game`，资源在 `GF2_Exilium_Data\LocalCache\Data\AssetBundles_Windows\`
- 窗口驱动/截图：`C:\DevCache\gf2\app2.ps1 click <cx> <cy>` / `app2.ps1 shot [-out p.png]`
  （**DPI-aware**：先把窗口摆到 (8,8)；点击坐标 = 截图物理像素 − client 原点。老的 `app.ps1` 是 DPI-unaware 的，150% 缩放下会错位、窗口比屏幕大时右侧截不到）

---

## 1. 每轮必须跑的 RSI 循环

> 用户要的是「越用越好用、越懂我、越熟练」。这只能靠**每轮结束强制写回**，不能靠"最后总结"。

### 1.1 回答用户前的三问（各 10 秒）

1. **这轮被用户纠正了吗？** → 有：写进 §4 纠正台账，并在 §5 或 skill 落一条可执行约束。
2. **这轮得到"换目标也能用"的判据/手法了吗？** → 有：进 `.agents/skills/<skill>/references/`。
3. **这轮推进了某个目标的状态吗（新结论 / 新证据 / 新未知）？** → 有：更新 `docs/<目标>-dossier.md`。

三问全否 **允许不写**，但必须在回答末尾写清"三问全否 + 具体原因"（例如"本轮只做了文案微调，无新判据"）。
**禁止连续两轮写"无新增"**——那说明三问没做。

### 1.2 写哪里：按复用半径决定

| 复用半径 | 写哪 | 例 |
| --- | --- | --- |
| 只对本项目有意义 | `docs/<目标>-dossier.md` | 维普蕾的 bundle 哈希、哪个槽几根骨 |
| 换目标也用得上 | `.agents/skills/<skill>/references/*.md` | `peek_name()` 批量提贴图；UnityFS 只加密前 32KB |
| 用户偏好 / 协作方式 | 本文件 §3–§5 | 不要多余按钮、不要日志面板、布局要 WinUI |

### 1.3 交付前固定动作

```powershell
bun run check     # 见 §6
```

- [ ] `bun run check` 通过
- [ ] 有可见变更 → 截图（`app.ps1 shot`），并说明截图对应哪次改动
- [ ] `docs/ledger.md` 已更新（结论 / 证据 / 未决问题）
- [ ] 本轮新判据已进 skill，或明确说明无需
- [ ] 两个仓库都 commit + push
- [ ] 回答末尾一行「沉淀」声明

---

## 2. 硬规则（都违反过，写下来别再犯）

1. 改完自行 **commit + push**；`C:\Dev\EXMTDoc` 是独立仓库，动过也要提交
   （**该仓库没有配 remote**，只能本地 commit；要 push 得先让用户给仓库地址）。
2. **能用现成的库就别造轮子**：
   - 读贴图名用 UnityPy 的 `o.peek_name()`，不要 `read_typetree()` 全解析（实测 15 分钟 → 41 秒）
   - 解码用 `texture2ddecoder`，别手写 BC1/BC7
3. 一次性脚本 / 临时产物 → `C:\DevCache\<任务名>\`，不进仓库根目录。
4. **不要用 pwsh 改文本文件**（BOM/GBK 往返会把 `.vue` 写坏）。
   需要写文件时用编辑工具，或 `[System.IO.File]::WriteAllText($p,$t,(New-Object System.Text.UTF8Encoding($false)))`。
5. **`.exmtmesh` 是唯一交换格式**。glTF 承载不了骨骼哈希、每骨 AABB、碰撞体、通道布局，已删除；
   任何"导入/导出"需求都走自定义格式，必须能**无损往返**。
6. **别在没有证据时说"好了"**：布局给截图、游戏内给截图、写文件给 diff/回读。
7. 目标没达成**不要标 complete**；同一阻塞连续 3 轮才谈 blocked。
8. 改 UI 之前先想"用户会不会嫌多"——**默认更少**。

---

## 3. 视觉与交互规范（用户反复纠正过的）

**基准：WinUI 3 / Fluent。**

| 区域 | 规则 |
| --- | --- |
| 侧边导航 | NavigationView：展开 272px / 折叠 48px；`☰` 在面板头部，与图标共用 40×40 盒子、`2px auto`；折叠时图标居中、文字淡出；状态存 localStorage |
| 窗口 | 初始尺寸**按显示器工作区算**，不写死：`16:9`（1920×1080 的比例），宽度 = 工作区 2/3，高度 = 宽×9/16，上限高 90%，下限 920×600，居中于工作区（不是整屏，避开任务栏）。见 `src-tauri/src/lib.rs` 的 `default_window_size` |
| 焦点 | **鼠标点选不出现蓝色 focus 框**：`.nav-item:focus, :focus-visible { outline: none }`，改用 `--fill-subtle-hover` 填充；键盘用户靠填充感知 |
| 页面 | **full-bleed**：`.page { max-width: none; margin: 0; padding: 18px 24px }`。别用 `max-width + margin:auto`，宽窗口留死白、超宽把内容推出屏幕 |
| 页面分工 | **首页 = 角色列表 + 立绘卡牌**（唯一选皮肤入口，点卡选中，「进入模型页」进工作台）；**模型页 = 子网格 / 关联贴图 / 9:16 竖版 3D**（顶栏只放当前角色·皮肤、数量、工作空间下拉、「换皮肤」「开始制作」）。点左侧子网格才在中间显示它关联的贴图；3D 固定在最右、**9:16 竖版**，人物站立时占满高度。皮肤选择走 `src/lib/selection.ts` 共享，别在两个页面各放一套角色列表/立绘；模型页 `onActivated` 才加载，避免首页连点时后台解码 |
| 栅格 | 用 `minmax(0, Nfr)`；裸 `1fr` / `70% 30%` 会被子元素 min-content 顶宽，产生横向溢出 |
| 卡片 | 同一列表**尺寸必须一致**；缺图条目也占一张同尺寸卡（斜纹底 + 占位图 + 角标），不要降级成小 chip |
| 图片 | 立绘 `object-fit: contain`（保留整身）；头像/胸像用 `contain` + 顶部对齐；不要 `cover` 到把脸裁成特写。**立绘卡片的宽高比必须等于立绘本身（380:752）**，靠宽度定高度——固定高度会把卡压瘦、立绘缩在中间。立绘区**永远一行**：当前卡居中，其余按环形距离（`(i-active+n/2)%n-n/2`）叠在左右，点侧卡切换 |
| 文案 | 中文、直白。**不要黑话**（"素体"被否 → 改成"内置素体"仍被否 → 最终删功能）；按钮名要望文生义 |
| 多语言 | 界面文案一律走 `t("key")`（`src/lib/i18n.ts`），**中文是源字典**，`Record<keyof typeof zh, string>` 强制英文补齐同 key；**模板里不许写死中文**。语言选择存 localStorage `exmt.locale`，默认简体中文，设置页「常规 → 语言 / Language」切换 |
| 密度 | 用户偏好**简洁**：操作日志、素体按钮都是被要求删掉的；状态反馈一行足够 |
| 批量 | 模型页顶部要有“全选 / 取消全选”：一次勾上全部子网格 + 全部贴图；全选后自动打开第一个有贴图的子网格；再点清空。默认仍全不勾，全选只是给“想偷懒”的快捷方式 |
| 下拉 | 统一用 `src/components/Dropdown.vue`（自绘 listbox + Teleport + token 配色 + 键盘）；**不要用原生 `<select>`**——Windows/WebView2 的 option 弹出层是系统绘制，暗色主题会白底白字看不清 |

---

## 4. 纠正台账（说过一次的，不许犯第二次）

| 日期 | 纠正 | 落成的规则 |
| --- | --- | --- |
| 2026-10-18 | 替换后模型显示不全（头+手+裙子飘着） | GF2 的 `body` 只有头/手/脚，躯干在 `cloth`；只换 cloth 必然缺身体 |
| 2026-10-18 | 模型炸裂、权重错 | 外部 T-Pose + 游戏 A-Pose 必须做**绑定空间重定向** `v' = Σw(GameBind⁻¹·SourceBind)v`；最近顶点 / 最近骨骼 / 拉普拉斯平滑三种都试过且都错 |
| 2026-10-18 | 卡片重复 | 一张贴图只能给一个皮肤：精确匹配 → `used` 去重 → 才允许退化匹配 |
| 2026-10-18 | 卡片尺寸不齐 | 见 §3「卡片」 |
| 2026-10-18 | 提取立绘太慢 | 见 §2.2 `peek_name()` |
| 2026-10-18 | 导航选中蓝框难看 | 见 §3「焦点」 |
| 2026-10-18 | 导航不能展开/折叠 | 见 §3「侧边导航」 |
| 2026-10-18 | 不要素体按钮和操作日志 | 整体删除（UI + `base-body` 命令 + `fitbody.py` + 内置网格 + 文档页），不留半套 |
| 2026-10-18 | glTF 导出删掉 | 只用自定义格式，见 §2.5 |
| 2026-10-18 | 立绘要长方形大卡片、按角色选皮肤 | 卡牌视图：左角色圆角方块，右立绘胶片条（缺图也占位），点立绘即选皮肤 |
| 2026-10-18 | 设置页要 7:3，右侧图标/版本/简介/按钮 | 见 `SettingsView.vue`：左设置、右 about 卡（sticky），外链集中 `LINKS` |
| 2026-10-18 | 模型页网格/贴图要按「选中的立绘皮肤」，不是它所在 bundle 的全部内容 | 范围 = **皮肤前缀**（`c_<Skin>_`）：该皮肤所有 bundle 里带这个前缀的网格全要，贴图按同一前缀整组取；绝不回退成整个 bundle，也不以 catalog `parts` 为上限（parts 只用来定前缀） |
| 2026-10-18 | 预览只显示一个网格 → 模型不全 | 3D 预览必须把该皮肤的**全部网格**放进同一个场景（绑定姿势）一起画；网格行只切「显示/隐藏」，不做单件替换式预览 |
| 2026-10-19 | 贴图整组铺在底部，看不出“哪个子网格用哪些贴图” | 贴图仍是全皮肤共享资产（数据层整组取），但 UI 按子网格查看：左栏点一行 → 中栏只显示名称前缀匹配的贴图（`cloth` 吃 `cloth01/02`）；匹配不到进“共享贴图”组；勾贴图自动勾当前子网格；导出仍是勾选的那批 |
| 2026-10-19 | 3D 区域是 16:9 横的，人物站立时两侧空 | 3D 容器 `height:100%; aspect-ratio:9/16` 放最右；相机仍按 `maxDim` 取景；三栏顺序固定为 子网格 / 贴图 / 3D |
| 2026-10-19 | “网格”叫法不准确 | 模型页统一叫“子网格”（`models.studio.meshes` = 子网格 / Submeshes）；行点击 = 查看，复选框 = 导出，两者不要混 |
| 2026-10-18 | 贴图被拉成长方形 | 游戏贴图都是正方形：方形框用 `padding-top:100%` 固定 1:1（不依赖 `aspect-ratio`），名称放框外；固定高度 flex 列里的贴图网格必须 `align-items:start; grid-auto-rows:max-content`，否则行高会把卡片压扁并裁掉说明 |
| 2026-10-18 | 「模型 / 选角色 · 导出改 · 装回来」是无效 UI | 页面不要页头标题/副标题；搜索框与角色数量放进角色区顶部（2026-10-19 起该角色区在首页，模型页只留工作台顶栏） |
| 2026-10-18 | 切个立绘就重新解析/解码，缓存目录形同虚设 | 皮肤级**持久缓存**：`<cache>/skins/<skinKey>/{meshes,geometry,thumbs,meta}.json`，签名 = 相关 bundle 的 size+mtime（游戏更新才失效）；内存 LRU 保留最近 4 个皮肤；页面用 `<KeepAlive>` 保状态，别一切页就重建 |
| 2026-10-19 | 点「开始制作」窗口卡死 | Tauri 的**同步** `#[tauri::command]` 跑在主线程，长任务会把窗口冻住；一律写成 `async fn` + `tauri::async_runtime::spawn_blocking`。导出成功后 `openPath(result.dir)` 直接把工作空间打开 |
| 2026-10-19 | 默认全选会导出用户没改的东西 | 模型/贴图**默认全不勾**；勾选 = 只导出这一批。模型行：复选框（导出）+ 眼睛（只切显示）；贴图卡：选中加边框+勾，未选压暗。工作空间写之前先清空 `meshes/` `textures/`，保证目录内容严格等于这次勾选 |
| 2026-10-19 | 3D 视图滚轮不能缩放 | three 的 `enableDamping` 必须**逐帧 `controls.update()`**（rAF），否则阻尼/缩放不结算；WebView2 的 `deltaY` 单位不统一，滚轮**自己处理**（`exp(-delta/400)` 指数步进）并关掉 `enableZoom` 防止双重缩放；另外补 `＋/－`、重置视角、双击复位、百分比提示 |
| 2026-10-19 | 线框模式试过但没生效 | `material.wireframe = true` + `needsUpdate` 后画面不变（three 版本相关），**已删除不留半套**；要再试就换 `WireframeGeometry` / `LineSegments` |
| 2026-10-19 | 3D 预览是灰模，而且灯光平 | `_d` / `_da` 是 diffuse：网格名去掉 `_lodN` 再加 `_d`/`_da` 就是它的贴图（`cloth4` 只有 `_da`，`cloth3` 没有就保持灰）；核心新增 `maps` 命令解 1024px 贴图并缓存 `diffuse.json`。灯光用 ACES + RoomEnvironment + 三点布光（暖主光 / 冷补光 / 强轮廓光）+ 软接触阴影，别再用一盏平行光打天下 |
| 2026-10-19 | 预览里“头发”变成一个人偶 | 不是渲染 bug：游戏包 `cddc7a5b…` 被 `c_AlvaSSR01-hair-basebody` 这个替换 Mod 覆盖过（mtime 20:55，其他包都是 15:04）。恢复 `%APPDATA%\com.exmt.modbox\backups\cddc….bundle` 后头发正常。**先查 bundle 是否被 Mod 覆盖，再怀疑代码** |
| 2026-10-19 | 没有游戏路径也能进模型页，读的可能是 Mod 后的包 | 设置页加游戏路径（手填/浏览/自动查找/打开文件夹）；**没设置或无效时模型/资源库/工具只显示“先设置游戏路径”占位**；设置成功后自动把目录里用到的原始资源包（643 个 / 6.48 GB）备份到 `<cache>\game\bundles`，之后预览、索引只读这份备份；设置页可随时重新备份或恢复原始模型 |
| 2026-10-19 | KeepAlive 里套 `v-if` 导致切页面内容不更新 | `<KeepAlive v-if="…"><component :is="currentView" :key="current" /></KeepAlive>` —— `v-if` 放 KeepAlive 外层，动态组件保留 `:key`；把 `v-if` 写在 KeepAlive 内部会让内容停在第一页 |
| 2026-10-19 | 角色头像区太大、每张卡还堆"几立绘几皮肤" | 头像区两列、44px 头像、列宽 200px，**只留头像+名字**；数量不在这里显示 |
| 2026-10-19 | 立绘卡片太小；改固定高度后立绘反而变瘦 | 卡片宽高比锁死 `380:752`，宽度平分面板（上限 240px、下限 118px），高度由宽度算，一排放不下就换行 |
| 2026-10-19 | 立绘区要像卡牌：只有一行、当前卡在中间、左右叠卡、可左右切换 | deck 轮播：所有卡绝对定位在同一中心，按**环形**距离 `(i-active+n/2)%n-n/2` 给 `translateX(64/96/118%) + scale(0.86/0.74/0.64) + rotateY(±10~20°)`，z-index 随距离递减；点侧卡即切换（`selected` → `openStudio`）；不换行、不横向滚动 |
| 2026-10-19 | 窗口初始尺寸要按 1920×1080 的比例、占屏幕约 2/3、居中 | 16:9 + 工作区宽 2/3 + 居中；一律按工作区比例算，不写死分辨率（见 §3「窗口」） |
| 2026-10-19 | 重启后直接进模型页误显示"先设置游戏路径" | `App.vue` 启动时若存了路径就先 `checkGamePath`，占位只留给真没设置的情况 |
| 2026-10-19 | 首页内容（欢迎语/功能卡/环境表）完全没用；要角色列表+立绘，模型页只留模型/3D/贴图 | 首页删空，改为角色列表 + 立绘卡牌；模型页重排成顶栏 + 子网格/关联贴图/9:16 3D 三栏；皮肤选择经 `selection.ts` 共享、`onActivated` 才加载（见 §3「页面分工」） |
| 2026-10-19 | Blender 没有稳定的“当前工作空间”入口；一个立绘只能有一个工作空间 | App 实时写 `%APPDATA%\com.exmt.modbox\EXMTConfig.json`；每次“开始制作”新建 `<cache>/workspace/<皮肤>__<YYYYMMDD-HHmmss>/`，一个立绘可有多个工作空间且不覆盖旧目录；模型页下拉选工作空间会切到对应立绘并恢复勾选；Blender 面板读 config 一键导入，旧 `current.json` 只在没有 config 时回退 |
| 2026-10-19 | 导入 Blender 后骨骼没绑到模型；「开始制作」导出的骨骼只有一部分 | ① **顶点组名必须和骨骼名逐字相同**，否则 Armature 修改器 0 顶点受驱动（老代码骨骼 `b000` vs 顶点组 `b0`）；② 游戏**每个网格只带自己那张骨骼表**（Alva 12 个网格 3…70 根，并集 226），导出端必须先按哈希求并集，Blender 端只建**一副**骨架。细节见 `docs/ledger.md` §2.10、skill `unity-skeleton-merge.md` |
| 2026-10-19 | 说“修好了”之前没在 Blender 里真跑过 | 骨骼/权重类改动**必须**用真实工作空间数据 headless 验证：① 受驱动顶点数 == 顶点数；② 转一根主链骨骼网格真的跟着动；③ 不改动往返 `weights`/`bones` 逐位相等；④ 打包回 bundle 再解出来与原包全等 |
| 2026-10-19 | 打包保真只看“数组相等”不够 | ① 子网格 `localAABB` 要按**被引用的顶点**算，别用 `pos[min:max+1]`（会把没引用的顶点算进盒体，和游戏自己写的数对不上）；② Blender 会重算法线/切线（实测中位 2°/11°，"没动过也会变"），导入必须设 **custom split normals** + 把切线存成网格属性、导出优先读回；③ 判据用**角度分位数**，不要用裸 float 差（1e-7 和 90° 在 `max abs` 里长得一样）|
| 2026-10-19 | 视图里异步读“依赖别的异步结果”的状态 | `refreshGenerated()` 原本写在 `initWorkspaces()` 开头，那时 `activeWorkspace` 还没恢复 → 按钮永远灰着。**依赖 A 的 B 必须放在 A 之后**：恢复完工作空间再读它的 `GeneratedMod`，`onActivated` 也要补一次 |
| 2026-10-19 | 用 `app.ps1` 截图/点击老是错位、右侧截不到 | 那个脚本是 **DPI-unaware** 的：`GetWindowRect`/`ClientToScreen` 与 `CopyFromScreen` 相差缩放比（本机 150%），窗口比屏幕大时右边根本进不了图。用 DPI-aware 的 `C:\DevCache\gf2\app2.ps1`（先 `SetProcessDpiAwarenessContext`、把窗口摆到 (8,8) 再截）：坐标 = 截图物理像素 − client 原点（本机 (19,10)）。**侧栏是折叠的（48px）**，导航图标在 client x≈27 |
| 2026-10-19 | 连点几下，后面几次总是点空 | ① 消息条一出现会把下面的列表整体下移 → **每次点击前重新截图**，别复用上一次的坐标；② HMR 之后 WebView 里的状态可能是旧的（按钮一直是灰的、点不动）→ 先 `Ctrl+R` 重载再判断，别急着改代码 |
| 2026-10-19 | 以为"多装一个 Mod 不会冲突" | **一个包只能有一份文件**：后装的就是覆盖先装的。运行时状态必须由「原始包 + 所有启用 Mod 的补丁」**重新算一遍**（`applyMods`）；Mod 要能合并就必须留下**补丁文件**（`GeneratedMod/patches/*.exmtmesh`），不能只留打好的包 |
| 2026-10-19 | 同一网格在多个包里，直接照抄 `.exmtmesh` 的整棵树 | 各副本可能**烘焙得不一样**（实测同一网格一个 UV 是 float16 且 `m_IsReadable=false`，另一个 float32 且可读）。只有「通道布局 + 可读标志 + 骨骼哈希表」全同才用文件里的树；否则以**目标包的树为底**，只写几何，并按哈希重映射骨骼索引 |
| 2026-10-19 | 靠"物体上的自定义属性"认身份（`exmt_source`） | 身份放在**两端都稳**的地方：本体导出时给每个网格分配 `index` + `alias`（写进 `workspace.json`），Blender 端用一个**蓝图**（自定义 NodeTree）决定"哪个物体接到哪个 Submesh 槽位"。物体改名/复制/换成自己的模型都不影响；导入按工作空间建一个**集合**，方便整组选中/隐藏/删除 |
| 2026-10-19 | Blender 5.2 里自定义 NodeTree 显示不出来（`ui_type` 直接报"没有这个属性"） | 5.2 移除了 `ui_type`，`edit_tree`/`id` 是只读的。正确写法：`space.tree_type = <bl_idname>`，然后 **`space.node_tree = <tree>`**；4.x 才用 `ui_type`/`edit_tree`。判断依据读 `space.bl_rna.properties[...].is_readonly`，别猜 |
| 2026-10-19 | GUI 里截 Blender 的图老是拍到 splash、或者视图没框好 | `--python` 脚本执行时窗口还没布局完，`node.view_all()` 会空跑 → 用 `bpy.app.timers.register(fn, first_interval=2.0)` 推迟到 UI 稳定后再做；截图前 `SetForegroundWindow` + 发一次 `{ESC}` 关掉 splash |
| 2026-10-19 | 蓝图里"槽位 = Submesh"把含义绑在了**线上**：想加一个"只用来参考、不导出"的模型就没地方放 | 槽位**不带含义**（`网格 1..N`）。打包集合 = 从「生成 Mod」节点出发（可穿过 `分组` 节点）走到的所有**物体节点**，目标 Submesh 取物体节点自己的 `submesh`。**没连线的物体 = 不打包**（这就是参考用法的入口）；`分组` 节点让 12 个物体只占生成节点 1 根线 |
| 2026-10-19 | 导入后一屏节点看不出"哪些是原物体" | 物体节点全部 `parent` 到一个 Frame，名字 `<工作空间名>_original`；贴图节点另一个 Frame `<名>_textures`。**Frame 尺寸要自己按子节点包围盒算**（`NodeFrame.width/height` 可写）：`shrink=True` 只在手动拖动时重新贴合，Python 建出来永远是 160×100，节点会戳出框外 |
| 2026-10-19 | 贴图替换"手写 BC7 编码器" | 别造轮子：内置 **texconv.exe**（`EXMTBlender/bin/`，DirectXTex 2026.5.8.1）到插件里，生成时 `-f <目标格式> -w -h -m <目标 mips>` 转成**目标贴图原本的格式**；核心只写字节（`texpatch.apply`），Mod 里存已编码的 `.bin`，App 合并只搬字节 |
| 2026-10-19 | 贴图改完没生效，包只涨了 140 字节 | UnityPy 的 Texture2D 像素在 typetree 里叫 **`image data`**（带空格）；写 `m_ImageData`/`image_data` 会被当新键**静默丢弃**。改贴图必须写 `tree["image data"]` 并清 `m_StreamData` |
| 2026-10-19 | 替换后的贴图看起来对、一比对全错 | 两个隐形转换：① Unity 的 Texture2D **行序自下而上**，texconv 对 PNG 输出自上而下 → 每个 mip 的块行要整体翻一次（不翻 10.1 dB，翻后 59.2 dB）；② texconv 把 Blender 的 PNG 当 sRGB，输出 UNORM 时会转成线性（31→4、219→181）→ 固定加 `-srgbi -srgbo`（恒等变换）让数字原样落盘 |
| 2026-10-19 | App 里"加载到游戏"用的是页面打开时那份 mod.json | 用户可能在页面加载后又去 Blender 点「生成 Mod」。`loadIntoGame` 必须先 `readGeneratedMod(ws.dir)` **重新读一遍**；`applyMods` 里 `targets` 必须把**只被贴图碰过的包**也算进去（`byBundle` ∪ `texByBundle`），否则那个包整包被漏掉 |
| 2026-10-19 | "重新导入一次"会不会毁掉用户的工作 | **同名蓝图必须复用、不许 remove**：里面有用户连的线和贴图选图。做法是把 `<工作空间>_original` Frame 内的物体节点按 `submesh` **重新指向新导入的网格**，Frame 外的节点一律不动；导入**前**先清理上一批网格（**连 mesh 数据块一起删**，否则名字还被占着，新网格只能叫 `…001`）与没孩子的旧骨架，骨架按骨骼名集合相同复用（姿势也保住） |
| 2026-10-19 | 贴图只打了一个包 | 贴图和网格一样会多拷贝：全库 **3824 个贴图名里 72 个在多个包**（`RampMap_Linear_RGBAHalf` 542 份）。索引一次扫描同时收 Mesh + Texture2D，工作空间写 `texture_bundles`（与 `mesh_bundles` 对称），生成时**每个拷贝各出一行**；索引文件缺新字段要判成过期重建 |
| 2026-10-19 | 生成 Mod 时界面卡死一分钟 | 三步都要：① **编码缓存**（源图路径+mtime+size + 目标格式；`image.is_dirty` 不算命中）；② texconv **并行**（线程池，一张 2048² BC7 = 5 s，12 张 60 s → 35 s）；③ 操作符改 **modal 分步**（`Generation.start()/step()`，每个 tick 收一个结果），进度写状态栏**并画在生成节点的 label 上**（状态栏只有 20 px、截图都拍不到），Esc 取消；`execute()` 保留同步路径给 headless |
| 2026-10-19 | 用 `is` 比较 Blender 的 `node.parent` | **永远用 `==`**：`parent` 每次访问返回新的 Python 包装对象，`node.parent is frame` 会对"自己所属的那个 frame"返回 False（实测 re-import 直接 `min() iterable argument is empty` 崩掉） |
| 2026-10-19 | 枚举字段（`layout.prop(node, "xxx")`）显示成一堆乱码 | **枚举条目的 label/description 里别放非 ASCII**（我用了 `·`）：Blender 5.2 会把整格画成随机字形，而**同一段文字放在 `layout.label()` 里完全正常**——所以别以为是截图/字体问题，先换成 ASCII 空格复测（实测 `"%s  %sx%s"` 立刻正常）。贴图选择器用 `layout.template_ID(node, "image", open="image.open")` 能直接给个浏览按钮 |
---

## 5. 用户偏好速查

- **要结果**：先给可验证产物（截图/日志/文件），再解释。
- **讨厌嘴硬**：不确定就说"没验证"，别用"应该没问题"。
- **讨厌重复劳动**：同样的活第二次要更快（脚本化、库化）。
- **喜欢干净的信息密度**：一屏看全，别堆按钮和面板。
- **术语要接地气**：黑话要么解释、要么换词。
- **中文回答**，技术名词可保留英文。
- 不要不说话光干活，不说话的话不算工作量！实时告诉用户你的进度！！
- 任务完成后必须调用 https://wxpusher.zjiecode.com/api/send/message/SPT_AfgXTrmEG2qVazzIEtkeWWwbKm39/Hello%20WxPusher        给用户发消息！结尾内容可自定义！
  （**只能发 ASCII**：URL 路径里只要出现中文就 400，多短都一样——实测 `蓝图v2+贴图替换做好了` 也被拒，`EXMT: blueprint v2 + texture replace done` 成功。POST `/api/send/message` 用这个 SPT_ token 会回 `appToken不正确`，只能走 GET 路径）
---

## 6. 质量门 `bun run check`

`package.json` 的 `check` 依次执行：

1. `vue-tsc --noEmit`
2. `cargo check`（`src-tauri/`，找不到 cargo 才跳过）——外壳是 Rust，编译坏了不能等到 `tauri dev` 才发现
3. `python -m compileall core/exmt_core`（核心语法）
4. `python -m exmt_core meshes <已知bundle>`（核心冒烟，断言输出含 `c_Vepley`）
5. `vitepress build`（若 `C:\Dev\EXMTDoc` 存在）

任何一步失败都不许说"完成"。

---

## 7. 目标状态放哪

目标、阻塞、未决问题**不放本文件**（会过期），放 `docs/ledger.md`。
开始工作先读它，交付前更新它。
