# 实例：EXMT Modbox 的 AGENTS.md 分层（2026-10-20）

## 起点

`AGENTS.md` 219 行、7 个小节：仓库地图 / 工具链 / RSI 循环 / 硬规则 / 视觉规范 /
纠正台账（68 行表格）/ 用户偏好 / 质量门 / 目标状态。

症状（用户原话）：**「AGENTS.md 不应该是流水线，而是约束和规则」**——
规则只有 9 条，却被 68 行历史纠正 + 环境表格淹没；新会话读它等于读一份操作手册。

## 拆法（映射表）

| 原小节 | 去向 |
| --- | --- |
| 仓库地图 + 本机工具链（bun 全路径 / python / blender / 游戏路径 / app2.ps1 截图） | `docs/agents-playbook.md` §1–2 |
| 交付前固定动作 + 质量门（`scripts/check.ps1` 五步，含样例 bundle 路径） | `docs/agents-playbook.md` §3–4 |
| 硬规则里的"别造轮子"细节（`peek_name()` 15min→41s、texture2ddecoder、texconv） | `docs/agents-playbook.md` §5 |
| 写回半径表（复用半径 → 写哪） | 规则层留一句判据 + playbook §6 完整表 |
| 视觉与交互规范整张表 + 用户偏好（含微信推送的 ASCII/GET 细节） | `docs/agents-ui-spec.md` |
| 纠正台账 68 行 | `docs/agents-corrections.md`（逐行搬运，交叉引用改成文件名） |
| 剩下真正是"约束"的 12 条 | 新 `AGENTS.md`（55 行） |

## 搬运时踩到的坑

1. **交叉引用腐烂**：台账里有 `见 §3「卡片」`、`见 §2.5`、`见 §4 那条`。
   必须逐条改成 `见 docs/agents-ui-spec.md「卡片」`、`见 AGENTS.md 铁律 6` 这种**不依赖编号**的写法；
   最后用 `grep -n "见 §\|本文件 §" docs .agents` 确认没有悬空引用（本仓库剩下 21 处全是各自文档内部的 §，属正常）。
2. **表内自引用**：某行写"别用短名兜底"，下一行才订正它。搬运时**不要顺手删旧行**，
   在旧行里保留"已被下面那条订正"，否则后来人只看到错的那条。
3. **并发会话**：迁移途中 `HEAD` 被另一个会话推进（`9df403b` → `7adac3d`），`core/` 一直有别人的未提交改动。
   提交只 `git add <本轮的文件>`；`git add -A` 会把别人的 WIP 一起提交。
4. **编辑工具误伤（同一个坑犯两次）**：用 `edit` 往表格里插行时，`old_string` 只截了已有行的**行首**
   （如 `| 2026-10-20 | 仓库里有…`），替换后那行只剩半截（`| ：本轮中途 HEAD …`）。
   规则：**插行要么锚定整行，要么直接追加到文件末尾**；改完用脚本校验每行都匹配 `^\| (20|日期|---)`。
5. **顶层文件仍在被人类编辑**：迁移期间用户在 `AGENTS.md` 加了新规则（`bun` 不在 PATH、skills 第一优先级）。
   提交前重读一次工作区文件，别用自己内存里的版本覆盖；`edit` 报 "file changed since it was read" 就是信号。

## 迁移后的验收（本仓库实际跑的）

- 规则层 55 行 / 12 条铁律；只读它就能回答"改完跑什么、写回去哪、何时不许说完成"。
- `docs/agents-playbook.md` 109 行、`docs/agents-ui-spec.md` 41 行、`docs/agents-corrections.md` 82 行（72 条纠正）。
- 质量门 `bun run check`（vue-tsc / cargo check / compileall / 核心冒烟 / vitepress build）全绿。
- 台账顶部"最近更新" + §5 检查点同步指向新写回位置。
