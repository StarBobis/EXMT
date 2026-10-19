# AGENTS.md

> 本文件只放**约束和规则**：短、每条都能执行、不写流水账。
> 细节在 `docs/agents-*.md`，目标状态在 `docs/ledger.md`，换个目标也用得上的判据在 `.agents/skills/`。
> **新学到的东西写回上面这些地方，不要往本文件里堆,维护和更新skills是第一优先级。**

## 这是什么

EXMT Modbox：给《少女前线2：追放》(GF2 Exilium) 换模型 / 换贴图、再打包回游戏的工具链。
Tauri 2 + Vue 3 外壳（`src/`、`src-tauri/`）+ Python 核心（`core/exmt_core/`）
+ Blender 插件（`C:\Dev\EXMTBlender\`）+ 文档站（`C:\Dev\EXMTDoc\`，独立 git 仓库）。

| 我要… | 去 |
| --- | --- |
| 跑起来 / 工具链（bun 全路径、python、blender、游戏路径、截图脚本）/ 交付清单 / 质量门 | `docs/agents-playbook.md` |
| UI 与交互必须长什么样 / 用户偏好 | `docs/agents-ui-spec.md` |
| 犯过的错（新纠正追加到这里，并落成规则） | `docs/agents-corrections.md` |
| 目标状态 / 已确认事实 / 未决问题 | `docs/ledger.md` |
| 换个目标也用得上的判据与手法 | `.agents/skills/<skill>/references/` |

## 铁律

1. **没证据不说"好了"**：布局给截图、游戏内给截图、写文件给 diff/回读；不确定就说"没验证"。
2. 目标没达成**不标 complete**；同一阻塞连续 3 轮才谈 blocked（状态写 `docs/ledger.md`）。
3. **交付前质量门必须全绿**：`C:\Users\Administrator\.bun\bin\bun.exe run check`（步骤见 playbook §4）。
   任何一步失败都不许说"完成"。
4. 改完自己 **commit + push**；`C:\Dev\EXMTDoc` 是独立仓库，动过也要提交（它没配 remote，只能本地 commit）。
   **只 `git add` 自己改的文件**——可能有别的会话在同一个仓库里干活，`git add -A` 会扫走别人的 WIP。
5. **能用现成的库就别造轮子**（UnityPy `peek_name()`、`texture2ddecoder`、内置 `texconv.exe`…，清单见 playbook §5）。
6. **`.exmtmesh` 是唯一交换格式**，必须无损往返；glTF 承载不了骨骼哈希/每骨 AABB/碰撞体/通道布局，已删除，不要再引入。
7. 一次性脚本与临时产物放 `C:\DevCache\<任务名>\`，不进仓库根目录。
8. **不用 pwsh 改文本文件**（BOM/GBK 往返会把 `.vue` 写坏）：用编辑工具写。
9. **改 UI 前先想"用户会不会嫌多"——默认更少**；界面文案走 `t("key")`，下拉用 `src/components/Dropdown.vue`，模板里不写死中文。
10. **Blender 插件 UI 改完必须用假 layout 真跑一遍 `draw_buttons()`**（headless）：
    死操作符 idname、写错的方法名、不存在的属性，`py_compile` 全抓不到，只在用户点下去时炸。
11. **中文回答，先给可验证产物（截图/diff/回读）再解释**；边干边说，不许闷头干活。
12. 任务收尾**发微信推送**（GET，路径只能 ASCII）：
    `https://wxpusher.zjiecode.com/api/send/message/SPT_AfgXTrmEG2qVazzIEtkeWWwbKm39/EXMT%20done`

## 每轮自我迭代（唯一的写回规则）

**第一优先级是维护和更新 skills**：能复用的判据/手法先写进 `.agents/skills/`（新领域就新建 skill），
别让它烂在对话里；写进去的东西要能**脱离本项目复用**。

回答用户前问三句（各 10 秒）：

1. **得到"换目标也能用"的判据或手法了吗？（第一优先级）** → 写 `.agents/skills/<skill>/references/`；
   已有 skill 就补进去，别建重复的。
2. **这轮被用户纠正了吗？** → 追加 `docs/agents-corrections.md`，并把结论折成一条可执行规则
   （进本文件铁律 / `docs/agents-ui-spec.md` / `.agents/skills/`）。
3. **推进了某个目标的状态吗（新结论 / 新证据 / 新未知）？** → 更新 `docs/ledger.md`。

三句全否允许不写，但要在回答末尾写清"三问全否 + 原因"；**不许连续两轮写"无新增"**。
一句话判据：**只对本项目有意义 → `docs/`；换个目标也用得上 → `.agents/skills/`。**

## 用户偏好（完整清单见 `docs/agents-ui-spec.md`）

要结果不要过程、讨厌嘴硬、讨厌重复劳动、喜欢一屏看全的干净密度、术语接地气、中文回答。
