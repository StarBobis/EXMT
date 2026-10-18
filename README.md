# EXMT Modbox

本地资源与 Mod 工作台 —— 一个用 **Tauri 2 + Rust** 作外壳、**Vue 3 + TypeScript** 作界面的桌面工具箱。

当前阶段只做**界面骨架**：WinUI 3（Fluent）风格的侧边导航、自定义标题栏、明暗主题；业务模块（资源包浏览、贴图替换、运行时注入）尚未接入。

## 技术栈

| 层 | 选型 |
| --- | --- |
| 前端 | Vue 3.5 + TypeScript 6 + Vite 8 |
| 外壳 | Tauri 2.12 + Rust 1.99（MSVC 工具链） |
| 包管理 | Bun 1.4 |
| 目标平台 | Windows 10/11（WebView2） |

## 开发

```powershell
bun install          # 安装前端依赖
bun run tauri dev    # 启动桌面应用（Vite + cargo run）
bun run typecheck    # vue-tsc 类型检查
bun run build        # 前端生产构建
bun run tauri build  # 打包安装程序（MSI / NSIS）
```

前置条件：Rust（`rustup` + `x86_64-pc-windows-msvc`）、Visual Studio Build Tools 的 C++ 工作负载、Bun、WebView2 运行时。

## 目录

```
src/
  App.vue                 应用外壳：标题栏 + 侧边栏 + 内容区
  components/
    TitleBar.vue          自定义标题栏（拖拽区 + 最小化/最大化/关闭）
    SideBar.vue           WinUI 3 NavigationView 风格侧边栏
    AppLogo.vue           应用图标
  views/                  首页 / 资源库 / 工具 / 日志 / 设置
  lib/
    nav.ts                导航模型（唯一数据源）
    theme.ts              明暗主题（跟随系统 / 浅色 / 深色）
    tauri.ts              Tauri 运行时薄封装（浏览器预览时可用）
  styles/
    tokens.css            Fluent 设计令牌（明暗两套）
    app.css               基础样式与组件样式
src-tauri/
  src/lib.rs              插件注册 + 命令
  tauri.conf.json         窗口配置（decorations: false）
  capabilities/           权限（本窗口放开任意路径读写、任意 http(s)）
```

## 权限

`src-tauri/capabilities/default.json` 对本窗口放开了：

- **文件系统**：`fs:scope` 允许 `**`，可读写任意盘符、任意目录。
- **网络**：`http:default` 的 URL 作用域覆盖 `http://*`、`https://*`、`http(s)://*:*`，即任意主机与端口。
- **窗口**：拖拽、最小化、最大化/还原、关闭、设置标题等。

这些是为了让工具箱能直接读写用户指定的游戏目录、访问任意远端清单。后续如果要收紧，改成显式目录白名单即可（把 `fs:scope` 的 `**` 换成具体路径）。

## 备注

- 窗口使用 `decorations: false`，标题栏由前端绘制；拖拽依赖 `data-tauri-drag-region`，双击最大化由 Tauri 内部处理（需要 `core:window:allow-internal-toggle-maximize`）。
- 没有安装 `window-vibrancy`，Mica 质感用 CSS 渐变模拟，避免透明窗口在 Win10 上的兼容问题。
