# EXMT Modbox · UI 规范与用户偏好

> 基准：WinUI 3 / Fluent。改 UI 前先读这一页；这里只写可执行的规则，不写来历。

## 1. 布局与组件规则

| 区域 | 规则 |
| --- | --- |
| 侧边导航 | NavigationView：展开 272px / 折叠 48px；`☰` 在面板头部，与图标共用 40×40 盒子、`2px auto`；折叠时图标居中、文字淡出；状态存 localStorage；**展开状态下点击任意导航项进入页面后自动折叠**（折叠态点击不再改变宽度）。导航只有首页 / 模型 / 资源库 + 底部设置，**不要再加工具、日志这类占位页** |
| 窗口 | 初始尺寸按显示器工作区算，不写死：16:9，宽度 = 工作区 2/3，高度 = 宽×9/16，上限高 90%，下限 920×600，居中于工作区（避开任务栏）。见 `src-tauri/src/lib.rs` 的 `default_window_size` |
| 焦点 | 鼠标点选不出现蓝色 focus 框：`.nav-item:focus, :focus-visible { outline: none }`，改用 `--fill-subtle-hover` 填充 |
| 页面 | full-bleed：`.page { max-width: none; margin: 0; padding: 18px 24px }`。不要 `max-width + margin:auto` |
| 页面分工 | 首页 = 角色列表 + 立绘卡牌（唯一选皮肤入口，点卡选中，「进入模型页」进工作台）；模型页 = 子网格 / 关联贴图 / 9:16 竖版 3D（顶栏只放当前角色·皮肤、数量、工作空间下拉、「换皮肤」「开始制作」）。点左侧子网格才在中间显示它关联的贴图；3D 固定最右、9:16 竖版。皮肤选择走 `src/lib/selection.ts` 共享；模型页 `onActivated` 才加载 |
| 栅格 | 用 `minmax(0, Nfr)`；裸 `1fr` / `70% 30%` 会被子元素 min-content 顶宽，产生横向溢出 |
| 卡片 | 同一列表尺寸必须一致；缺图条目也占一张同尺寸卡（斜纹底 + 占位图 + 角标），不要降级成小 chip |
| 图片 | 立绘 `object-fit: contain`；头像/胸像用 `contain` + 顶部对齐；不要 `cover` 把脸裁成特写。立绘卡片宽高比必须等于立绘本身（380:752），靠宽度定高度。立绘区永远一行：当前卡居中，其余按环形距离 `(i-active+n/2)%n-n/2` 叠在左右 |
| 文案 | 中文、直白，不要黑话；按钮名要望文生义 |
| 多语言 | 文案一律走 `t("key")`（`src/lib/i18n.ts`），中文是源字典，`Record<keyof typeof zh, string>` 强制英文补齐同 key；模板里不许写死中文。语言存 localStorage `exmt.locale`，默认简体中文 |
| 密度 | 简洁：状态反馈一行足够，不要堆按钮和面板；操作日志面板、素体这类功能是被明确删掉的 |
| 批量 | 模型页顶部要有「全选 / 取消全选」：一次勾上全部子网格 + 全部贴图；全选后自动打开第一个有贴图的子网格；再点清空。默认全不勾 |
| 下拉 | 统一用 `src/components/Dropdown.vue`（自绘 listbox + Teleport + token 配色 + 键盘）；不要用原生 `<select>`（WebView2 的系统弹出层在暗色主题下白底白字） |
| 3D 选中态 | **点击子网格行（正在查看的那块）**必须覆盖贴图涂纯紫（`0xa855f7` + 暗紫自发光 `0x3b0f7a`，`roughness 0.92` 压掉镜面高光否则亮灯下洗成粉白），且 `depthTest/depthWrite = false` + `renderOrder = 1` **画在最前**：护目镜藏在头发下、躯干被外套包住，不前置的话紫了也看不见。勾选（会导出）不是这个信号，勾选不改贴图。取消查看/换皮肤/重建时必须还原贴图（贴图存 `mesh.userData.diffuse`；摘挂 map 要置 `material.needsUpdate`，USE_MAP 分支变了） |

## 2. 交付与协作偏好

- **要结果**：先给可验证产物（截图 / 文件 / 数字），再解释。
- **不确定就说没验证**，不要用「应该没问题」。
- **同样的活第二次要更快**：脚本化、库化，别重复手工。
- **一屏看全**：不要堆按钮和面板。
- **实时说进度**：不要闷头干活不说话。
- 中文回答，技术名词可保留英文。
- 可见变更必须用**真窗口截图**交付，不接受「类型检查过了 / 组件挂载了」。

## 3. 任务完成通知（WxPusher）

- 只能走 GET 路径：`https://wxpusher.zjiecode.com/api/send/message/<SPT_TOKEN>/<URL_ENCODED_TEXT>`。
- 简报用 `[uri]::EscapeDataString('中文简报')` 编码后拼进路径：**百分号编码的中文实测可用**（2026-10-20 / 10-21 均返回 `code:1000, success:true`）。不编码的中文会 400。
- **简报里不要出现 `/`**：即使 `EscapeDataString` 编成 `%2F`，服务端仍回 400（2026-10-21 实测）；写成 `BC1/3/4/5/7` 会失败，改成 `BC 解码` 这类说法。
- POST `/api/send/message` 用该 SPT token 会回 `appToken不正确`，不要用。
