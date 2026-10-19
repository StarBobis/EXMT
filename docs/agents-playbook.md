# 运行手册 · EXMT Modbox

> 这份是 `AGENTS.md` 的**细节层**：仓库地图、本机工具链、交付动作、质量门、写回半径。
> 可以长，但**不记流水账**：结论 / 证据 / 未决问题 → `docs/ledger.md`；
> 纠正 → `docs/agents-corrections.md`；换个目标也用得上的判据 → `.agents/skills/`。

---

## 1. 仓库地图

| 位置 | 是什么 | 怎么跑 |
| --- | --- | --- |
| `src/` | 前端（Vue 3 + TS + Vite）；视图 `src/views/`，样式 `src/styles/` | `bun run dev` |
| `src-tauri/` | Rust 外壳（Tauri 2），命令 `run_core` / `show_main_window` | `bun run tauri dev` |
| `core/exmt_core/` | Python 核心：`bundle.py` `sf.py` `mesh.py` `interchange.py` `textures.py` `preview.py` `bc1.py` `retarget.py` `subdivide.py` `anim.py` `cli.py` | `python -m exmt_core <cmd>`（cwd 必须在 `core`） |
| `C:\Dev\EXMTBlender\` | Blender 插件：导入/导出 `.exmtmesh`、传权重、蓝图、生成 Mod | Blender → 偏好设置 → 安装 `__init__.py` |
| `C:\Dev\EXMTDoc\` | VitePress 2 文档站（**独立 git 仓库**，无 remote） | `bun run build` / `bun run preview` |
| `docs/` | **台账**：结论 + 证据 + 未决问题（`ledger.md` 是入口） | — |
| `.agents/skills/` | **能力**：换目标也能用的判据/手法 | — |
| `C:\DevCache\<任务名>\` | 一次性脚本、临时产物 | — |

以仓库结构为准，别凭记忆：`AGENTS.md` 只给规则，细节在这里查。
下表与后面的 `bun run …` 都要按工具链那条走全路径：`C:\Users\Administrator\.bun\bin\bun.exe run …`。

---

## 2. 本机工具链（已验证）

- 应用：`bun run tauri dev`，窗口标题 `EXMT Modbox`
  ⚠️ 本机 `bun` **不在 PATH 里**：一律用全路径 `C:\Users\Administrator\.bun\bin\bun.exe run <script>`
- 核心 Python：`C:\Users\Administrator\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe`
  （UnityPy 1.25 / lz4 / numpy / PIL / texture2ddecoder）
- Blender：`C:\Program Files\Blender Foundation\Blender 5.2\blender.exe`
  （headless：`--background --factory-startup --python x.py`）
- 游戏：`C:\GF2Exilium\GF2 Game`，资源在 `GF2_Exilium_Data\LocalCache\Data\AssetBundles_Windows\`
- 窗口驱动 / 截图：`C:\DevCache\gf2\app2.ps1 click <cx> <cy>` / `app2.ps1 shot [-out p.png]`
  （**DPI-aware**：先把窗口摆到 (8,8)；点击坐标 = 截图物理像素 − client 原点。老的 `app.ps1`
  是 DPI-unaware 的，150% 缩放下会错位、窗口比屏幕大时右侧截不到）
- 质量门：`C:\Users\Administrator\.bun\bin\bun.exe run check` → `scripts/check.ps1`

---

## 3. 交付前固定动作

```powershell
& 'C:\Users\Administrator\.bun\bin\bun.exe' run check
```

- [ ] `bun run check` 通过
- [ ] 有可见变更 → 截图（`app2.ps1 shot`），并说明截图对应哪次改动
- [ ] `docs/ledger.md` 已更新（结论 / 证据 / 未决问题）
- [ ] 本轮新判据已进 `.agents/skills/`，或明确说明无需
- [ ] 改动过的仓库都 commit + push
- [ ] 回答末尾一行「沉淀」声明
- [ ] 任务收尾发微信推送（见 `agents-ui-spec.md` 的用户偏好）

---

## 4. 质量门 `bun run check`（`scripts/check.ps1`）

依次执行，任一步失败即整体失败：

1. `npx vue-tsc --noEmit`（前端类型）
2. `cargo check`（`src-tauri/`，找不到 cargo 才跳过）——外壳是 Rust，编译坏了不能等 `tauri dev` 才发现
3. `<核心 python> -m compileall -q core/exmt_core`（核心语法）
4. `python -m exmt_core meshes <样例 bundle>`（核心冒烟，断言输出含 `c_Vepley`；
   样例 `C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig` 不在则跳过）
5. `vitepress build docs`（若 `C:\Dev\EXMTDoc` 存在）

任何一步失败都不许说"完成"。

---

## 5. 别造轮子：已经有的轮子

| 需求 | 用什么 | 别做什么 |
| --- | --- | --- |
| 读 Unity 资产对象名（贴图等） | UnityPy 的 `o.peek_name()` | `read_typetree()` 全解析（实测 15 分钟 → 41 秒） |
| 解码 BC1/BC7 贴图 | `texture2ddecoder` | 手写 BC 解码 |
| 贴图编码成目标格式（含 mips） | 内置 `EXMTBlender/bin/texconv.exe`（DirectXTex 2026.5.8.1） | 手写 BC7 编码器 |
| 3D 预览交互 | three.js；`enableDamping` 逐帧 `controls.update()`，滚轮自己按 `exp(-delta/400)` 步进并关掉 `enableZoom` | 自研相机数学 |
| Blender 插件 UI 无窗口验证 | 假 layout 真调 `draw_buttons()` | 靠肉眼 / 只跑 `py_compile` |
| 加载/保存 bundle、网格、贴图 | `core/exmt_core/*` 现有命令与函数 | 在脚本里重写一遍解析 |

---

## 6. 写回半径：新知识放哪

| 复用半径 | 写哪 | 例 |
| --- | --- | --- |
| **换目标也用得上（第一优先级）** | `.agents/skills/<skill>/references/*.md`（新领域新建 skill） | `peek_name()` 批量提贴图；UnityFS 只加密前 32KB；指令文件分层法 |
| 只对本项目有意义 | `docs/<目标>-dossier.md` 或 `docs/ledger.md` | 维普蕾的 bundle 哈希、哪个槽几根骨 |
| 用户纠正 / 协作方式 | `docs/agents-corrections.md`，再把结论折成规则进 `AGENTS.md` / `docs/agents-ui-spec.md` | 不要多余按钮、不要日志面板、布局要 WinUI |
| 目标状态 / 阻塞 / 未决 | `docs/ledger.md` | G13 动作导出进行中 |

---

## 7. 常用命令速查

```powershell
$bun = 'C:\Users\Administrator\.bun\bin\bun.exe'
& $bun run tauri dev                   # 跑应用
& $bun run check                       # 质量门
cd core; & $py -m exmt_core <cmd>      # 核心 CLI（cwd 必须在 core）
C:\DevCache\gf2\app2.ps1 shot -out p.png
C:\DevCache\gf2\app2.ps1 click <cx> <cy>
& 'C:\Program Files\Blender Foundation\Blender 5.2\blender.exe' --background --factory-startup --python x.py
```
