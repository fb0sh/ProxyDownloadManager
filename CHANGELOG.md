# 更新日志

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/spec/v2.0.0.html)。

## [Unreleased]

## [0.4.1] - 2026-07-08

### Fixed

- 修复主窗口重新获得焦点时下载列表不刷新的问题 — 添加 Tauri 原生 focus 事件监听和 `refetchIntervalInBackground`
- 修复 app 重启后新增下载任务消失的问题 — WorkerPool ID 计数器改为从 DB 的 `MAX(id) + 1` 开始，避免主键冲突
- 修复 DB 写入错误被静默吞掉的问题 — `start_download` 在 insert 失败时打印错误日志
- 修复 CDN 跳转 URL 无法提取文件名的问题 — 新增 query 参数扫描和全文兜底策略

### Added

- 全链路日志增强 — Rust 后端（engine、worker、probe、pool、config、cmd）、前端组件生命周期、浏览器扩展 WebSocket 生命周期均添加结构化日志
- `Db::max_id()` 方法 — 用于跨重启持久化 ID 计数器
- `filename_from_url()` 三策略文件名提取：路径提取、query 参数 `filename=` 扫描、全文 `name.ext` 兜底
- About 对话框显示新版本的更新内容（GitHub Release body）

### Changed

- `WorkerPool::new()` 接受 `next_id_start` 参数替代硬编码的 `1`
- `NewDownloadDialog` 提交成功后也 `emit("download-created")`，与 `NewDownloadWindow` 行为一致

### Docs

- 新增 AGENTS.md — AI 开发工作流规范（任务流程、提交前校验、Changelog 纪律、授权规则）
- 新增 CHANGELOG.md — 中文更新日志

### CI

- 新增 check.yml — PR 提交时自动执行 TypeScript 类型检查 + Rust check/test + 前端测试（不构建安装包）

## [0.4.0] - 2026-07-08

### Added

- 浏览器扩展作为应用资源打包 — Chrome、Edge、Firefox 扩展随应用一起分发
- 更新检查对话框 — 查询 GitHub Releases API 检测新版本
- 国际化支持 — 英文和中文界面
- 完整 README — 功能列表、开发环境搭建、贡献指南
- macOS 浏览器扩展安装教程 — Finder → 资源库 → Application Support 路径指南
- macOS 自动部署扩展 — 首次启动时将扩展复制到 `~/Library/Application Support/<id>/extensions/`

### Fixed

- 扩展发送 CDN 跳转地址而非原始下载地址的问题 — 显示原始地址，后端 probe 自动跟随跳转

### Changed

- UI 重设计 — 所有对话框改用 Primer React GitHub 风格（属性、设置、新建下载、日志等）
- 默认窗口尺寸从 800×600 调整为 1020×587
- 代理解析修复 — `DownloadConfig.proxy_name` 存储解析后的代理 URL 而非代理名称
- Tauri 2 capabilities 更新 — 添加 `dialog:default` 和 `opener:default` 权限

## [0.3.0] - 2026-07-07

### Added

- 初始版本发布 — 支持代理的多线程下载管理器
- 多线程下载（每个任务可配置连接数）
- 断点续传（支持 HTTP Range 请求）
- 代理支持（HTTP/SOCKS5）
- 浏览器扩展（Chrome、Edge、Firefox）
- 系统托盘集成（最小化到托盘、后台下载、快速访问）
- 下载日志（颜色分级）
- 重复 URL 检测
- 重新下载失败/丢失的文件
- IDM 风格进度显示（流畅动画）
- 右键菜单（停止、删除、打开、打开文件夹、重新下载、属性）
