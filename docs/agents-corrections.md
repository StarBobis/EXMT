# 纠正台账（说过的，不许犯第二次）

> 这是**流水账**，只追加、不重写历史（写错了就在下面补一条订正，别删旧行）。
> 每条纠正都必须落成一条**可执行规则**：追加到本表，同时把结论折进
> `AGENTS.md`（铁律）或 `docs/agents-ui-spec.md`（UI / 偏好）或 `.agents/skills/`（通用手法）。
> 只写"以后注意"而不落规则的条目 = 没写完。
>
> 规则冲突时：**本表的下面的行 > 上面的行 > 旧规则**（代码/事实类以 `docs/ledger.md` 的最新结论为准）。

| 日期 | 纠正 | 落成的规则 |
| --- | --- | --- |
| 2026-10-18 | 替换后模型显示不全（头+手+裙子飘着） | GF2 的 `body` 只有头/手/脚，躯干在 `cloth`；只换 cloth 必然缺身体 |
| 2026-10-18 | 模型炸裂、权重错 | 外部 T-Pose + 游戏 A-Pose 必须做**绑定空间重定向** `v' = Σw(GameBind⁻¹·SourceBind)v`；最近顶点 / 最近骨骼 / 拉普拉斯平滑三种都试过且都错 |
| 2026-10-18 | 卡片重复 | 一张贴图只能给一个皮肤：精确匹配 → `used` 去重 → 才允许退化匹配 |
| 2026-10-18 | 卡片尺寸不齐 | 见 `agents-ui-spec.md`「卡片」 |
| 2026-10-18 | 提取立绘太慢 | 见 `agents-playbook.md` §5 `peek_name()` |
| 2026-10-18 | 导航选中蓝框难看 | 见 `agents-ui-spec.md`「焦点」 |
| 2026-10-18 | 导航不能展开/折叠 | 见 `agents-ui-spec.md`「侧边导航」 |
| 2026-10-18 | 不要素体按钮和操作日志 | 整体删除（UI + `base-body` 命令 + `fitbody.py` + 内置网格 + 文档页），不留半套 |
| 2026-10-18 | glTF 导出删掉 | 只用自定义格式，见 `AGENTS.md` 铁律 6 |
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
| 2026-10-19 | 窗口初始尺寸要按 1920×1080 的比例、占屏幕约 2/3、居中 | 16:9 + 工作区宽 2/3 + 居中；一律按工作区比例算，不写死分辨率（见 `agents-ui-spec.md`「窗口」） |
| 2026-10-19 | 重启后直接进模型页误显示"先设置游戏路径" | `App.vue` 启动时若存了路径就先 `checkGamePath`，占位只留给真没设置的情况 |
| 2026-10-19 | 首页内容（欢迎语/功能卡/环境表）完全没用；要角色列表+立绘，模型页只留模型/3D/贴图 | 首页删空，改为角色列表 + 立绘卡牌；模型页重排成顶栏 + 子网格/关联贴图/9:16 3D 三栏；皮肤选择经 `selection.ts` 共享、`onActivated` 才加载（见 `agents-ui-spec.md`「页面分工」） |
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
| 2026-10-19 | 游戏内"完全不显示模型"，以为模型没导出/没装上 | 解包发现 patch 的 `bone_hashes=0`、**权重行和全 0.0** → 网格没有蒙皮，Unity 画不出来。外部导入的物体没有 `exmt_bones`，生成 Mod 时**不传 `bone_table`** 就让 `resolve()` 全返回 None、每条权重被丢。三处修：① `_bone_slots` 空表时从场景已导入网格借表；② 生成循环显式传 `target_bone_table()`，且 `groups>0 and influenced==0` 直接报错；③ `dropped` 计数器原来写在顶点循环内、永远 0。判"权重进没进文件"看四个数：行和≈1、索引不越界、`bone_hashes`/`bindpose` 非空、参与骨骼槽 > 0。见 `docs/ledger.md` §2.16 |
| 2026-10-19 | 用户看 Blender 里"没绑定"（转骨骼模型不动） | 插件的绑定**不靠 Armature 修改器**（靠"顶点组名 = 游戏骨骼名"+导出写 `bone_hashes`），但预览必须说真话：`Sally_outfit` 的修改器指向 PMX 留下的空骨架（`bones=0`）。新增 `mesh_io.reuse_rig()`：场景里已有含这些骨骼的骨架（`c_VepleySSR01` 163 根）就复用，否则按骨骼表新建 |
| 2026-10-19 | 以为"同一网格在不同包只是 UV/可读性不同" | **骨骼表也会不同**：`c_VepleySSR01_slg_cloth_lod0` 三份拷贝里 `872f1c4f` 的骨骼表与另两份**差 15 根**（`Hip_M` 被删、`Hip_R/Hip_L` 直接挂 `Root_M`，层级重组）。按哈希重映射后那份丢 30% 权重、1954 顶点塌到原点。**别用短名兜底**（实测 60.0% < 69.6%）（**本行早先写的“别用短名兜底”是错的，已被下面那条订正**：顺序必须是先哈希、后短名）。见 `docs/ledger.md` §2.16 |
| 2026-10-19 | 游戏里"手臂不见了、手环浮空"，以为是权重或绑定错 | **材质槽**问题：导出的子网格编号 = 源模型槽号，而渲染器的材质列表属于**被替换的游戏网格**（GF2 只有 4 个），索引 ≥4 的子网格 Unity 根本不画。Sally 20 槽 → 6803/14568 面被丢，其中手臂/手 2295 面（`Glove` 槽 4、`Skin` 槽 6），而槽 3 `Bracelets` 活着 → 手环浮空。修：导出时按目标槽位数**合并材质槽**（新增 `exmt_core matinfo` 读目标子网格数） |
| 2026-10-19 | 用 `len(m_DataSize) / vertex_count` 当顶点 stride | **错**：`m_DataSize` 是**三个连续流**（s0 pos+normal+tangent 40B、s1 colour+uv 20B、s2 weights+bones 32B；和 = 92B），顶点在 **s0 内**步长才是 **40**。按 92 索引只命中顶点 0（症状：y 最小值变了、最大值没变、x/z 不动 = 只缩放了第一个顶点）。判据：把**解码后的位置**去 `raw.find()`，连续偏移的差值才是真 stride |
| 2026-10-19 | 认为"两套骨骼表"是导出/合并的问题 | 是**游戏自己的包处于两个代际**：一份有 `Hip_M` 中间层，一份删了它、腿直接挂 `Root_M`。126 根里 123 根**短名相同**、12 个短名各对应两个哈希（腿链）。`_remap_bones` 改成**先哈希、后短名兜底**：保留权重 69.6% → 85.8%，无驱动顶点 1954 → **0**。⚠️ 别把"只用短名"的 60.0% 当成"短名无效"——顺序必须是先哈希 |
| 2026-10-19 | 蓝图「分组」节点点「＋槽位」报 `AttributeError: 'EXMTNodeGenerate' object has no attribute 'sync_slots'` | **重构漏改的死调用**：方法早改名 `ensure_slots`，`EXMT_OT_blueprint_sync` 里还叫着旧名。改名为 `ensure_slots()`，并让它同时处理「生成 Mod」和「分组」两种节点。判据：**改过方法名后必须 grep 旧名**，`py_compile` 抓不到这种错（只在运行时炸） |
| 2026-10-19 | 用户："生成 Mod 节点上没有生成 Mod 按钮" | 按钮**在**，但 `row.operator("exmt.blueprint_generate")` 指向的 idname **从来不存在**（真名 `exmt.generate_mod`），所以点了等于没点、也不报错。判据：**UI 里每个 `operator()` 的 idname 都要能对上已注册的操作符**，用脚本批量比对（见下一条），别靠肉眼 |
| 2026-10-19 | 怎么在没有窗口的情况下验证 UI 代码 | **用假 layout 真调 `draw_buttons()`**：写一个 `FakeLayout`，实现 `label/prop/row/column/box/operator/template_ID/template_preview`，`operator()` 返回可赋属性的 `FakeOp`（赋值时用 `bpy.ops.x.y.get_rna_type()` 校验属性名真的存在）。这样 `AttributeError`/`NameError`/写错的操作符 idname **全部在 headless 就被抓住**（实测一轮抓出 3 个 bug，25 个节点 0 失败）。注意 `bpy.types.XXX.bl_rna.properties` **不列操作符自定义属性**，要用 `bpy.ops.mod.fn.get_rna_type().properties` |
| 2026-10-19 | 生成节点显示"1 条连线没指定 Submesh"，可连线明明是对的 | 警告只认**直接连过来的** `物体` 节点；中间隔一个「分组」就误判成无效 → **假警报**，会把人往错方向带。新增 `socket_ok()`（走 `walk_objects` 穿透分组）后误报消失。**警告逻辑必须和真正的解析逻辑用同一条路径**，否则它骗的就是自己 |
| 2026-10-19 | 贴图节点只写贴图名字，用户记不住 `c_Xxx_slg_cloth01_d` 是哪张 | `workspace.json` 的 `textures[].file` 已经是解好的 PNG → 节点里 `layout.template_preview(image)` 画原图，并在标签里标**用途**（漫反射/法线/粗糙金属遮挡/自发光，按 `_d/_da/_n/_nt/_rmo/_bt` 后缀判）；下拉条目的 label 保持**纯 ASCII**（中文用途放 description，理由见下一条枚举乱码）。`preview` 拿不到时降级成文字标签，绝不让节点崩 |
| 2026-10-19 | 「＋槽位」点了像没反应 | 原逻辑"只要有 1 个空闲就不加"，于是连着 5 个物体的人点了没变化。改成 `ensure_slots(count=4)`：**保证 4 个空闲输入口**，按钮名也改成「＋ 输入口」（说清楚是补输入口，不是"槽位"这种黑话）。永不裁剪用户已有的空闲口 |
| 2026-10-19 | 枚举字段（`layout.prop(node, "xxx")`）显示成一堆乱码 | **枚举条目的 label/description 里别放非 ASCII**（我用了 `·`）：Blender 5.2 会把整格画成随机字形，而**同一段文字放在 `layout.label()` 里完全正常**——所以别以为是截图/字体问题，先换成 ASCII 空格复测（实测 `"%s  %sx%s"` 立刻正常）。贴图选择器用 `layout.template_ID(node, "image", open="image.open")` 能直接给个浏览按钮 |
| 2026-10-20 | 用户："AGENTS.md 不应该是流水线，而是约束和规则；流水账改到 docs 下面" | **AGENTS.md 只放短的可执行约束**（铁律 + 写回规则 + 指针，≤120 行）。环境/质量门细节 → `docs/agents-playbook.md`；UI 规范与偏好 → `docs/agents-ui-spec.md`；逐条纠正 → `docs/agents-corrections.md`。**新知识一律别往 AGENTS.md 堆**，只把"结论性规则"折进去 |
| 2026-10-20 | `bun` 不在 PATH，直接 `bun run check` 会失败 | 一律用全路径 `C:\Users\Administrator\.bun\bin\bun.exe run <script>`（见 playbook 工具链与速查） |
| 2026-10-20 | 微信推送"只发 ASCII"还不够：带 `:` `;` `,` `%2F` 的消息仍然 400 | 路径里只用 字母/数字/空格(`%20`)/`.`/`-`；别用标点（见 `agents-ui-spec.md` 用户偏好末条） |
| 2026-10-20 | 用户：「维护和更新skills是第一优先级」 | 每轮写回先问"这条能不能进 `.agents/skills/`"（新领域就新建 skill，已有 skill 就补）；skill 里的内容必须能脱离本项目复用；`docs/` 只放只对本项目有意义的东西 |
| 2026-10-20 | 仓库里有**另一个会话在同时提交**：本轮中途 HEAD 从 `9df403b` 变成 `7adac3d`，`core/` 一直带着别人的未提交改动 | 提交只 `git add <本轮改的文件>`，绝不用 `git add -A` / `git commit -a`；push 前先 `git log --oneline -3` 确认基线；`git status` 里看到的 `M` 不一定是自己的 |
| 2026-10-20 | 用编辑工具往表格里插行时，`old_string` 只截了已有行的**行首**，结果把那行截成了半截（`\| ：本轮中途…`），同一个坑犯两次 | **追加表格行一律锚定整行或追加到文件末尾**；改完必须脚本校验每行都匹配 `^\| (20\|日期\|---)`（本轮 `bad_rows=0` 就是这条） |
| 2026-10-20 | 为了改几个 CSS 变量名，用 `pwsh` 的 `Get-Content -Raw | Set-Content -Encoding utf8` 重写了 `ModelsView.vue`：结果**中文全变乱码 + 开头多了 BOM**，模板里的 `✓`/`×`/中文文案一起坏掉 | `Get-Content` 默认按**系统 ANSI(GBK)** 读 → UTF-8 中文被解成乱码再写回；`Set-Content -Encoding utf8` 还会加 BOM。**铁律 8 就是这么来的**：任何 `.vue/.ts/.py/.md` 都只用编辑工具改；真被写坏就 `git checkout -- <file>`（未提交的改动会丢，所以**先提交或先备份**）再用编辑工具重做；`[System.Text.Encoding]::UTF8.GetString(...)` 这种只读校验可以，别写回 |
| 2026-10-20 | 新写的 CSS 用了 `--stroke-2 / --stroke-3 / --fill-control / --on-accent`，界面上边框和选中圈**完全不显示**，看着像纯文字 | 这些变量**不存在**（`tokens.css` 里真名是 `--stroke / --stroke-strong / --fill-subtle-pressed / --fill-subtle-hover / --accent-contrast`）。CSS 自定义属性写错**不会报错、只会静默失效** → 写样式前先 `Select-String tokens.css` 核对变量名，改完截图看边框真的在 |
| 2026-10-20 | 前端用 `c_${skinKey}_` 拼动作前缀，界面显示"这个皮肤没有动作"（0 个） | `skinKeyOf()` 返回的**已经带 `c_`**（`c_VepleySSR0102`），所以前缀成了 `c_c_…`。和贴图同一条约定：`${skinKey}_`。判据：**凡是按"皮肤前缀"过滤的地方，都拿同一个 `skinKey` 直接加下划线**，别再手动补 `c_` |
| 2026-10-20 | Blender 里首帧姿势对、中间帧"头飞了" | 不是动作算错，是**零权重挂件**（脸 `face_lod0` 3 组、武器 21 组，所有权重都是 0）在游戏里是刚性挂在某根骨上的，播动作时留在原地。判据：**先看"受驱动顶点数"，再看姿势**——`used=0` 的网格永远不动。这类网格要单独约定挂点，别当蒙皮网格 |
| 2026-10-20 | 移动端式"层级合成"踩坑：把 clip 的父子链和 Blender 的 rest offset 混用，200+ 根骨整体位移 1 m | 两条链必须**配套**：`world` 沿 clip 的父子链合成，算 basis 时父的世界矩阵要取 **Blender 父**在 `world` 里的值（没有就取 rest），offset 用 `parent.matrix_local⁻¹ @ bone.matrix_local`。自证：首帧挂动作 vs 不挂动作应为 **0 顶点移动** |
| 2026-10-20 | 动作后缀登记抢答，5 根骨变成假根 | 同一哈希会在不同裁剪深度被登记多次，深处那次常算出"父=空" → **有父优先、同条件取最浅裁剪**。自证：父子世界距离在整个动作里中位 0.0000 m、p90 0.0039 m |
