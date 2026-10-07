# 更新日志

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/spec/v2.0.0.html)。

## [Unreleased]

### Fixed

- 修复 0.20.0 引入的详情窗口空白：播种请求头编辑器的 `useEffect` 被放在了组件的提前 return 之后，下载项从「未加载」变为「已加载」时 hook 数量变化，React 抛 `Rendered more hooks than during the previous render` 导致整个详情窗口白屏；现已移到提前 return 之前，并补了一条能复现该崩溃的 jsdom 渲染测试（项目没有 eslint，tsc 也看不见这类问题）

## [0.20.0] - 2026-10-07

### Added

- 详情窗口新增「高级」页签、新建下载对话框新增「高级」区：可查看并编辑该下载将重放的请求头，支持直接粘贴 DevTools / `curl -H` 的请求头文本导入；「保存并重试」会用新请求头重新探测后继续下载，已下载的字节不丢

### Changed

- 媒体浮窗位置改为贴住媒体元素上方并带指向箭头（与 NeatDownloadManager 一致），跟随滚动与缩放；找不到媒体元素（MSE/blob 播放器、XHR 拉的 m3u8）时回退到右下角，手动拖拽后不再自动吸附；为放得进播放器上方改用紧凑密度（22px 行高、操作按钮移入标题栏）
- 重放请求头改为「只挡逐跳与框架头」：此前是白名单，`x-playback-session-id` 这类源站用来鉴权或标记播放会话的自定义头会被静默丢弃——这正是「浏览器里能放、ProxyDM 下载 403」的常见原因；现在浏览器发过的头都会重放，只有 Host、Connection、Content-Length、Content-Encoding、Range / If-Range、HTTP/2 伪头等会被挡掉

### Fixed

- 修复媒体浮窗在刷新或重新进入页面后不出现：嗅探列表此前从不在页面导航时清空，新页面的第一个媒体请求会被去重掉、于是不再推送；现在导航即清空，页面也会在加载完成、重新可见时主动追问一次
- 修复媒体浮窗拖拽：改用 Pointer Capture，指针移到播放器 / iframe 上方时不再丢失移动事件（此前用 mousemove，正是视频站必踩的坑）

## [0.19.0] - 2026-10-07

### Added

- 浏览器扩展新增可选的媒体浮窗：页面右下角列出本页嗅探到的媒体，点条目即交给桌面端下载；可拖拽、空闲 15 秒淡到 45%、可折叠与关闭，使用应用主题色并跟随系统深色
- popup 新增两个独立开关：「媒体嗅探」（关掉后不再监听媒体，下载拦截照常）与「显示浮窗」，可分别控制；浮窗内「此站不再显示」可按站点永久隐藏，隐藏的站点在 popup 里可一键恢复

### Fixed

- 浏览器扩展现在会捕获浏览器真实发送的请求头并原样重放（`Origin`、`Referer`、`Accept`、`Authorization` 等）：此前从不发 `Origin`，嗅探到的媒体的 `Referer` 还只带域名（`details.initiator` 只有 origin），校验来源或防盗链的源站会返回 403
- 扩展拦截普通下载时捕获的请求上下文，此前错误地存的是响应头、还被请求头白名单过滤成空集合，实际等于没有重放任何请求头；现在改为在 `webRequest.onBeforeSendHeaders` 捕获真实请求头

## [0.18.0] - 2026-10-07

### Fixed

- 源站如果限制单次 206 响应的长度（只返回所请求区间的一部分），续传会被误判为已完成：已知总大小时报「下载不完整」而失败，未知总大小时会更糟——把被截断的文件当成完整文件重命名。现在会按 Content-Range 里声明的总大小继续请求剩下的部分

## [0.17.1] - 2026-10-06

### Fixed

- 源站中途不再发数据时，暂停和删除立即生效，不再等到 30 秒无数据超时

### Changed

- 单连接下载和 HLS 分片现在与多连接下载一样，遇到网络错误、超时或 408/429/5xx 会按退避自动重试（次数用设置里的重试次数）；单连接下载重试期间状态显示「重试中」，401/403/404 等仍然直接失败
- HLS 分片也按 30 秒无数据判定卡住，并和其他下载一样缓冲写入
- 应用图标、托盘图标、浏览器扩展图标与 favicon 全部换为新的粗笔画下载标志：应用/扩展/store 图标保留白色圆角底、四周透明，macOS 菜单栏模板图标改为按笔画浓淡生成的黑白透明图

## [0.17.0] - 2026-09-30

### Added

- 设置里新增“同时下载任务数”（默认不限制），取代此前固定 8 个并发下载的隐藏上限
- Auto 连接数改为按实测吞吐自适应升档/降档（4→8→16→32→64），实测无提升或有新重试/卡顿就回退；Manual 选择完全按用户设置执行，不再被动态改写
- 探测/下载日志新增协议与启动分阶段耗时（protocol / first-header / first-body / first-progress），每 5 秒一条 `[perf]` 摘要

### Changed

- 全应用只有一个 `NetworkPool`：Direct 或同一代理下的探测、所有 Range worker、所有重试共享同一个 `reqwest::Client`，不再每个 worker/每次重试新建
- 探测 `Range: bytes=0-0` 的小 206 body 会被安全读完（上限 64 KiB），让 HTTP/1.1 连接回到连接池供随后的下载复用；巨大 200 响应仍然直接丢弃、不读入内存
- Auto 初始连接数收敛为小文件 1、其余 4，再按吞吐升档；不再按“是否走代理”写死上限
- Windows NSIS 安装包在升级/同版本重装时不再弹出 “Already Installed” 选择页：检测到旧版本后自动运行旧卸载程序（静默、不勾选删除应用数据），旧文件释放后再安装新版本；降级仍遵守 allowDowngrades，旧卸载失败会明确报错并停止

## [0.16.3] - 2026-09-29

### Fixed

- 全局限速只由后端保存一次，工具栏不再用一份旧设置覆盖其他字段；0 始终表示不限速
- 新建窗口已经探测过的地址，开始下载时 20 秒内直接复用，不再做第二次探测
- 下载进度按 256 KB 或 250 毫秒刷出，不再等每个连接攒满 1 MB 才显示已下载
- 只有连续约 30 秒收不到任何数据才算卡住；限速等待和慢速持续传输不再被当成失败重试
- 单个分块重试不再把整个任务在「下载中」和「重试中」之间来回切换

### Changed

- 自动连接数最多 8 路，大文件不再默认开到 16 或 32
- 设置里的线程数是新下载的默认值，不再作为上限；设成具体数值后不再按文件大小自动选择，选自动才按大小决定
- HTTP 客户端启用 HTTP/2，Range 请求行为保持不变

## [0.16.2] - 2026-09-29

### Fixed

- 自动连接数的大文件按探测时规划的分段下载，不再把整个文件当成一个分块停在 0 B
- 分块请求只保留引擎自己的 Range，并固定 Accept-Encoding 为 identity，避免浏览器回放的 Range 被追加进去
- 等待响应头超过 30 秒、或一个分块长时间没有任何进度时，会超时并消耗重试次数

### Changed

- 下载详情里的代理、连接数和限速与引擎实际能力一致：不支持的选项禁用；下载中切换代理会先停掉旧连接，再用同一个任务继续
- 详情页的代理和连接选项不再重复“代理”“连接”前缀

### Added

- 浏览器扩展弹窗显示扩展自身版本和桌面下载器版本

## [0.16.1] - 2026-09-29

### Fixed

- macOS 构建能通过：分配文件图标位图时使用 objc2::AnyThread

## [0.16.0] - 2026-09-29

### Added

- 主列表文件名和下载详情共用系统文件图标：文件已完成时优先用真实文件的图标，否则按扩展名取系统类型图标；系统查不到时用内置图标。结果进内存和磁盘缓存，相同类型只查询一次
- 展示页列表和详情使用同一套按类型区分的文件图标

## [0.15.0] - 2026-09-29

### Changed

- 下载详情改为 560×320 的横向单页：窗口标题用文件名，正文用三行信息、一条总进度和一条连接分段；底部可改代理、连接数和限速
- 展示页嵌入同一详情窗，说明改成一屏里的状态、速度、代理和连接进度

## [0.14.1] - 2026-09-29

### Changed

- 工具栏把日志、浏览器扩展、关于和退出移回可见按钮

### Fixed

- 详情页状态跟随界面语言；失败原因不再把同一个 HTTP 状态码显示两遍；连接、重试、合并阶段也会估算速度；限速选项与工具栏对齐，并保留当前不在预设里的连接数和限速

## [0.14.0] - 2026-09-29

### Added

- 浏览器扩展上架物料：双语 `_locales`、Edge 商店文案、权限理由、认证说明与提交作业单（`browsers-extension/store/`）
- 隐私政策页 `public/privacy.html`，随现有 Pages 工作流发布
- 架构说明 `docs/architecture.md`；拉取请求与 main 推送走 Check 工作流（类型检查、前端测试、`cargo fmt` / `check` / `test`），打 tag 构建前先跑同一组检查

### Changed

- 扩展 manifest 的 `name` / `description` 改用 `__MSG_` 占位符并补 `default_locale`（chrome / edge / firefox），使商店能识别 en 与 zh_CN 两种语言
- 在线演示站重做：与当前 shadcn 界面一致，内嵌可操作主窗口，去掉剪贴板宣传
- HLS 走统一 WorkerPool：分片流式写入临时目录再按序拷贝合并，暂停/删除由 worker 管理
- 初始 worker 与运行中加的 worker 共用同一套错误处理，全部 join 之后才合并落盘
- 连接数选 Auto 时任务里保持 0，由 worker 按文件大小决定实际连接数；探测建议不再覆盖这个选择
- 热路径日志降为 debug；日志在 ERROR 和退出时刷盘
- 主窗口收成一行工具栏（新建 / 继续 / 暂停 / 删除 / 限速 / 设置），状态过滤与类型过滤分开；右键菜单作用在被点中的那一行
- 新建下载对探测做防抖，后返回的旧请求不能覆盖新结果；详情窗展示失败原因、分片进度和连接/限速，仅失败或暂停时可刷新 URL
- 设置按下载、网络、应用三节排布

### Fixed

- 展示页内嵌的主窗口丢失全部 Tailwind 工具类（工具栏竖排、表格挤到右侧），在 `src/index.css` 显式声明 `@source "../src"` 让扫描覆盖 `src/`
- 展示页的「新建下载」「详情」不再用产品里不存在的简化弹窗，改为真机的 `NewDownloadWindow` / `DownloadDetailsWindow`，以第二个窗口的形式盖在主窗口上（演示框加高到能容纳 560px 的新建窗）
- 未写完的文件不再标成 Completed：必须刷盘、同步、临时文件改名成功之后才记完成
- 206 的 Content-Range 缺失或与请求不符时不写入正文；正文短于已知长度时保持未完成
- 同名任务不再共用临时文件，临时路径按下载 id 隔离
- 暂停、恢复和收尾失败时保持暂停，不发完成事件；队列中的任务取消后回到暂停
- 只带阶段、不带字节数的进度事件不再把已下载字节清零
- 已被取消替换掉的 worker 不再上报错误
- 浏览器扩展接管下载时复用已连接的 WebSocket，先快速确认再取消浏览器任务；ProxyDM 离线或确认失败时仍由浏览器自己下载，不再把小文件下完才取消

## [0.13.2] - 2026-09-24

### Fixed

- macOS 升级后不再沿用第一次拷贝到 Application Support 的旧扩展；启动时按应用版本同步 chrome/edge/firefox

## [0.13.1] - 2026-09-23

### Fixed

- 彻底移除剪贴板自动弹出新建下载
- 浏览器扩展弹窗改用 shadcn Neutral 配色，文案跟随浏览器语言
- 点击已捕获的媒体条目可以真正发给桌面端下载

## [0.13.0] - 2026-09-23

### Added

- 设置里可选择默认代理
- 下载详情改为按连接编号的横向进度条（默认显示 5 条，可滚动）
- 探测 3 秒无响应时自动改用默认代理再试

### Changed

- 工具栏与控件统一为 13px / 32px 高度
- 开始下载后只打开详情窗，不再拉起主窗口
- 复制 URL 不再自动弹出新建下载；仅浏览器点击劫持或右键「用扩展下载」
- 全局限速改为共享令牌桶，保存设置时立即生效

### Fixed

- 全局限速未作用到正在进行的下载

## [0.12.2] - 2026-09-23

### Changed

- 应用图标与 macOS 托盘图标更换为新的下载箭头标识

## [0.12.1] - 2026-09-23

### Changed

- 界面改用 shadcn/ui 默认 Neutral 主题（取消自定义青绿配色）

## [0.12.0] - 2026-09-23

### Added

- 浏览器扩展结构化 DownloadRequest（Cookie / Referer / Authorization 等）与 `{request_id, accepted, reason}` ACK；桌面离线时保留浏览器原下载
- 探测与分段下载共用同一请求上下文；Cookie / Referer / Basic Auth 保护资源的 mock 测试
- 下载状态 Connecting / Retrying / Merging；失败记录 error_code / http_status / retry_count
- 连接数硬上限 64，Auto 按文件大小分档；ChunkQueue 尾部分片窃取；下载中可改连接数与限速
- 按错误类型重试（429/5xx/timeout 退避 + jitter；401/403 不自动重试）
- 代理用户名/密码（HTTP / HTTPS CONNECT / SOCKS5）、文件冲突策略、Refresh URL、基础 HLS（master/media → 合并 `.ts`）
- 扩展 popup：连接状态、拦截开关、媒体嗅探、Alt/Delete 跳过本次、忽略规则
- 新建下载异步 probe、目录选择器、Download Later；主列表搜索/类型过滤与行级重渲染
- 前端迁移到 shadcn/ui + Tailwind CSS v4 + Lucide

### Changed

- WebSocket 兼容旧版纯 URL / `{action,url}` 协议
- 进度库刷盘间隔改为 3 秒；UI 进度事件仍约 500ms
- 日志对 Authorization / Cookie / Proxy-Authorization 输出 `<redacted>`
- README「最高 64 线程」与引擎行为对齐

### Fixed

- `execute_download` / resume 不再丢弃请求头
- 设置里 Auto（0）不再被当成 1 连接上限

## [0.11.0] - 2026-07-26

### Added

- 引擎接缝测试：重试耗尽不降级、Range 失效降级（先作废记录）、mock 服务器端到端
- resume / 崩溃恢复首批测试：对账规则、不伪造前缀、恢复计划完整性、陈旧事件门闸
- 前端事件接缝 `downloadEvents`：类型化事件载荷 + 唯一缓存写入策略 + 订阅接线测试
- 文件名净化：Windows 保留字符 / 设备名（CON、NUL…）/ 结尾点空格 / 长度上限

### Changed

- **进度账本（Progress Ledger）**：下载进度的唯一所有者。原分散 5 处的对账公式收拢为一条 `reconcile` 规则；resume 计划由 `begin_resume` 一次成型，不再「构造后补丁」
- **引擎接缝封死**：降级有原则——重试耗尽保留进度标记失败（可恢复），Range 中途失效才降级；降级顺序固定为「先作废全部进度记录 → 再截断 .pdm」（任意时点崩溃都安全）；取消标志只表示用户暂停；`.pdm` 临时文件约定收拢至一处
- 进度地图状态规则成为代码（Completed 全格 100%）；表格 / 属性弹窗 / 详情窗共用同一总百分比公式；详情窗按钮显隐决策纯函数化
- 数据库进度写入合并为单语句，且仅作用于 downloading 状态的行

### Fixed

- 崩溃恢复不再为多分片下载伪造连续前缀（此前可能造成带空洞文件的静默损坏）
- 暂停瞬间在途任务丢失导致恢复快照覆盖不全；旧版本写坏的恢复文件被一致性守卫拒绝
- 降级截断后旧进度记录 / 排队事件复活的竞态
- resume：并发数满时状态正确回滚；已下载完但未改名的任务直接收尾而非整文件重下；Completed / Downloading 状态不可再 resume
- redownload 把完整文件路径当保存目录导致嵌套路径
- Windows：详情窗无法弹出 / 聚焦（窗口能力缺失）、非法文件名下载失败、`.pdm` 改名句柄占用（含杀软重试）、开机启动注册表路径未加引号、打开文件闪控制台窗口与 explorer 假错误、托盘图标在任务栏不可见、内嵌 WebView2 引导器
- 详情窗永远显示英文；通知标题未国际化；剪贴板 URL 自动检测在 WebView2 下失效
- 单引擎降级后每 MB 同步写库；速度计算 `computeBps` 导出并补测试

## [0.10.0] - 2026-07-21

### Added

- 下载详情窗 **进度地图（Progress Map）**：一格一个固定分片，每行 8 列，绿色自下而上填充并显示百分比
- 详情窗标题区：总进度条 + 速度 + 暂停/继续；进度地图置于第二卡片
- 引擎按写入 offset 归入固定 `DownloadPart`，进度事件携带 per-part 字节

### Fixed

- 暂停/恢复后分片进度不再卡住；resume 使用 gob/DB 剩余任务，禁止把 `downloaded` 清零
- 崩溃/强退后：约 1s 刷盘 `downloaded`+`parts`；启动恢复用 DB 重建 resume
- 详情速度显示（修正采样门槛与重渲染干扰）；操作按钮点击后立即禁用直至完成
- 多选操作后保持勾选状态

## [0.9.7] - 2026-07-20

### Added

- `PdmError` 实现 `serde::Serialize`，Tauri 命令返回结构化错误（tagged union），前端可匹配 `error.kind`
- 前端新增 `PdmError` 类型（`src/types.ts`）
- `DownloadStateFacade` 新增生命周期方法 `on_paused(id)` / `on_deleted(id)`，封装 gob 持久化决策
- 引擎集成测试：mock HTTP server + 4 个端到端测试（单引擎/并发引擎/进度事件/取消）
- `EventTransformer` 纯函数：引擎事件 → 结构化 `EventAction`，6 个单元测试
- `tauriClient.ts` 完整化：8 个新命令（exitApp, openFile, readLogs, getExtensionsDir, openExtensionsFolder, getFileIcon, checkUpdate, testProxy）
- `DialogRenderer` 共享组件：消除 App.tsx 与 src-present/App.tsx 的对话框渲染重复
- `useDialog()` / `useSelection()` 自定义 hooks，AppContext 深化
- 浏览器扩展 `shared/` 共享源码 + `build.sh` 构建脚本
- `patchDownloadProgress` 纯函数及 5 个 vitest 测试

### Changed

- `DownloadManager` 移除 `network` 依赖，`test_proxy`/`check_update` 直接通过 `AppState` 调用
- 所有 Tauri 命令返回 `Result<T, PdmError>`（不再 `.map_err(|e| e.to_string())`）
- `save_gob` → `save_resume_state`，`load_gob` → `load_resume_state`（语义更清晰）
- 前端 `invoke()` 调用全部经 `tauriClient`，零裸 invoke
- `parse_message()` 从 WebSocket handle_connection 提取为纯函数 + 5 个测试
- `sendReliable()` 签名简化（移除未使用的 referrer/tabTitle 参数）

### Fixed

- 19 个空 `catch {}` 块全部添加注释（预期失败 vs 需追踪的异常）
- 浏览器扩展删除死代码 `send()` 和 `looksLikeDownload()`
- 移除 content.js + `host_permissions: ["<all_urls>"]`（无需权限的空脚本）
- 移除未使用的 zustand 依赖

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
