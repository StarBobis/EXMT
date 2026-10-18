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
- 窗口驱动/截图：`C:\DevCache\gf2\app.ps1 click <cx> <cy>` / `app.ps1 shot`
  （**client 坐标 = CSS 坐标 × DPI 比例**；截图用 `CopyFromScreen` 并把窗口 HWND_TOPMOST 再 NOTOPMOST，否则会抓到别的窗口）

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

1. 改完自行 **commit + push**；`C:\Dev\EXMTDoc` 是独立仓库，动过也要提交。
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
| 窗口 | 初始尺寸**按显示器工作区算**，不写死：`16:10` 宽屏，宽度 = 工作区 72%，高度上限 88%（超宽屏先让高），下限 920×600，居中于工作区（不是整屏，避开任务栏）。见 `src-tauri/src/lib.rs` 的 `default_window_size` |
| 焦点 | **鼠标点选不出现蓝色 focus 框**：`.nav-item:focus, :focus-visible { outline: none }`，改用 `--fill-subtle-hover` 填充；键盘用户靠填充感知 |
| 页面 | **full-bleed**：`.page { max-width: none; margin: 0; padding: 18px 24px }`。别用 `max-width + margin:auto`，宽窗口留死白、超宽把内容推出屏幕 |
| 栅格 | 用 `minmax(0, Nfr)`；裸 `1fr` / `70% 30%` 会被子元素 min-content 顶宽，产生横向溢出 |
| 卡片 | 同一列表**尺寸必须一致**；缺图条目也占一张同尺寸卡（斜纹底 + 占位图 + 角标），不要降级成小 chip |
| 图片 | 立绘 `object-fit: contain`（保留整身）；头像/胸像用 `contain` + 顶部对齐；不要 `cover` 到把脸裁成特写 |
| 文案 | 中文、直白。**不要黑话**（"素体"被否 → 改成"内置素体"仍被否 → 最终删功能）；按钮名要望文生义 |
| 多语言 | 界面文案一律走 `t("key")`（`src/lib/i18n.ts`），**中文是源字典**，`Record<keyof typeof zh, string>` 强制英文补齐同 key；**模板里不许写死中文**。语言选择存 localStorage `exmt.locale`，默认简体中文，设置页「常规 → 语言 / Language」切换 |
| 密度 | 用户偏好**简洁**：操作日志、素体按钮都是被要求删掉的；状态反馈一行足够 |

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
| 2026-10-18 | 贴图不是某个网格的附属 | 网格与贴图是立绘的两份平级资产：贴图整组显示、不按网格过滤，导出也是整套 |
| 2026-10-18 | 贴图被拉成长方形 | 游戏贴图都是正方形：格子用 `padding-top:100%` 固定 1:1（不依赖 `aspect-ratio`），横向滚动的胶片条，别换成竖排大网格把页面撑出屏幕 |
| 2026-10-18 | 「模型 / 选角色 · 导出改 · 装回来」是无效 UI | 模型页不要页头标题/副标题；搜索框与角色数量放进左侧角色区顶部 |
| 2026-10-18 | 为什么没有 docs/skill 更新 | 本文件 §1 RSI 循环 |

---

## 5. 用户偏好速查

- **要结果**：先给可验证产物（截图/日志/文件），再解释。
- **讨厌嘴硬**：不确定就说"没验证"，别用"应该没问题"。
- **讨厌重复劳动**：同样的活第二次要更快（脚本化、库化）。
- **喜欢干净的信息密度**：一屏看全，别堆按钮和面板。
- **术语要接地气**：黑话要么解释、要么换词。
- **中文回答**，技术名词可保留英文。

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
