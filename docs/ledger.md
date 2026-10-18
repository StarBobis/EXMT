# 台账 · EXMT Modbox

> 结论 + 证据 + 未决问题。**不写流水账**（不记"先跑 X 再跑 Y"）。
> 每轮开始读这里，结束前更新这里。目标状态只在这里，不在 AGENTS.md。

最近更新：2026-10-18

---

## 1. 目标

| # | 目标 | 状态 | 验收 |
| --- | --- | --- | --- |
| G1 | Tauri 2 + Vue3 + TS 的 Modbox 外壳（WinUI 风格） | ✅ 完成 | 侧栏可折叠、自定义标题栏、任意路径读写 |
| G2 | 全角色目录 + 立绘 + 卡牌视图 | ✅ 完成 | 78 角色 / 240 皮肤 / 169 立绘，卡片不重复 |
| G3 | `.exmtmesh` 无损往返（导出→Blender→打包） | ✅ 完成 | 不改动直接打包，23 个字段 + 顶点流 + 索引**逐字节一致** |
| G4 | 换衣服（外部服装替换 cloth 槽） | ✅ 可用 | 夹克 / 宿舍装 / 校服 均在游戏内生效 |
| G5 | **独立身体替换（body 槽）** | ❌ 未完成 | 需要绑定空间重定向，见 §4 |
| G6 | VitePress 2 文档站 + 替换模型图文教程 | ✅ 完成 | `bun run build` 通过，9 步带截图 |
| G7 | 导出流程只保留自定义格式 | ✅ 完成 | glTF 代码/命令/按钮全部删除 |

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
