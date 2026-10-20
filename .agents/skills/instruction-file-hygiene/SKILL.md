---
name: instruction-file-hygiene
description: Restructure a repository's agent instruction file (AGENTS.md / CLAUDE.md / .cursorrules) so a model actually follows it — a short imperative rules layer on top and long-form detail (environment, quality gates, ledgers, correction logs) in docs/ below it. Use when an instruction file has grown into a running log or operations manual, when rules are buried in history, when a model re-learns the same lessons, or when a "correction ledger" produces no enforceable rule.
metadata:
  version: "1"
---

# 指令文件分层（AGENTS.md 瘦身）

适用场景：仓库里的 `AGENTS.md`（或 `CLAUDE.md` / `.cursorrules`）被当成**流水线/操作手册**写：
环境、清单、历史纠正全堆在一起，规则被淹没 → 模型读不完、记不住、重复犯错。

**目标结构（四层，各司其职）**

| 层 | 文件 | 放什么 | 长度纪律 |
| --- | --- | --- | --- |
| 规则层 | `AGENTS.md` | 只放**可执行的约束**（铁律）+ 写回规则 + 指针表 | 越短越好（本仓库 55 行 / 12 条铁律）；**只增结论，不增细节** |
| 细节层 | `docs/agents-playbook.md`、`docs/agents-ui-spec.md` | 环境、工具链、命令、质量门、交付清单、UI 规范、用户偏好 | 可以长；一条事实一行 |
| 台账层 | `docs/ledger.md` | 目标状态、已确认事实+证据、未决问题 | 只写结论，不写"先做 X 再做 Y" |
| 能力层 | `.agents/skills/<skill>/` | 换目标也用得上的判据/手法（+ `references/<case>.md` 实例） | 通用化，剥离项目专有名词 |

**Global layer vs workspace layer (DSH)**

DSH reads cross-project rules from `~/.dsh/AGENTS.md` (`%USERPROFILE%\.dsh\AGENTS.md` on Windows),
a different file and a different scope from the workspace `AGENTS.md`.

- "Append to the global rules / core AGENTS.md" means that file, not the workspace one. If it does not
  exist, create it there; do not invent `~/.agents`, `~/.config/dsh`, or anything inside the app bundle
  (`resources\app.asar` is packaged and gets replaced on update).
- Precedence: workspace instructions are more specific and win on conflict.
- Activation: read at session start. For an already-running session, `dsh-agent-instructions` detects the
  change and injects `Additional instructions from: ~/.dsh/AGENTS.md` after the next file-tool call, so
  no restart is needed.
- Proof the path is right: that `Additional instructions from:` block shows up in the conversation. Read
  the file back to confirm content; do not infer success from the write returning "Created file".

**纠正台账单独一层**（`docs/agents-corrections.md`）：逐条追加的流水账，允许长、允许乱序，
但**每一条必须落成一条规则**——落进规则层、UI 规范或 skill。
只写"以后注意"而不改规则的条目 = 没写完。冲突时以**更靠后的行**为准。

**规则怎么写才可执行**

- 反例：「注意 UI 不要太复杂」→ 正例：「改 UI 前先想用户会不会嫌多——默认更少；文案走 `t("key")`，下拉用 `Dropdown.vue`」。
- 每条规则自带**触发时机 + 具体动作 + 判据**（跑什么命令、看哪个文件、断言什么）。
- 规则层只允许**短句 + 指针**（"细节见 playbook §4"），不复述细节。
- 别在规则层里写编号交叉引用（`§3.2`）；写**文件名 + 小节名**，改名后仍可读。

**迁移步骤（一次性，约半小时）**

1. 列出现有文件的所有小节，按上表归位：规则留下、细节搬走、历史搬进台账。
2. 规则层重写成 10–15 条铁律，每条一行；把"违反过的"排在前面。
3. 交叉引用全改文件名；然后 `grep -n "见 §\|本文件 §" docs .agents` 确认没有指向旧编号的悬空引用。
4. 在台账里追加一条本次纠正（含用户原话），并给出规则落点；提交时一并带上。

**验收判据**

- 新模型**只读规则层**就能正确行动（改完跑什么、写回去哪、什么时候不许说"完成"）。
- 规则层行数不随项目增长（新知识进细节层/台账/skill）。
- 每次纠正都能追到一条规则；`grep` 不到悬空引用。

仓库实例（EXMT Modbox，2026-10-20 迁移：219 行 → 55 行，68 条纠正外移）见
`references/exmt-modbox-migration.md`。
