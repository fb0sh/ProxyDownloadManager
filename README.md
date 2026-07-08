# ProxyDownloadManager

> 多代理下载管理器 — 基于 Tauri 2 + React 19 构建。

ProxyDownloadManager 是一款开源的多代理下载工具。每个下载任务可独立选择 HTTP / SOCKS5 代理，支持多候选代理自动切换。浏览器扩展一键拦截，多线程并发加速，断点续传不中断。

## 🚀 在线演示

[**👉 点此体验在线演示**](https://fb0sh.github.io/ProxyDownloadManager/)

无需安装，浏览器中即可体验完整的 ProxyDownloadManager 界面，包括下载管理、代理配置、实时进度等功能。

## 特性

- **每下载独立选代理** — 每个任务可选择不同代理（HTTP / SOCKS5），支持多候选代理自动切换
- **多线程并发下载** — 单任务最高 64 线程并行下载，自动根据文件大小调整连接数
- **断点续传** — 支持 HTTP `Range` 请求，中断后自动恢复
- **浏览器扩展联动** — Chrome / Edge / Firefox 扩展，右键菜单或自动拦截下载，URL 实时发送到桌面端
- **剪贴板智能监测** — 自动识别剪贴板中的下载链接
- **重试机制** — 下载失败自动切换代理和 User-Agent 重试
- **重复检测** — 文件已存在时提示，缺失时自动重新下载
- **下载日志** — 颜色分级日志，应用内可查看
- **系统托盘** — 后台下载、开机自启、通知推送
- **国际化** — 中英文界面
- **跨平台** — macOS、Windows、Linux

## 演示

<img width="1120" height="687" alt="image" src="https://github.com/user-attachments/assets/1aba9885-f2a4-471f-abe4-b3cf724c29c1" />

<img width="700" height="590" alt="image" src="https://github.com/user-attachments/assets/55858220-79f5-48b0-8ee8-0b73470a25a4" />

<img width="1570" height="949" alt="image" src="https://github.com/user-attachments/assets/404b5667-7b06-4441-ae21-be7be473890f" />

<img width="1570" height="949" alt="image" src="https://github.com/user-attachments/assets/5b5e34bc-8367-45cc-85e2-bbe479cdead2" />



## 下载

从 [Releases 页面](https://github.com/fb0sh/ProxyDownloadManager/releases) 下载对应平台的安装包。

| 平台 | 格式 |
|------|------|
| macOS | `.dmg` |
| Windows | `.exe` / `.msi` |
| Linux | `.deb` / `.rpm` / `.AppImage` |

## 浏览器扩展

ProxyDownloadManager 附带浏览器扩展，Chrome / Edge / Firefox 均可使用。

### macOS — 安装步骤

> 扩展文件位于 `~/Library/Application Support/com.fb0sh.proxydownloadmanager/extensions/`，需先显示 `~/Library` 文件夹。

1. 打开 **Finder** → 菜单栏 **前往**
2. 按住 <kbd>Option</kbd> 键 → **资源库** 出现，点击进入
   > 或按 <kbd>⌘⇧G</kbd>，输入 `~/Library` 回车
3. 进入 `Application Support` → `com.fb0sh.proxydownloadmanager` → `extensions`

#### Chrome / Edge

1. 打开浏览器，访问 `chrome://extensions` 或 `edge://extensions`
2. 开启 **开发者模式**（右上角开关）
3. 点击 **加载已解压的扩展**，选择 `extensions` 目录下的 `chrome` 或 `edge` 文件夹
4. 启用扩展，点击工具栏图标可开关下载拦截

#### Firefox

1. 打开 Firefox，访问 `about:debugging#/runtime/this-firefox`
2. 点击 **加载临时附加组件**，选择 `firefox` 目录下的 `manifest.json`
3. 扩展为临时加载，重启后需重新加载

### Windows / Linux — 安装步骤

1. 打开应用，点击工具栏 **Extensions**
2. 点击 **Open Folder** 打开扩展目录
3. 同上 Chrome / Edge / Firefox 的加载步骤

## 开发环境

### 依赖

- [Node.js](https://nodejs.org/) (v20+)
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/) (edition 2021)
- Tauri 系统依赖 — 参见 [Tauri 前置条件](https://v2.tauri.app/start/prerequisites/)

### 启动

```bash
pnpm install
pnpm tauri dev
```

### 项目结构

```
├── src/                          # React 前端
│   ├── components/dialogs/       # 各对话框组件
│   ├── components/               # Layout、Toolbar、DownloadTable
│   ├── hooks/                    # 自定义 Hooks
│   ├── i18n/                     # 中英文翻译
│   ├── stores/                   # Zustand 状态管理
│   ├── query/                    # TanStack Query 配置
│   └── App.tsx                   # 根组件
├── src-tauri/                    # Rust 后端
│   └── src/
│       ├── cmd.rs                # Tauri 命令（IPC 处理器）
│       ├── lib.rs                # 应用启动、插件、托盘
│       ├── engine/               # 并发/单线程下载引擎
│       ├── worker.rs             # 工作池
│       ├── network/              # HTTP 客户端池、速率限制
│       ├── ws/                   # WebSocket 服务器（浏览器扩展通信）
│       ├── state/                # SQLite 数据库、状态快照
│       ├── config.rs             # TOML 配置加载
│       ├── probe.rs              # URL 探测（文件大小、Range 支持）
│       └── types.rs              # 共享类型定义
├── browsers-extension/           # 浏览器扩展
│   ├── chrome/
│   ├── edge/
│   └── firefox/
└── docs/                         # 设计文档、截图
```

## 构建

```bash
pnpm tauri build
```

构建产物在 `src-tauri/target/release/bundle/` 目录下。

## 技术栈

| 层        | 技术 |
|-----------|------|
| 桌面框架   | [Tauri 2](https://v2.tauri.app/) |
| 前端      | [React 19](https://react.dev/)、[TypeScript](https://www.typescriptlang.org/)、[Vite](https://vite.dev/) |
| UI        | [Primer React 38](https://primer.style/react/)、[Octicons](https://primer.style/octicons/) |
| 状态管理   | [Zustand 5](https://github.com/pmndrs/zustand)、[TanStack Query 5](https://tanstack.com/query) |
| 后端      | [Rust](https://www.rustlang.org/)、[tokio](https://tokio.rs/)、[reqwest 0.12](https://docs.rs/reqwest/) |
| 存储      | SQLite via [rusqlite](https://github.com/rusqlite/rusqlite) |
| 代理      | HTTP / SOCKS5 via `reqwest` |
| 扩展      | Chrome MV3、Firefox Manifest V2 |

## 贡献指南

### 报告问题

- **Bug 报告** — 注明应用版本、操作系统、复现步骤，附上日志（应用内 Log → 复制）
- **功能建议** — 描述使用场景

###  Pull Request

1. Fork 仓库，从 `main` 创建分支
2. 功能型 PR 请先开 issue 讨论设计
3. 确保类型检查和测试通过：
   ```bash
   npx tsc --noEmit
   pnpm test
   cd src-tauri && cargo check && cargo test
   ```
4. 更新 README 如果改动影响用户可见功能
5. PR 标题和描述清晰

### 开发约定

- **前端** — React 函数组件 + Hooks，Primer React 保持 UI 一致，i18n 键在 `src/i18n/`
- **后端** — Rust Tauri 命令在 `cmd.rs`，下载引擎在 `engine/`
- **浏览器扩展** — Chrome/Edge 使用 MV3，Firefox 使用 Manifest V2，通过 WebSocket（`ws://127.0.0.1:18999`）与桌面端通信
- **提交格式** — [Conventional Commits](https://www.conventionalcommits.org/)：`feat:`、`fix:`、`docs:`、`refactor:` 等

### 添加语言

1. 创建 `src/i18n/xx.ts`，导出 `Translations` 对象
2. 在 `src/i18n/index.ts` 中注册
3. 在 Rust `types.rs` 和 TypeScript `src/types.ts` 中添加语言代码到 `Settings.language`

## 许可

[MIT](./LICENSE) © fb0sh
