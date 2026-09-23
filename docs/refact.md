你现在负责仓库：

https://github.com/fb0sh/ProxyDownloadManager

目标：

将 ProxyDownloadManager 的核心可用性提升到 Neat Download Manager 同等级别。

当前阶段优先解决“作为日常下载器是否真正好用”，避免堆砌边缘功能。

基准产品：
Neat Download Manager

重点基准能力：

* 浏览器下载无感接管
* 多连接动态分段下载
* 稳定暂停 / 恢复 / 崩溃恢复
* 下载失败自动恢复
* 下载过程中修改连接数
* 下载过程中修改限速
* HTTP / HTTPS 下载
* HTTP / SOCKS5 代理
* HTTP Authentication / Proxy Authentication
* 浏览器请求上下文透传
* 音视频资源捕获
* HLS 下载与合并
* 过期下载链接更新后继续下载
* 低资源占用
* 简洁、高响应的 GUI

请完整阅读现有代码、README、AGENTS.md、CLAUDE.md、CHANGELOG.md 和现有测试后实施。

现有架构尽量保留：
React 19 + TypeScript + Primer React + Zustand/TanStack Query
Tauri 2
Rust + Tokio + Reqwest
SQLite
现有 engine / worker / ledger / network / WebSocket / browser-extension 分层

不要大规模重写项目。

---

# P0：浏览器接管链路重构

这是最高优先级。

当前 browsers-extension/shared/background.js 的主要问题：

浏览器下载被捕获以后，基本只向桌面端发送 URL。

这会导致大量真实网站出现：

403
401
签名链接失效
Referer 校验失败
Cookie 登录态丢失
CDN 防盗链失败
Content-Disposition 文件名丢失

将浏览器 → WebSocket → Rust → 下载引擎协议升级为结构化 DownloadRequest。

至少包含：

url
final_url
filename
method
referrer
user_agent
cookies
headers
tab_url
content_type
content_length

只传输下载真正需要的安全请求头。

优先支持：

Cookie
Referer
Origin
User-Agent
Authorization
Accept
Accept-Language

过滤 hop-by-hop headers：

Host
Connection
Content-Length
Transfer-Encoding
Sec-* 等浏览器内部头

扩展截获下载后必须执行：

Browser Download
→ 获取请求上下文
→ 向 ProxyDM 发送完整 DownloadRequest
→ 等待桌面端 ACK
→ ACK 成功后 cancel 浏览器下载
→ ACK 失败则保留浏览器原下载

绝不能因为 ProxyDM 未启动导致下载直接丢失。

WebSocket 协议增加：

protocol_version
request_id
action

ACK 返回：

request_id
accepted
reason

避免当前只要收到任意 WS message 就认为成功。

同时兼容旧版纯 URL 协议。

---

# P0：下载引擎请求头完整支持

EngineConfig 增加 headers。

Probe 和真实下载必须使用相同请求上下文。

当前 execute_download 中存在：

headers = HashMap::new()

这种行为需要消除。

probe：

HEAD
Range GET
fallback GET

以及实际分段请求全部共享：

Cookie
Referer
Authorization
User-Agent
其他必要 Header

重定向后应正确继承允许继承的 header。

为这些情况增加测试：

Cookie protected download
Referer protected download
Authorization protected download
redirect download
Content-Disposition filename
Range + authentication

---

# P0：暂停 / 恢复 / 崩溃恢复可靠性

对现有 ProgressLedger、resume state、`.pdm` 临时文件和 ChunkQueue 做完整审计。

目标：

任意下载阶段关闭程序，然后重新启动，都能恢复到尽可能接近中断位置。

覆盖：

单连接
多连接
未知 Content-Length
Range server
服务器忽略 Range
下载到 99% 时退出
正在 retry/backoff 时退出
正在暂停时退出
任务排队时退出

应用启动时：

Downloading 状态的历史任务必须进行 crash recovery。

不要让旧状态永久显示 Downloading。

建议状态：

Queued
Connecting
Downloading
Paused
Retrying
Merging
Completed
Failed

Failed 保存：

error_code
error_message
http_status
retry_count
last_error_at

---

# P0：修复连接数语义

当前 UI 提供：

Auto
4
8
16
32
64

但 Rust backend 多处存在 `.min(32)`。

统一整个系统。

定义：

0 = Auto
1-64 = 用户指定

如果最终决定最大连接数为 32，则 UI 也必须限制 32。

本项目目标建议支持最大 64。

Auto 根据文件大小、服务器 Range 能力和当前吞吐动态决定。

同时避免小文件创建大量连接。

参考策略：

< 2 MiB：1
2-16 MiB：2-4
16-128 MiB：4-8
128 MiB-1 GiB：8-16

> 1 GiB：16+

这只是初始策略。

实际实现应允许 dynamic segmentation。

---

# P1：Dynamic Segmentation

目前固定 chunk 分配继续增强为动态任务池。

目标：

快连接完成自己的 segment 后可以继续领取剩余区间。

慢连接不能拖住整个下载。

要求：

ChunkQueue 支持动态拆分
worker stealing
剩余大 chunk 自动二分
小 chunk 避免无限细分
记录每连接瞬时吞吐
极慢 worker 可以停止当前 segment 并重新分配

下载最后阶段尽量保证所有连接持续工作。

避免：

31 个连接结束以后只剩 1 个慢连接下载最后几十 MB。

增加 engine benchmark / integration test。

---

# P1：运行时修改连接数

Neat Download Manager 支持下载过程中改变连接数量。

Properties / Details 页面增加：

Connections

允许：

1 → 8
8 → 16
16 → 4

无需暂停下载。

WorkerPool / ConcurrentDownloader 支持动态扩缩 worker。

减少 worker 时：

等待当前小 segment 完成或者安全回收任务。

增加 worker 时：

立即从 ChunkQueue 获取任务。

修改值需要持久化。

---

# P1：运行时限速

支持：

Global bandwidth limit
Per-download bandwidth limit

都可以在下载过程中即时修改。

单位：

Unlimited
KB/s
MB/s

MultiLimiter 改造成 runtime-updatable limiter。

不要重新启动任务。

优先实现 token bucket 或等效算法。

UI：

Toolbar 可以快速设置全局限速。

Download Properties 可以设置单任务限速。

---

# P1：Retry / Failover 重构

Retry 应根据错误类型分类。

Retryable：

timeout
connection reset
temporary DNS error
HTTP 408
HTTP 429
HTTP 500
502
503
504
partial stream failure

默认不自动重试：

400
401
403
404
405

401 / 403 应提示可能需要：

Cookie
Authorization
Referer
刷新下载地址

指数退避增加 jitter：

1s
2s
4s
8s
16s
30s max

如果任务配置多个代理：

Proxy A failure
→ retry
→ Proxy B
→ Proxy C
→ Direct（根据配置决定）

保存每个代理近期：

success count
failure count
latency
last failure

避免持续选中明显不可用代理。

---

# P1：代理系统完善

ProxyConfig 增加：

name
protocol
host
port
username
password

支持：

HTTP
HTTPS CONNECT
SOCKS5

代理认证至少支持 Reqwest 能力允许的 Basic authentication。

密码不要进入普通日志。

UI 增加：

Test
Latency
Authentication
Status

下载任务可以选择：

Direct
System Default
指定 Proxy
Proxy Group

保留 ProxyDownloadManager 自身“每任务独立代理”的特色。

---

# P1：新建下载体验

重新设计 New Download。

URL 输入后立即异步 probe。

显示：

文件名
文件大小
Content-Type
服务器
是否支持断点续传
最终 URL
建议连接数

保存路径使用系统目录选择器。

不要要求用户手动输入路径字符串。

主操作：

Download

次操作：

Download Later / Add Paused

下载来源来自浏览器扩展时：

自动填充 URL
文件名
Header
Cookie
Referer
建议保存目录

不要把 Cookie / Authorization 明文显示在主界面。

---

# P1：任务列表 UX

主界面优先展示：

Name
Size
Progress
Speed
ETA
Status

次级信息：

Connections
Proxy

右键：

Pause
Resume
Restart
Open
Open Folder
Copy URL
Refresh URL
Properties
Delete

双击：

Completed → Open file
Downloading → Details

支持：

搜索
状态过滤
文件类型过滤

分类至少：

All
Downloading
Completed
Incomplete

保持界面简洁。

---

# P1：真实 Speed / ETA

确保速度来自固定时间窗口，而非单次事件差值。

建议 rolling window：

3-5 秒

ETA：

remaining_bytes / smoothed_speed

避免速度显示：

0 → 100MB/s → 0 → 80MB/s

进度事件可以继续 500ms 左右推送。

前端避免每个 progress event 导致整张 DownloadTable 重渲染。

做 React Profiler / render count 检查。

每个 row 只响应自己的状态变化。

---

# P2：Refresh / Renew Download URL

实现“更新过期链接继续下载”。

对 Paused / Failed 下载提供：

Refresh URL

用户粘贴新的 URL。

系统重新 probe：

Content-Length
ETag
Last-Modified
Content-Disposition

确认新资源与旧资源兼容以后继续现有 `.pdm`。

如果资源不匹配：

明确警告
禁止静默拼接不同文件

浏览器扩展后续可以提供：

捕获相同文件的新 URL
→ 更新已有任务。

---

# P2：Media Capture

浏览器扩展增加 Media 页面。

使用 webRequest 等浏览器 API 检测：

video/*
audio/*
application/vnd.apple.mpegurl
application/x-mpegURL
.m3u8
.ts

记录：

URL
Content-Type
Size
Tab
Referer
Cookie/Header context

扩展 popup 显示当前 Tab 捕获的媒体资源。

允许：

Download with ProxyDM

过滤：

blob:
data:
明显重复的 segment

建立 dedup key。

---

# P2：HLS

实现基础 HLS：

master.m3u8
media playlist
.ts segments

流程：

解析 master
→ 用户选择 variant
→ 下载 playlist
→ 并发下载 segment
→ 保持顺序
→ 合并为单文件

支持：

AES-128 HLS 可以后续实现。

DRM 内容保持 unsupported，并明确提示。

首版输出 `.ts` 即可。

后续再考虑 ffmpeg remux。

不要把完整视频下载能力绑定到 yt-dlp。

---

# P2：认证

增加 DownloadRequest authentication abstraction。

支持：

HTTP Basic

可以研究：

Digest
NTLM

优先保证：

浏览器现有 Authorization 透传。

认证信息：

不得出现在普通日志
不得出现在 toast
不得直接展示在 Properties

增加日志脱敏：

Authorization
Cookie
Proxy-Authorization

统一输出：

<redacted>

---

# P2：资源占用

目标是日常驻留没有明显负担。

检查：

WebSocket reconnect loop
500ms progress events
React DownloadTable rerender
SQLite writes
resume state writes
日志刷盘
大量下载任务时 worker 数量
Network Client Pool

优化：

progress DB persistence 做 throttle

例如 UI 500ms 更新一次，
持久化 2-5 秒一次或状态切换立即保存。

避免每个网络 chunk 写 SQLite。

---

# P2：浏览器扩展体验

Toolbar popup 至少显示：

ProxyDM Connected / Disconnected
Capture Downloads On / Off
当前页面捕获 Media 数量

提供快捷跳过方式。

例如：

按住 Alt / Delete 后点击下载
→ 本次使用浏览器原生下载

支持设置：

Minimum file size
Ignored domains
Ignored extensions
Intercept file types

避免用户下载 favicon、小图片、小 JSON 时全部弹 ProxyDM。

---

# P2：文件冲突

现在 unique_filename 会自动：

file.zip
file.1.zip
file.2.zip

增加用户策略：

Ask
Rename
Overwrite
Skip

默认建议 Rename。

如果相同 URL / 相同目标文件已经有正在下载的任务：

提示 duplicate。

---

# 工程质量要求

整个改造拆成小 PR / 小 commit。

推荐顺序：

Phase A
Browser request context + structured WS protocol

Phase B
Crash recovery + resume reliability

Phase C
Dynamic segmentation + runtime connection control

Phase D
Runtime bandwidth control + retry/failover

Phase E
Proxy authentication

Phase F
UX cleanup

Phase G
Media capture + HLS

每阶段独立可运行。

不要一次做一个巨大 PR。

每一阶段都增加测试。

必须保持旧配置兼容。

数据库 schema 变化必须 migration。

配置新增字段全部 serde(default)。

---

# 必须补充的测试

Rust unit/integration：

Range server
Non-Range server
server ignores Range
disconnect midway
pause/resume
crash recovery
retry exhaustion
403
429
500
redirect
Cookie protected
Referer protected
Basic Auth
Proxy failover
dynamic workers
rate limiter
unknown Content-Length
duplicate file

Frontend：

download status transitions
progress
speed smoothing
ETA
new download probe
runtime settings editing

Browser extension：

ACK success
ACK failure fallback
desktop offline
header extraction
temporary bypass
duplicate interception

---

# 当前已知需要优先检查的问题

1. UI 可以选择 64 connections，但 backend 多处限制到 32。

2. browsers-extension/shared/background.js 当前 `sendReliable()` 主要发送 URL，浏览器请求上下文没有完整传给桌面应用。

3. PendingDownloadRequest 当前字段过少，无法表达 headers / cookies / referer。

4. execute_download probe 当前创建空 headers。

5. ProxyConfig 当前只有 protocol / host / port，缺少认证。

6. PropertiesDialog 当前基本属于只读信息页，无法实时调整 connections / bandwidth。

7. 浏览器扩展当前主要依赖 chrome.downloads.onCreated，没有形成完整媒体嗅探能力。

8. Refresh expired URL / renew download 需要独立实现。

9. README 中写“最高 64 线程”，需要与实际 engine 行为完全一致。

先验证这些判断，再实施，发现代码已经解决某项时跳过，不要重复改造。

---

# Definition of Done

完成上述 Phase A-F 后，ProxyDownloadManager 应满足：

普通公开文件：
浏览器点击下载 → ProxyDM 接管 → 正确文件名 → 多连接下载 → 正确完成。

登录态文件：
浏览器点击下载 → Cookie / Referer 等上下文成功透传 → 可以正常下载。

弱网络：
断网 / timeout / reset → 自动 retry → 可以继续。

退出应用：
重新启动 → 未完成任务可恢复。

下载过程中：
可以修改 Connections 和 Speed Limit。

服务器不支持 Range：
自动降级单连接，文件保持正确。

Proxy：
HTTP / SOCKS5 工作正常，认证可用。

失败：
用户可以明确看到原因和可执行操作。

性能：
大量进度事件不会导致整个 UI 高频重渲染。

所有：

cargo check
cargo test
npx tsc --noEmit
pnpm test

必须通过。

同时更新 CHANGELOG.md。

先输出一份 `docs/neat-parity-plan.md`，内容包括：

现状
Gap Matrix
涉及文件
数据结构变化
数据库 migration
Phase A-G
风险
测试方案

然后按 Phase 顺序实施。

每完成一个 Phase：

运行测试
列出修改文件
说明新增测试
说明仍存在的 gap

遵循 AGENTS.md 的 commit / push 授权规则，不自行 bump version，不自行 release。
# Phase UI：Primer React → shadcn/ui 全面迁移

在前述 Neat Download Manager parity 工作之外，前端 UI 技术栈同时进行一次系统性迁移：

当前：

React 19
TypeScript
Vite
Primer React
Primer Octicons
大量 inline style

目标：

React 19
TypeScript
Vite
shadcn/ui
Tailwind CSS v4
Lucide React
CSS Variables Theme

最终移除：

@primer/react
@primer/primitives
@primer/octicons-react

整个应用统一使用 shadcn/ui 组件体系。

这次迁移需要同步改善 UI/UX，避免机械地把 Primer Component 替换成 shadcn Component。

目标风格：

* 桌面应用
* 高信息密度
* 简洁
* 克制
* 现代
* 适合长期驻留
* 接近 Neat Download Manager / Transmission / qBittorrent 这类工具的操作效率
* 保留 ProxyDownloadManager 自己的视觉识别

避免：

* 大面积巨型 Card
* Dashboard 风格
* SaaS 后台风格
* 过大的圆角
* 过多阴影
* 过多留白
* 手机 App 风格
* 每一块内容都套 Card
* 花哨动画
* Gradient
* Glassmorphism

这是桌面下载管理器。

核心目标是：

一眼看清下载状态，
一两次点击完成常见操作。

---

# 1. 初始化 shadcn/ui

这是已有 Vite React 项目。

按照当前官方 Existing Project 方案配置：

Tailwind CSS v4
@tailwindcss/vite
shadcn/ui
lucide-react

使用：

pnpm

不要重新创建 Vite 项目。

配置：

@/* → ./src/*

增加：

components.json

全局 CSS 使用 Tailwind v4。

优先使用 shadcn 当前推荐的 CSS Variables theme。

颜色全部通过 semantic token：

background
foreground
card
card-foreground
popover
popover-foreground
primary
primary-foreground
secondary
secondary-foreground
muted
muted-foreground
accent
accent-foreground
destructive
border
input
ring

不要在业务组件里散落：

#ffffff
#000000
#d8dee4
#656d76

等硬编码颜色。

---

# 2. Theme

支持：

Light
Dark
System

默认：

System

Tauri 桌面端需要正确跟随系统主题。

应用启动时不得发生明显：

light → dark

闪烁。

ThemeProvider 放在应用根节点。

主题设置持久化。

Settings 中增加：

Appearance

选项：

System
Light
Dark

如果 Rust Settings 已经有持久化设置体系，可以将 theme 纳入 Settings。

避免存在两个互相冲突的配置源。

---

# 3. Design Token

建立统一设计 token。

推荐：

radius：
偏小

桌面工具建议：

--radius ≈ 0.4rem ~ 0.5rem

spacing：

整体紧凑。

表格行高度：

约 36-42px

Toolbar：

约 40-44px

字体：

优先系统 UI font。

macOS：

-apple-system

Windows：

Segoe UI

Linux：

system-ui

数字区域，例如：

速度
大小
ETA
进度

使用：

font-variant-numeric: tabular-nums

避免数字变化导致列宽视觉抖动。

---

# 4. 组件选择

优先引入：

Button
Input
Label
Select
Dialog
AlertDialog
DropdownMenu
ContextMenu
Tooltip
Popover
Command
Tabs
Progress
Badge
Checkbox
Switch
Separator
ScrollArea
Table
Sheet
Skeleton
Sonner

Toast 使用：

Sonner

避免继续使用：

alert()

所有：

成功
失败
warning
操作结果

统一通过 Sonner 或页面内 inline feedback。

---

# 5. Icon

Primer Octicons 全部迁移到：

lucide-react

统一：

16px
18px
20px

三档。

Toolbar 主图标建议：

18px

Table / Context Menu：

16px

禁止不同页面随意使用不同 icon size。

图标按钮必须提供：

Tooltip

以及：

aria-label

---

# 6. 主窗口 Layout 重做

建议整体结构：

┌──────────────────────────────────────────────────────────────┐
│ Toolbar                                                      │
├──────────────┬───────────────────────────────────────────────┤
│ Sidebar      │ Download Table                                │
│              │                                               │
│ All          │                                               │
│ Downloading  │                                               │
│ Completed    │                                               │
│ Incomplete   │                                               │
│              │                                               │
├──────────────┴───────────────────────────────────────────────┤
│ Status Bar                                                   │
└──────────────────────────────────────────────────────────────┘

Sidebar 保持窄：

约 170-210px。

允许未来增加：

Video
Audio
Other
Proxy Group

但当前不要过度实现。

---

# 7. Toolbar

Toolbar 保持单行。

推荐内容：

New Download
Start / Resume
Pause
Delete

Separator

Open
Open Folder

右侧：

Global Speed Limit
Search
Settings
More

图标 + Tooltip 为主。

New Download 可以保留文字。

Toolbar 操作必须根据 selection 状态 enable / disable。

例如：

没有选中任务：
Pause disabled

选中 Completed：
Open enabled
Pause disabled

---

# 8. Download Table

Download Table 是整个应用最重要的 UI。

优先保证性能和信息密度。

推荐列：

Name
Size
Progress
Speed
ETA
Status

可选列：

Connections
Proxy

Name 列占最大空间。

Progress 列：

Progress Bar + percentage

例如：

████████░░ 82%

Speed：

12.4 MB/s

ETA：

01:42

Status：

Downloading
Paused
Queued
Retrying
Completed
Failed

状态颜色需要克制。

Downloading：

正常 foreground / subtle accent

Completed：

绿色可以使用 semantic success token

Failed：

destructive

Paused / Queued：

muted

不要整行大面积染色。

---

# 9. Download Table 性能

迁移 shadcn/ui 时不要因为组件封装增加额外渲染。

重点检查：

DownloadTable
columns
useDownloadEvents
React Query
progress events

要求：

单个 download progress 更新时，
尽量只刷新对应 row。

避免：

整个 table 500ms rerender。

必要时：

React.memo
useMemo
selector
拆分 DownloadRow

下载数量：

100
500
1000

都应该保持流畅。

如果普通 HTML table 在 1000 条以后明显吃力，可以评估：

@tanstack/react-table
+
@tanstack/react-virtual

但只有实际需要时再引入 virtualization。

不要为了技术复杂度提前过度设计。

---

# 10. Row Selection

支持：

single click
Ctrl/Cmd click
Shift click

键盘：

Arrow Up
Arrow Down
Enter
Delete
Space

建议：

Enter：
打开 Details

Space：
Pause / Resume

Delete：
删除

Cmd/Ctrl+A：
全选

选择状态必须清晰。

---

# 11. Context Menu

把现有自定义右键逻辑迁移到：

shadcn ContextMenu

菜单：

Start / Resume
Pause

Separator

Open
Open Folder

Separator

Copy URL
Refresh URL

Separator

Properties

Separator

Delete

根据任务状态动态隐藏或 disable。

---

# 12. New Download Dialog

重新设计成紧凑的桌面 Dialog。

结构：

URL
[________________________________________]

File
Filename
Save to

Download Info
Size
Type
Resume Support

Network
Connections
Proxy

底部：

Cancel
Add Paused
Download

URL 输入后进行异步 probe。

Probe 中时：

小型 Spinner / Skeleton。

不要阻塞整个 Dialog。

Save To：

Input
+
Folder Picker Button

不要要求用户自己手输路径。

---

# 13. Download Details

现在：

PropertiesDialog
DownloadDetailsWindow

功能存在重叠。

重新整理职责。

建议：

双击任务打开统一 Details。

布局：

Header
────────────────────
filename
status
progress
speed / ETA

Tabs：

Overview
Connections
Activity

Overview：

URL
Save Path
Size
Downloaded
Created
Resume Support
Proxy

Connections：

Progress Map
连接数
实时连接状态
每连接速度

Activity：

该任务日志
retry
redirect
proxy switch
HTTP error

避免维护两个完全不同的 UI 实现。

共享：

DownloadDetailsContent

Dialog 与独立 Window 只负责外壳。

---

# 14. Progress Map

保留这个能力。

视觉上重新设计：

每一个分片作为细长 segment。

状态：

Pending
Downloading
Completed
Failed

支持 Tooltip：

Part
Range
Downloaded
Status

避免大量鲜艳颜色。

如果 segment 数量很多：

合并视觉 bucket。

例如：

64 / 128 / 256 个显示块上限。

避免 DOM 节点爆炸。

---

# 15. Settings

当前 SettingsDialog 内容继续重构。

建议使用：

左侧 Settings Navigation
右侧 Settings Panel

分类：

General
Downloads
Network
Proxy
Browser
Appearance
Advanced

General：

Download directory
Launch at startup
Language

Downloads：

Default connections
Concurrent downloads
Retries
Global bandwidth limit
Duplicate behavior

Network：

User-Agent
Timeout
TLS settings

Proxy：

Proxy Table
Default Proxy

Browser：

Browser integration
Download capture
Ignored domains
Minimum file size

Appearance：

Theme

Advanced：

Logs
Dangerous TLS option
Debug options

Dangerous Settings 必须明确放 Advanced。

尤其：

danger_accept_invalid_certs

当前默认值需要审计。

生产默认设置应以安全为优先。

---

# 16. Proxy Table

迁移为 shadcn Table。

每行：

Name
Protocol
Address
Latency
Status
Actions

Status：

Online
Offline
Untested

操作：

Test
Edit
Delete

新增代理：

Dialog

字段：

Name
Protocol
Host
Port

Authentication Switch

Username
Password

密码：

type=password

不要在普通列表显示。

---

# 17. Logs

Log Dialog 改为专门的日志 Viewer。

要求：

monospace

支持：

INFO
WARN
ERROR

过滤：

All
Info
Warn
Error

支持：

Search

操作：

Copy
Clear
Open Log File

日志区域使用：

ScrollArea

新日志到达：

用户在底部时自动 scroll

用户向上查看历史时停止自动 scroll。

---

# 18. Search

主界面 Search 支持：

filename
URL
status
proxy

可以使用：

Input

快捷键：

Cmd/Ctrl+F

Escape：

清除或退出 Search。

---

# 19. Empty State

没有任务时不要显示巨大营销页面。

简单显示：

No downloads yet

以及：

New Download

允许拖拽 URL / 文件链接时可以进一步增强。

---

# 20. Error UX

彻底清除：

alert()

所有后端错误转换成统一前端 Error。

建议：

AppError {
code
message
details?
action?
}

例如：

403 Forbidden

UI：

Download failed
Server returned HTTP 403.

可能的操作：

Retry
Refresh URL
Open Properties

错误信息需要对用户有操作价值。

---

# 21. Responsive

这是桌面应用。

优先优化：

900×600
1100×700
1440×900
4K

小窗口：

Sidebar 可以 collapse。

表格隐藏低优先级列：

Proxy
Connections
ETA

不要把桌面端做成手机 responsive layout。

---

# 22. Tauri Native Feel

窗口拖动区域、title bar、Dialog、快捷键要考虑桌面环境。

支持：

macOS
Windows
Linux

避免依赖浏览器 hover 才能完成关键操作。

所有核心操作支持鼠标 + 键盘。

---

# 23. Accessibility

shadcn/Radix 原生能力不要破坏。

必须保留：

focus ring
keyboard navigation
aria-label
Dialog focus trap
Escape close

不要为了视觉效果添加：

outline: none

除非提供等价 focus-visible 状态。

---

# 24. CSS 清理

迁移完成后审计：

src/App.css
src/utils/styles.ts
各组件 inline style

目标：

删除绝大多数 inline style。

优先：

Tailwind utility
CSS Variables
少量真正需要的 CSS

不要把原来的 inline style 简单复制成：

style={{ ... }}

迁移必须达到代码层面的统一。

---

# 25. 删除 Primer

当所有页面迁移完成后：

pnpm remove 
@primer/react 
@primer/primitives 
@primer/octicons-react

然后检查：

rg "@primer/" src

结果必须为：

0

检查：

rg "style=\{\{" src

只允许极少数真正动态计算的 style。

例如：

动态宽度
动态 progress
动态坐标

普通 layout 不允许继续大量 inline style。

---

# 26. Migration Strategy

不要一次把整个 UI 拆烂。

按照：

UI-1 Foundation
UI-2 Shell
UI-3 Download Table
UI-4 Dialogs
UI-5 Settings
UI-6 Details
UI-7 Cleanup

执行。

## UI-1 Foundation

完成：

Tailwind v4
shadcn/ui
components.json
@ alias
ThemeProvider
Sonner
Lucide

确认旧 UI 仍然运行。

## UI-2 Shell

迁移：

App
Layout
Toolbar
Sidebar
StatusBar

## UI-3 Download Table

迁移：

DownloadTable
columns
ContextMenu
Selection
Progress
Badge

同时进行 render performance audit。

## UI-4 Dialogs

迁移：

NewDownload
Delete
About
Update
Extension
Log

## UI-5 Settings

迁移：

SettingsDialog
ProxyTable

## UI-6 Details

迁移：

PropertiesDialog
DownloadDetailsWindow
ProgressMap

共享核心 Details component。

## UI-7 Cleanup

删除 Primer。

删除：

unused CSS
unused styles
unused imports
dead components

统一 design token。

---

# 27. 与 Neat parity Plan 的执行关系

UI migration 与下载引擎改造不要混成一个超大提交。

推荐执行顺序：

1. UI-1 Foundation

2. Phase A
   Browser Request Context

3. Phase B
   Resume Reliability

4. UI-2
   Application Shell

5. UI-3
   Download Table

6. Phase C
   Dynamic Segmentation

7. Phase D
   Runtime Control

8. UI-4
   Dialogs

9. UI-5
   Settings

10. UI-6
    Details

11. Phase E
    Proxy Authentication

12. Phase F
    UX Cleanup

13. UI-7
    Primer Removal

这样 UI 可以逐步承接新增 backend capability。

---

# 28. 禁止事项

不要：

重新创建整个前端项目。

不要：

删除现有业务逻辑然后重写。

不要：

更换 React Query / Zustand，除非发现明确问题。

不要：

因为 shadcn/ui 再引入一个大型 UI framework。

不要：

同时存在长期的 Primer + shadcn 双组件体系。

双体系只允许存在于迁移阶段。

不要：

复制 shadcn 官网 Dashboard 示例然后套到下载器。

不要：

引入几十个没有实际使用的 shadcn components。

按需 add。

不要：

为了好看牺牲信息密度。

不要：

大量 animation。

---

# 29. 验收标准

UI 完成以后：

* `@primer/*` dependency 为 0
* shadcn/ui 成为唯一主要 UI component system
* Tailwind CSS v4 配置正确
* Light / Dark / System 正常
* 主下载列表适合高频桌面操作
* New Download 操作路径明显缩短
* Settings 分类清晰
* Download Details 信息完整
* Context Menu 完整
* Toast 统一
* alert() 为 0
* 核心 UI 支持键盘操作
* 100+ 下载任务下操作流畅
* Progress 更新没有全表无意义 rerender
* 没有明显 layout shift
* macOS / Windows / Linux 样式均可接受
* 原有功能全部保留
* 中英文 i18n 保留

执行：

pnpm build
pnpm test
npx tsc --noEmit

以及：

cd src-tauri
cargo check
cargo test

全部通过。

最后输出：

1. Primer → shadcn Component Mapping
2. 修改文件列表
3. 删除 dependency
4. 新增 dependency
5. UI architecture
6. performance changes
7. screenshots / UI descriptions
8. remaining gaps

并更新：

CHANGELOG.md

不要自行 bump version。
不要自行 release。
遵循 AGENTS.md 的 commit / push 授权规则。

