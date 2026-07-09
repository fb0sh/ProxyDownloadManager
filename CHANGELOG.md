# 更新日志

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/spec/v2.0.0.html)。

## [Unreleased]

## [0.6.2] - 2026-07-09

### Fixed

- 暂停/恢复机制重写：引擎保存真实 task 列表到 gob，修复并发下载产生文件空洞的损坏问题
- `download_task` 失败时仅重试未写入部分（`TaskError` 格式），修复 `bytes_written` 重复计数
- `Range: bytes=X-Y` 收到 HTTP 200 时按致命错误处理（服务器忽略 Range 时全量写入错误偏移）
- 移除客户端级 120 秒整请求超时，改为 per-request 超时（慢速下载不会超时断开）
- `delete_download` 中 `.pdm` 路径计算修正（`with_extension` → `format!("{}.pdm")`）
- worker 清理竞态修复：用 `Arc::ptr_eq` 校验 entry 归属，暂停→恢复不会误删新 worker
- 前端 `listDownloads` 归一化 status 字段（`failed:<msg>` → `failed`），修复 failed 状态匹配
- `start_download` 与 `redownload_download` 先插 DB 再 spawn worker，消除僵尸任务竞态
- `WorkerPool` 满时 `try_acquire_owned` 立即返回错误而非无限挂起
- `SingleDownloader` 取消时保存进度状态，支持单线程下载的断点续传
- 日志截断改用 `char_indices` 安全边界，修复多字节字符 panic
- 前端连接选项移除 64（后端上限 32），两上限统一
- `check_update` 修复硬编码 UA 版本号 + 添加 30s 超时

## [0.6.1] - 2026-07-09

### Fixed

- `redownload_download` 现在分配新 ID 而非复用旧 ID，与领域模型一致
- `resume_download` gob 丢失时保持同一 ID 重新下载，不再回退到 redownload（分配新 ID）
- ConcurrentDownloader 失败后自动降级到 SingleDownloader，避免临时网络错误直接导致下载失败
- `open_file` 注册到 Tauri 命令处理器，修复右键菜单「打开」按钮不工作的问题
- `delete_download` 改为等待 worker 彻底停止后才删除文件，修复删除竞态
- `pause_download` 发送前端事件 `download-paused`，不再依赖 1s 轮询
- `resume_download` 从 DB 读取 `resumable` 字段决定引擎选择，不再硬编码 `supports_range: true`
- 新增 `WorkerPool::cancel_and_wait` 方法，确保取消后 worker 完全停止
- ConcurrentDownloader 暂停/取消时保存真实 task 列表到 gob 状态，修复恢复后文件空洞损坏问题
- SingleDownloader 取消时也保存进度状态，支持单线程下载的断点续传
- download_task 失败时仅重试未写入部分（TaskError 格式），修复 bytes_written 重复计数
- 完整性检查改为队列空 + 字节计数双重校验
- 移除客户端级 120 秒整请求超时，改用 per-request timeout（probe 30s/test_proxy 10s/check_update 30s）
- check_update 修复硬编码 UA 版本号 + 添加 30s 超时
- 前端 `listDownloads` 归一化 status 字段（`failed:<msg>` → `failed`），修复 failed 状态匹配
- 修复 `delete_download` 中 `.pdm` 路径计算（`with_extension` → `{path}.pdm` 拼接）
- 前端连接选项移除 64（后端上限 32），两端统一
- 修复 worker 清理竞态：用 `Arc::ptr_eq` 校验 entry 归属，暂停→恢复不会误删新 worker
- WorkerPool 满时 `try_acquire_owned` 立即返回错误而非无限挂起
- `start_download` 先插 DB 记录再 spawn worker，修复下载完成早于 DB insert 导致的僵尸任务
- `redownload_download` 同样修正 DB 插入顺序
- 日志截断改用 `char_indices` 安全边界，修复多字节字符 panic
- PropertiesDialog 和 DownloadDetailsWindow 中的长 URL 默认截断一行显示，右侧复制图标按钮可复制完整 URL
- 修复 Windows 上「设置」保存按钮无响应的问题：`sync_autostart` 失败不再阻塞整个 save，改为日志记录

## [0.6.0] - 2026-07-08

### Added

- 全局快捷键 `Ctrl+Super+J` 呼出主窗口（macOS: Control+Command+J，Linux/Win: Ctrl+Win+J）

### Fixed

- 系统通知不再被 `list_downloads` 查询失败阻塞，添加错误日志和 Web API 兜底
- 通知 `sendDownloadNotification` 不再使用空的 `catch {}` 吞掉错误

### Added

- 产品展示页 `src-present/`（独立 GitHub Pages 项目）
- 在线演示部署 CI（`.github/workflows/pages.yml`）
- 移动端适配：横向滚动演示窗口、响应式字体和布局

## [0.5.0] - 2026-07-08

### CI

- Linux 构建合并为一次编译 — `--bundles deb,appimage,rpm` 避免三次重复编译
- Release 页面使用 `CHANGELOG.md` 内容替代自动生成的 PR 标题

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

[0.6.2]: https://github.com/fb0sh/ProxyDownloadManager/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/fb0sh/ProxyDownloadManager/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/fb0sh/ProxyDownloadManager/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/fb0sh/ProxyDownloadManager/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/fb0sh/ProxyDownloadManager/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/fb0sh/ProxyDownloadManager/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/fb0sh/ProxyDownloadManager/releases/tag/v0.3.0
