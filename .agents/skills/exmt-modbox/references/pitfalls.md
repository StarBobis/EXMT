# 跨领域坑清单（症状 → 根因 → 判据/修法）

> 每条只写「症状 → 根因 → 判据/修法」，按主题分组。可脱离本项目复用。

## 1. 编码与文件写回

| 症状 | 根因 | 判据 / 修法 |
| --- | --- | --- |
| `.vue`/`.ts`/`.md` 中文全变乱码，文件头多 BOM | `Get-Content` 默认按系统 ANSI(GBK) 读，`Set-Content -Encoding utf8` 又加 BOM | 源码一律用编辑工具改，**任何情况下不用 `Get-Content \| Set-Content` 处理源码**；只读校验可用 `[Text.Encoding]::UTF8.GetString(...)`，不写回。被写坏先提交或备份，再 `git checkout -- <file>` 用编辑工具重做 |
| 往表格插行把上一行截成半截 | `old_string` 只截了行首 | 追加行锚定**整行**或追加到文件末尾；改完脚本校验每行形状（如 `^\| `） |
| 提交里混进别人的改动 | 有并行会话在改同一仓库 | 只 `git add <本次改的文件>`，绝不用 `git add -A` / `git commit -a`；push 前 `git log --oneline -3` 确认基线，`git status` 的 `M` 不一定是自己的 |
| 改过方法名，运行时才 `AttributeError` | 重构漏改死调用，`py_compile` 抓不到 | 改方法名后立即 `grep` 旧名；UI 里每个 `operator()` 的 idname 都要能对上已注册操作符（脚本批量比对） |

## 2. Windows / WebView2 / 进程上下文

| 症状 | 根因 | 判据 / 修法 |
| --- | --- | --- |
| `0x8000FFFF 灾难性故障`、`PluginInitialization("http", 拒绝访问 os error 5)` | 工作区内的 exe 被以 Low IL（S-1-16-4096）启动；低 IL 受 no-write-up 限制，写不了 `%LOCALAPPDATA%` → WebView2 建不了 profile | 30 秒证伪代码：换 `tauri::Builder::default()`（零插件/零 setup）照样崩；10 秒锁定环境：同一份 `whoami.exe` 放工作区 = Low、放 `C:\DevCache` = High。修法 `icacls <dir> /setintegritylevel "(OI)(CI)LW"`；加 LW 意味着任何低 IL 进程都能改它，属开发取舍，正常双击运行不需要 |
| 崩溃发生在 `msedgewebview2.exe` 里 | 症状出现在别人的进程里 | 先量**自己进程**的上下文（IL / AppContainer / job / 父进程），别去读业务代码；判定某变量必须 A/B 交叉来回跑并固定「谁拉起它」，否则沙箱里成败成片波动只是噪声 |
| `Port 1420 is already in use` | 上次留下的后台 dev server 还活着（后台 job 会活过对话） | 改成探测-复用：`beforeDevCommand = node scripts/dev.mjs`，`GET /@vite/client` 200 就打 reusing 并 exit 0；端口空才 spawn vite；被别的程序占则 exit 1。收尾必须关掉自己起的 server |
| `bun tauri dev` 编译/链接失败：`rust-lld: undefined symbol: anon.…llvm.…`，引用 `libexmt_core-*.rlib` | `src-tauri/target` 里的 exmt-core 增量/rlib 坏了（源码已变但 cargo 复用了旧 rlib；`cargo check` 能过、一链接就炸） | 不是代码错误。`cd src-tauri; cargo clean -p exmt-core -p exmt-modbox` 后重跑（2026-10-22 实测清了 7.3 GB、重建 3.5 min，之后 `bun tauri dev` 1.9 s 起）：先怀疑增量缓存，不要改代码 |
| 点按钮没反应、截图还是上一页 | **有两个同名窗口叠着**，脚本只认第一个匹配 | `Get-Process <name> \| Measure-Object`，多个就全 `Stop-Process` 再重启；否则所有 UI 证据作废 |
| 长流程中途回到第一步 / 整窗白屏 | HMR 重置 Vue 本地状态、热更新把组件树更新坏 | 分析类长流程中途不要改前端源码；白屏直接重启应用，别当代码 bug 查 |

## 3. PowerShell / 截图 / CDP 验证

| 症状 | 根因 | 判据 / 修法 |
| --- | --- | --- |
| 截图、点击整体偏 1.5 倍，右侧截不到 | pwsh 是 DPI-unaware 进程，`GetWindowRect`/`SetCursorPos` 返回虚拟化坐标（150% 缩放） | 先 `SetProcessDpiAwarenessContext`（或 `SetProcessDPIAware()`）再截图/点击；坐标 = 截图物理像素 − client 原点 |
| 窗口被遮挡时截图是旧帧 | `PrintWindow` 默认返回旧帧 | `HWND_TOPMOST` → `NOTOPMOST` + `CopyFromScreen`，或 `PrintWindow(PW_RENDERFULLCONTENT)` |
| 合成鼠标点击在窗口被盖住时失效 | `SetForegroundWindow` 被拒 | 先置顶/前台再点，或改用 CDP / UIA 验证状态 |
| 连点几下，后面几次总点空 | 消息条出现会把下面列表整体下移 | 每次点击前**重新截图**，不复用上一次坐标；HMR 后先 `Ctrl+R` 重载再判断 |
| 只有截图、没有可断言的状态 | 视觉判断不可靠 | 用 CDP 读 DOM / 计算样式做断言（导航项数量、展开宽度、选中态），截图只作旁证 |

## 4. Unity 资产编解码

| 症状 | 根因 | 判据 / 修法 |
| --- | --- | --- |
| 只有第 1 个顶点被缩放（y 最小值变、最大值没变、x/z 不动） | 把 `len(m_DataSize)/vertex_count = 92` 当 stride；它其实是三条流之和，s0 内 stride 才是 40 | 把**解码后的位置**去 `raw.find()`，连续偏移差值才是真 stride |
| 贴图"写成功"但像素没变、包只涨 140 字节 | typetree 键是 `image data`（带空格）；写 `m_ImageData`/`image_data` 被当新键静默丢弃 | 写 `tree["image data"]` 并清 `m_StreamData`；写回后从包里解回来比数值（dB / 最大通道差），别只看包大小 |
| 替换贴图"看着对"、一比对全错（10.1 dB） | Unity Texture2D 行序自下而上，texconv 对 PNG 输出自上而下 | 每个 mip 的块行整体翻一次（翻后 59.2 dB） |
| 贴图整体变暗（31→4、219→181） | texconv 把 PNG 当 sRGB，输出 UNORM 时转线性 | 固定加 `-srgbi -srgbo` 做恒等变换（BC1 端到端 42.17 dB / BC7 54.94 dB） |
| 索引翻倍、子网格错位 | `m_IndexBuffer` 是字节数组，`SubMesh.firstByte` 是**字节**偏移 | 按字节切片再 `frombuffer`，不要当索引下标 |
| 未改动网格的 `localAABB` 与原包不等 | 按索引区间 `pos[min:max+1]` 算盒体，把未引用顶点算进去 | 按**被引用顶点**算盒体 |
| 重打包后 `Failed to load CAB-` | 只有前 32768 字节是加密的 | 只加密头部这一段，别整文件 XOR |
| 批量读贴图极慢（240 张 15 min） | `read_typetree()` 会连 image data 一起解析 | 先 `peek_name()` 筛（41 s） |
| 丢截断切片喂 UnityPy → 静默 0 个对象 / EOFError | `check_file_type` 拒收 + 急切解析 AssetBundle 对象 | 只解 SF 元数据前缀读 type list（找 class id 74），不解全包 |
| 贴图重名只打了一个包 | 一个贴图名可能有多份拷贝（3824 个名字里 72 个） | 索引同时收 Mesh + Texture2D，写 `texture_bundles`，每个拷贝各出一行 |

## 5. 骨骼 / 权重 / 动画

| 症状 | 根因 | 判据 / 修法 |
| --- | --- | --- |
| 转骨骼模型不动、Armature 修改器 0 顶点 | 顶点组名 ≠ 骨骼名（`b000` vs `b0`） | 顶点组名必须与骨名逐字相同；判据 `influenced == 0` |
| 游戏里模型完全不可见，以为没导出 | patch 的 `bone_hashes=0`、权重行和 0.0 → 网格没蒙皮，Unity 画不出来 | 四数判据：行和≈1、索引不越界、`bone_hashes`/`bindpose` 非空、参与骨骼槽 > 0 |
| 顶点组名完全一致但权重全丢 | 导出没传 `bone_table`，外部物体没有 `exmt_bones` → `resolve()` 全 None | 生成循环显式传 `target_bone_table()`；`groups>0 and influenced==0` 直接报错；`dropped` 计数器必须写在顶点循环**外** |
| 某副本权重丢 30%、1954 顶点塌到原点 | 该副本骨骼表与 patch 差 15 根，是**层级重组**（`Hip_M` 被删）不是骨骼不同 | 先哈希、后短名兜底（69.6% → 85.8%，无驱动 1954 → 0）；只用短名 60.0% 不能当「短名无效」的证据 |
| 200+ 根骨整体位移约 1 m | 后缀裁剪深度抢登记，5 根骨变假根 | 有父优先、同条件取最浅裁剪；判据父子世界距离中位 0.0000 m / p90 0.0039 m |
| 摆起来整团扭曲，而首帧验收通过 | ① Blender rest ≠ 游戏 bind（32 根里 18 根差 10–179°，纯 roll）② 骨架骨数 32 < 动作驱动 213 | 按游戏 deform 算 basis；带动作时骨架按全皮肤网格并集建（32→156）；验收换成边缘拉伸绝对毫米（中位 0.14–0.19 mm / p95 2.2–2.7 mm） |
| 「首帧 0 顶点移动」永远为真 | 那条 clip 首帧恰好等于绑定姿势，误差全抵消 | 这条检查不检验任何东西；换首帧非绑定姿势的 clip 做反证，且要能因「你要防的那个错」变红 |
| 播动作时头飞了 / 挂件留在原地 | 零权重挂件（脸 3 组、武器 21 组全 0）在游戏里是刚性挂在某根骨上的 | 先看「受驱动顶点数」再看姿势；`used=0` 的网格要单独约定挂点，别当蒙皮网格 |
| 权重行和差 1.2e-7、往返不是逐位一致 | 等权重影响槽排序方向反了（游戏数据是索引升序）；权重和已 ≈1 还在归一化 | 并列槽按索引**升序**；和 ≈1 时不再归一化 |
| 武器/挂件网格解不出几何（`buffer is smaller than requested size`） | 该类型在打包里的烘焙方式不同 | 前端按 `error` 过滤，不显示点不开的行 |

## 6. Blender API

| 症状 | 根因 | 判据 / 修法 |
| --- | --- | --- |
| 5.2 里自定义 NodeTree 显示不出来（`ui_type` 报没有该属性） | 5.2 移除 `ui_type`，`edit_tree`/`id` 变成只读 | `space.tree_type = <bl_idname>` 然后 `space.node_tree = <tree>`；4.x 才用 `ui_type`/`edit_tree`；读 `bl_rna.properties[...].is_readonly` 判断，别猜 |
| 5.2 没有 `action.fcurves` | API 改版 | `slots.new` + `layers.new` + `strips.new(type="KEYFRAME")` + `channelbag(slot).fcurves.new(...)`，并设 `action_slot` |
| `node.parent is frame` 对「自己所属的 frame」返回 False | `parent` 每次访问返回新的 Python 包装对象 | 一律用 `==` |
| `foreach_get` 写进 `out[:, :3]` 读回全 0 | 非连续视图等于写临时拷贝 | 用连续临时数组再赋值 |
| 枚举字段整格乱码 | 枚举 label/description 里有非 ASCII（如 `·`）；同文字放 `layout.label()` 却正常 | label/description 全 ASCII，中文放 description |
| 节点戳出 Frame 外 | Python 建的 `NodeFrame` 永远 160×100，`shrink=True` 只在手动拖动时重贴 | 按子节点包围盒自算 `width/height` |
| `--python` 里 `view_all()` 空跑、截图拍到 splash | 脚本执行时窗口还没布局完 | `bpy.app.timers.register(fn, first_interval=2.0)`；截图前 `SetForegroundWindow` + 发 `{ESC}` 关 splash |
| headless 渲染有大块灰墙挡住模型 | `--factory-startup` 自带 2 m Cube、相机、灯 | 渲染前 `for o in list(bpy.data.objects): bpy.data.objects.remove(o, do_unlink=True)` |
| 按钮画得出来、点了什么都不发生（也不报错） | `operator()` 的 idname 从未注册（写了 `exmt.blueprint_generate`，真名 `exmt.generate_mod`） | 每个 idname 都要能对上已注册操作符；用 `FakeLayout` 在 headless 跑 `draw_buttons()` 批量比对 |
| 查不到操作符自定义属性 | `bpy.types.XXX.bl_rna.properties` 不列操作符自定义属性；`bpy.ops.x.y` 是函数包装器不能赋值 | 用 `bpy.ops.<mod>.<fn>.get_rna_type().properties` |
| 警告误报「1 条连线没指定 Submesh」，而连线是对的 | 检查只认直接连过来的物体节点，中间隔了「分组」 | 警告与真正解析必须走**同一个函数**（穿透分组），否则警告骗的是自己 |
| 重新导入后用户连线/选图没了、物体变 `.001` | `new_blueprint()` 同名直接 remove；旧网格数据块没删（名字被占） | 同名蓝图复用；Frame 内物体节点按 `submesh` 重指向；导入**前**删旧网格**连数据块**、回收无孩子的旧骨架 |
| 自定义属性存 uint32 抛 `OverflowError` | `bpy` 属性类型限制 | 转 JSON 字符串 |
| 没有窗口也想验证 UI 代码 | `UILayout` 只在绘制时存在 | 写 `FakeLayout`（`label/prop/row/column/box/operator/template_ID/template_preview`）+ `FakeOp`，真调 `draw_buttons()` |

## 7. UI / 渲染 / i18n

| 症状 | 根因 | 判据 / 修法 |
| --- | --- | --- |
| 新写的 CSS 变量不生效，边框/选中圈完全不显示 | 变量名不存在；CSS 自定义属性写错**不报错、只静默失效** | 写样式前 `Select-String tokens.css` 核对真名；改完看边框真的在 |
| KeepAlive 切页面内容不更新（一直停在首页） | `v-if` 写在 `<KeepAlive>` 内部 | `v-if` 放 KeepAlive **外层**，动态组件保留 `:key` |
| 原生 `<select>` 弹出层白底白字、只显示一项 | 弹出层由 Windows/WebView2 系统绘制，不吃暗色主题 | 自绘 Dropdown：Teleport 到 body、fixed 定位、token 配色、键盘 ↑↓/Enter/Esc |
| 动作栏显示「这个皮肤没有动作」（0 个） | `skinKeyOf()` 已带 `c_`，又手动补了一次 → `c_c_…` | 凡是按皮肤前缀过滤，都直接用 `skinKey` 加下划线 |
| 3D 滚轮缩放没反应 | `enableDamping` 的相机必须逐帧 `controls.update()`；WebView2 `deltaY` 单位/步长不统一 | rAF 里逐帧 update；关 `enableZoom`，自己按 `exp(-delta/400)` 指数步进 dolly |
| 线框模式开了画面不变 | `material.wireframe` 在该 three 版本无效 | 换 `WireframeGeometry`/`LineSegments`；没生效就删干净，不留半套 |
| 贴图被拉成长方形 / 卡片被压扁且说明被裁 | 固定高度网格只写 `align-content:start` 会把行压扁，`overflow:hidden` 裁掉说明 | 方形框 `padding-top:100%`（不依赖 `aspect-ratio`）；网格加 `align-items:start; grid-auto-rows:max-content`，名称放框外 |
| 相机拉太远、人物缩成一小团 | `fit()` 用了所有子网格并集，异常挂饰在脚下 0.9 m | 按零件中心中位数 ±1.2 m 取景，异常件不参与 |
| 大网格画成碎片 | WebGL1 无 `OES_element_index_uint` 时 32 位索引坏 | 按最大索引选 `Uint16Array`/`Uint32Array` |
| 有贴图但预览是灰的（旧版：只有勾选才有颜色） | 材质基色用 idle 灰 `0x6c737c` 把贴图乘暗 | 有贴图的子网格基色固定白色；颜色只由「有没有贴图」决定，不依赖勾选/查看状态（模型页已无勾选，点击只切显隐） |
| 3D 衣服像碎布、贴图块随机错位 | 配对拿了**别的变体**的 exact-head 贴图（Vepley SSR01 cloth 曾被配成 Dorm cloth） | 同变体有同名编号层（cloth01/02）时让它压过跨变体 exact head；编号层全保留、编号最大的当主图（实测 cloth02 是默认外观）；没有编号层时规则不变 |
| 用户说「网格缺失」 | 四种可能：数据没有 / 索引错 / 被裁掉 / 没取景 | 分诊顺序：面板统计（已建/总数、提交三角形）→ 同一份包导出 OBJ 给第二个渲染器 → 材质参数。真因常是 `alphaTest` + BC3 alpha 全 0 |
| 皮肤包 `diffuse.json` 只有 0–3 条、一直灰 | 包内配对只做精确匹配，而游戏部件名与贴图名不同名（`slg_cloth` vs `slg_cloth1_d`） | 缓存产物里的派生数据必须与运行时**同一套兜底规则**（精确候选 → 共享茎最长）；加自检「期望几条 / 实际几条」 |
| 变体皮肤的 `body`/`hair` 找不到贴图 | 只按自己皮肤 id 找，贴图其实在同角色家族（少两位变体号）或数字不同名 | 三级匹配：精确 → 同家族同语义头（去掉 `Dorm`/`Fight`/`lodN`）+ 共享前缀最长 → 该皮肤已采用贴图中出现最多的一张兜底 |
| 子网格数量翻倍 / 默认衣服被宿舍服顶掉 | `_lod(\d+)` 漏掉 `lodm0`；部件键把 `_Dorm`/`_Fight` 后缀一起丢了 | 正则 `_lod(m?)(\d+)`，同 lod 优先非移动版；部件键 = lod 前茎 + lod 后**全部后缀** |
| 切皮肤还要现场解析几十秒 | 各页面首次用时各自扫全库 | 一次 `analyze` 写全量缓存，页面只读；验收问「用户点下去到看见，中间有几次子进程」 |
| 预解完了却仍退回现场解（包被误判过期） | 各产物各自取 `time.strftime()`，manifest 更晚取 → `cache_built_at` ≠ `built_at` | 整次分析只取一次时间戳，所有产物共用；回归验证「packs 匹配 manifest = N/N」 |
| 同一会话切走再切回又解析一遍 | 缓存签名缺失或页面重建 | 签名 = 相关 bundle 的 `size+mtime`；内存 LRU 保留最近 4 个；页面用 `<KeepAlive>` 保状态 |
| 按钮永远灰着 | `refreshGenerated()` 写在 `initWorkspaces()` 开头，那时 `activeWorkspace` 还没恢复 | 依赖 A 的 B 必须放在 A 之后；`onActivated` 补一次 |

## 8. 数据契约 / 状态

| 症状 | 根因 | 判据 / 修法 |
| --- | --- | --- |
| 装第二个 Mod 后第一个白改了 | 一个包只有一份文件，后装整包覆盖 | Mod = 补丁集合；运行时 = 原始包 + 所有启用补丁重算（`applyMods`）；状态写 `applied.json` |
| 只换贴图的包整包被漏 | `targets` 只取了 `byBundle` | 改成 `byBundle ∪ texByBundle` |
| 点安装用的却是旧补丁 | 页面打开时缓存了 `mod.json`，用户可能刚在外部重新生成 | 安装前重新读一遍 `mod.json` |
| 素体身体被别的包覆盖后再也回不来 | 该类 Mod 只存指针、不存数据 | 也录成补丁，或安装前检查该包是否已被占用并拒绝 |
| 连点两次「生成 Mod」把上次改动烤进本次 | 打包起点指向游戏目录而非原始副本 | 打包必须从原始字节开始：`<cache>/game/bundles` → 旧备份 → 游戏目录 |
| 同一网格一个包改了、另一个没改 | 工作空间只记了首个包 | `mesh_bundles`/`texture_bundles` 记全部拷贝，每个拷贝各打一行 |
| 换包后 UV 格式/可读标志被改掉 | 照抄了另一个副本的 tree | 只有「通道布局 + 可读标志 + 骨骼哈希表」全同才用文件自带 tree；否则以目标包 tree 为底，只写几何 + 按哈希重映射骨骼 |
| 状态行数字虚高（"3 个 Mod 整合成 1 个包"） | 统计把被跳过的旧记录也算进去了 | 只数真正参与整合的 Mod，旧记录单独计 `legacy` |
| 点「开始制作」报 `open <cache>/game/bundles/<hash>.bundle: os error 2` | 备份是在**更早一次分析**后做的（`game/done.json` 标记完成），后来的分析才发现新包（实测是 Vepley 的 Audio 音效包），备份目录里没有 | 旧 `bundlePathOf` 只看 `backup.ready`、不看该文件是否存在 → 返回不存在的备份路径。改成 `bundleFileOf()` 按 **存在性** 三级回落：`<cache>/game/bundles` → `%APPDATA%\…\backups` → 游戏本体；落到游戏本体时顺手 `healBackup()` 把包补进备份。两边都没有才报「确认游戏路径并重新分析 / 再点备份原始模型」 |
| 备份显示"完成"却少包 | `done.json` 只代表备份那一刻的分析代际；重新分析/游戏更新后发现的新包不会自动补进备份 | 备份与 GameCache 是两代数据，不要把 `ready` 当全覆盖；新包靠运行时回落 + 自动补齐，判断缺失要看**具体文件存在性** |
