---
name: exmt-modbox
description: Work inside the EXMT Modbox repository (C:\Dev\EXMT) — a Tauri 2 + Vue 3 desktop mod toolbox for the Unity game GF2 Exilium. Use when changing the EXMT shell/UI, when touching the Rust asset core that parses or patches UnityFS bundles, when extracting or replacing GF2 meshes/textures/animations, when building or exporting mods through the Blender add-on, or when verifying a UI/asset change in the real app.
metadata:
  version: "1"
---

# EXMT Modbox

EXMT 是一个 Tauri 2 + Vue 3 + Rust 资产核心的桌面 Mod 工具箱，目标游戏是 Unity 2019.4 + IL2CPP 的《少女前线2：追放》。
本 skill 只保留**判据、做法、坑与常量**；按任务查下面的参考页，不要通读。

## 先看哪里

| 任务 | 参考 |
| --- | --- |
| 仓库结构、本机命令、质量门、GameCache、验收方法、Windows 坑 | `references/project-and-toolchain.md` |
| 改界面 / 侧栏 / 页面分工 / i18n / 用户偏好 / 完成通知 | `references/ui-spec.md` |
| 模型、骨骼、贴图、动画、Mod 打包、Blender 插件、工作空间 | `references/asset-pipeline.md` |
| 跨领域的坑（编码、PowerShell、CDP、Unity 资产、UI） | `references/pitfalls.md` |
| GF2 的保护层、UnityFS 加密、IL2CPP、运行时 hook、关键地址 | `references/game-recon.md` |
| 选路线：Unity 拦截阶梯 vs Unreal pak 覆盖、成本判据 | `references/feasibility.md` |
| Rust 资产核心 `core-rs/`：模块、进度、验收、格式陷阱 | `references/rust-core.md` |

## 核心不变量（动代码前先确认）

1. **GameCache 是唯一事实来源**：主页 / 模型页 / 动作栏只读 `<缓存目录>\GameCache\`，不各自扫包。
   `skins/<key>/meta.json` 的 `cache_built_at` 必须等于 `manifest.json` 的 `built_at`；整个 analyze 只取一次时间戳。
2. **启动三态**：`loading → ready | setup | error`。`ready` 需要 setup 标记 + 游戏路径可用 + `cacheState()==="ready"`。
   不满足就进向导，不要在页面里偷偷扫包绕过。
3. **GF2 的 LocalCache bundle 只有前 `0x8000` 字节加密**（XOR，密钥由原密文头 4 字节派生），其余是明文；
   写回时必须由**原密文前 4 字节**派生密钥流。
4. **改包只重建与目标区间相交的块**，其余块按原 `compressedSize` 逐字节搬运；不要全量解压多块 LZ4 流。
5. **骨骼表按网格各自携带**：生成 Mod 前先按哈希求并集；顶点组名必须与骨骼名逐字相同。
6. **导航只有首页 / 模型 / 资源库 / 设置 4 页**；不要再加工具、日志这类占位页。
7. **文案一律走 `t("key")`**，中文是源字典，英文必须补齐同 key；模板里不许写死中文。

## 标准工作流

### 前端 / UI 改动

1. 复用已在运行的 Vite（1420 有 dev server 时 `tauri dev` 会复用）；小改动不要冷启动新 exe。
2. 改完跑 `npx.cmd vue-tsc --noEmit` 与 `npx.cmd vite build`（PowerShell 下 npx.ps1 被执行策略拦）。
3. 用 CDP 或 `app2.ps1` 读真实 DOM / 截图，确认宽度、选中态、文案，而不是只看类型检查。
4. 折叠/展开这类交互要同时验证「展开态点击」与「折叠态点击」。

### 核心 / 资产改动

1. 运行时只有 Rust：`core-rs/` + `src-tauri/src/`，没有 Python 回退。命令经前端 `runCore([...])` 进入 `run_core`。
2. 改 `analyze` 产物格式时给 `FORMAT` 加版本；皮肤推导规则改动只需重跑 analyze，不依赖外部进程池。
3. 真机样例：`D:\Program Files\GF2Exilium\...\AssetBundles_Windows`；单包可用 `EXMT_GF2_BUNDLE` 指定。
4. 验收：`cargo test`（core-rs / src-tauri）+ 真机 `analyze` / `workspace` 产物；资产往返用「打包回 bundle 再解出来逐位相等」。

### 改包（贴图 / 网格 / 材质）

1. 备份原包；改完还原并用 SHA256 校验。
2. Rust `exmt-core` 解析 SerializedFile，改属性。
3. 只重建与目标区间相交的块，其余搬运；更新 `cbis/ubis/size`。
4. 只对前 `0x8000` 施加由原密文头派生的密钥流。
5. 启动游戏验证；失败先区分「加密不对」还是「资源 ID/地址不匹配」。

### Blender 侧

- 插件在 `C:\Dev\EXMTBlender`（独立仓库），安装副本在 Blender 的 addons 目录。
- headless 验证：`blender --background --factory-startup --python <check>.py`，先清场再渲染。

## 交付清单

- [ ] `bun run check` 通过（类型 / cargo check / core-rs 测试 / 文档）。
- [ ] 四关自检：反证（把坏造一次，检查必须变红）、敏感指标、前提假设、用户视角。
- [ ] 可见变更附真窗口截图，并说明截图对应哪次改动。
- [ ] 改过的仓库都 commit + push。
- [ ] 发 WxPusher 完成通知：只能用 ASCII，路径字符集限制见 `references/ui-spec.md`。
- [ ] 新判据写回本 skill（`references/` 或新的 skill），不要往代码注释或聊天记录里堆。
