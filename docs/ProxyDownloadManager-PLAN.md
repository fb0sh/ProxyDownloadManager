# ProxyDownloadManager Implementation Plan

> 目标：只处理 **性能、稳定性、下载正确性、交互缺陷、UI/UX 精简**。  
> 明确排除：TLS、WebSocket 鉴权、凭据加密、Origin 校验、CSP 等安全类改造。  
> 原则：优先修复会造成错误结果、状态错乱、重复下载、文件损坏、资源浪费的问题；UI 保持小巧、克制、低学习成本。

---

## 0. 实施原则

Agent 实现时遵守以下约束：

1. 不进行大规模架构重写。
2. 不引入新的复杂状态管理方案。
3. 不改变现有 Tauri + React + TanStack Query + shadcn/ui 技术路线。
4. 优先复用现有 `WorkerPool`、`ProgressLedger`、`DownloadEngine`、`tauriClient`、现有事件系统。
5. 每个阶段独立完成、独立测试、独立提交。
6. 修复行为优先于新增功能。
7. UI 保持紧凑：
   - 主窗口不增加侧边栏。
   - 不增加复杂 Dashboard。
   - 不增加多层导航。
   - 不增加大面积卡片。
   - 不增加不必要动画。
8. 所有新 UI 必须同时兼容中英文。
9. 所有异步操作必须有明确的 loading / disabled / error 状态。
10. 所有下载完成路径必须满足“文件确实完整并 finalize 成功”后再进入 Completed。

---

# Phase 1 — 下载完整性与核心 Bug

优先级：P0

## 1.1 SingleDownloader 完成前校验文件长度

文件：

```text
src-tauri/src/engine/single.rs
```

当前问题：

顺序下载遇到服务器提前结束响应时，`stream.next()` 返回 `None` 后会直接 finalize 文件并发送 `DownloadCompleted`。

修改：

- 下载开始时记录预期总长度：
  - 优先使用 `cfg.total_size`
  - 若 `cfg.total_size == 0`，使用响应 `Content-Length`
- stream 正常结束后：
  - 若存在已知总长度，则验证 `written == expected`
  - 小于 expected 时返回 `PdmError::Incomplete`
- `Incomplete` 状态必须保留 `.pdm`
- 不发送 `DownloadCompleted`
- 保存当前 resume progress

完成条件：

```text
服务器声明 100 MB
实际只返回 60 MB
→ 状态 Failed / Incomplete
→ .pdm 保留
→ 不生成最终文件
```

测试：

- Content-Length 与真实 body 相等
- body 提前结束
- unknown Content-Length
- pause 后恢复

---

## 1.2 ConcurrentDownloader 校验 Content-Range

文件：

```text
src-tauri/src/engine/task_download.rs
```

每个 Range 请求收到 HTTP 206 后必须验证：

```text
requested:
bytes=start-end

response:
Content-Range: bytes actual_start-actual_end/total
```

要求：

```text
actual_start == requested start
actual_end >= actual_start
actual_end <= requested end
total == cfg.total_size（已知时）
```

建议新增一个纯函数：

```rust
validate_content_range(...)
```

并单独测试。

遇到异常响应时：

```text
→ TaskResult::RangeNotSupported
```

或新增更准确的：

```text
TaskResult::InvalidRangeResponse
```

然后统一触发 concurrent → single fallback。

不要继续向目标 offset 写数据。

测试：

- 正常 206
- start 错误
- end 超范围
- total 不一致
- 缺少 Content-Range
- HTTP 200 ignore Range

---

## 1.3 修复动态 worker 与初始 worker 行为不一致

文件：

```text
src-tauri/src/engine/concurrent.rs
```

当前有两套 worker loop：

```text
初始 worker
动态扩容 worker
```

两边对：

```text
Fatal
FatalNoRetry
RangeNotSupported
Partial
Cancelled
```

处理不完全一致。

改造：

提取统一函数：

```rust
async fn run_worker(...)
```

或：

```rust
async fn worker_loop(...)
```

初始 worker 与动态新增 worker全部调用同一份实现。

统一规则：

```text
Complete
→ 正常继续

Partial
→ remaining 回队列

Cancelled
→ 当前 task 必须保证可恢复

RangeNotSupported
→ 当前 task 回队列
→ 设置统一 abort_reason
→ stop

FatalNoRetry
→ 当前 task 回队列
→ 设置统一 abort_reason
→ stop

Fatal
→ retry
→ retries exhausted 后 task 回队列
→ 设置 abort_reason
→ stop
```

必须保证：

```text
任何 worker 已经 pop 的 task
在失败退出时不会从恢复快照消失
```

测试：

- 初始 worker 403
- 动态 worker 403
- 初始 worker RangeLost
- 动态 worker RangeLost
- retries exhausted
- runtime 增加 connections 后失败

---

## 1.4 修复同名并发下载的临时文件冲突

涉及：

```text
src-tauri/src/download_manager.rs
src-tauri/src/engine/file_io.rs
```

当前：

```text
foo.zip
foo.zip.pdm
```

两个同时创建的同名任务存在竞争。

改成下载 ID 独立临时文件。

推荐：

```text
<home_dir>/temp/<download-id>.pdm
```

或：

```text
<save_dir>/.proxydm/<download-id>.pdm
```

优先推荐：

```text
<home_dir>/temp/<id>.pdm
```

原因：

- 简单
- 不污染下载目录
- 天然避免同名
- 删除与恢复按 ID 定位
- 最终 rename 时统一解决文件名冲突

需要调整：

```text
create_output_file
finalize_file
remove_download_files
resume
crash recovery
delete
```

临时文件路径应成为明确字段或统一函数：

```rust
temp_path(download_id)
```

不要在多个模块自行拼接 `.pdm`。

最终文件冲突策略只在 finalize 前处理一次。

测试：

```text
两个任务同时下载同名 foo.zip
→ 两个 temp 文件不同
→ 不会相互写入
```

---

## 1.5 修复 Resume 完成边界 rename 错误被忽略

文件：

```text
src-tauri/src/download_manager.rs
```

现有 resume 特殊路径存在：

```rust
let _ = std::fs::rename(...)
self.ledger.on_completed(id)
```

修改：

- 删除直接 `std::fs::rename`
- 统一调用现有 finalize 流程
- finalize 失败：
  - 保持 Paused / Failed
  - 保留临时文件
  - 返回错误
- finalize 成功后才能：
  - `ledger.on_completed`
  - emit `DownloadCompleted`

---

# Phase 2 — 下载状态机一致性

优先级：P0 / P1

## 2.1 SingleDownloader 真正支持续传

目前 SingleDownloader 虽然会保存 progress，但重新开始时仍使用普通 GET + create/truncate。

期望：

若：

```text
cfg.is_resume == true
cfg.downloaded > 0
```

且服务器支持 Range：

```http
Range: bytes=<downloaded>-
```

否则：

```text
从 0 重下
```

如果 Single 模式本身来自“不支持 Range”的资源，则 UI 中明确：

```text
暂停后继续 = 重新开始
```

不要让用户误以为一定是字节级续传。

实现重点：

- append / seek 到已有 offset
- 校验 206
- 下载量从 `cfg.downloaded` 起算
- 完成后验证最终大小

---

## 2.2 统一 Completed 判定

给整个项目定义唯一原则：

```text
Completed =
1. 所有预期字节已写入
2. 文件 flush/sync 成功
3. 临时文件 finalize 成功
4. ledger 更新成功
```

所有引擎：

```text
Concurrent
Single
HLS
Resume-completion shortcut
```

均遵守。

禁止任何路径先：

```text
on_completed()
```

再 finalize。

---

## 2.3 WorkerPool 状态统一

检查：

```text
Queued
Connecting
Downloading
Retrying
Paused
Completed
Failed
```

保证：

- queued task 取消后状态不会遗留 Queued
- pause 仅在 worker 真正停止后进入 Paused
- resume admission queued 时状态正确进入 Queued
- worker 启动时统一进入 Connecting/Downloading
- retry 时显示 Retrying
- error 事件不会被旧 worker 覆盖新 worker 状态

补充状态机测试。

---

# Phase 3 — HLS 重构

优先级：P1

目标：让 HLS 使用和普通下载相同的生命周期，避免成为特殊旁路。

## 3.1 HLS 纳入 WorkerPool

当前 `spawn_hls()` 直接 `tauri::async_runtime::spawn`。

修改方向：

新增：

```text
HlsDownloader
```

实现现有 `DownloadEngine` 或 WorkerPool 可管理的统一任务接口。

HLS 必须支持：

```text
pause/cancel
delete
global rate limit
task rate limit
worker slot
统一事件
```

不要求第一版实现跨重启断点续传。

---

## 3.2 HLS 改为流式落盘

禁止：

```rust
Vec<Vec<u8>>
```

持有全部 segment。

推荐：

```text
并发下载 N 个 segment
→ 每个 segment 写临时 part
→ 按顺序 merge 到主 .pdm
→ 删除 part
```

目录：

```text
temp/hls-<id>/
  000001.part
  000002.part
```

或者使用有限有序 buffer：

```text
最大只缓存 2 × connections 个 segment
```

优先选择临时 part 文件方案，简单可靠。

---

## 3.3 HLS Master / Media playlist 检查统一

Master playlist：

```text
master
→ variant
→ media playlist
```

media playlist 重新检查：

```text
segments empty
encrypted
drm
```

每层 parser 输出统一检查。

---

## 3.4 HLS 进度接入 UI

当前 callback 基本没有真正接入 ledger。

需要：

```text
downloaded segments / total segments
```

映射到统一 progress。

列表显示：

```text
45%
```

详情可以显示：

```text
18 / 40 segments
```

保持简洁。

---

# Phase 4 — 性能优化

优先级：P1

## 4.1 减少过度日志

下载热路径当前日志较多。

以下内容降低到 debug：

```text
每个 chunk
每次 worker pop
每个 Range
频繁进度日志
```

Info 只保留：

```text
任务开始
暂停
恢复
重试耗尽
引擎 fallback
完成
错误
```

release 默认不要输出 chunk 级日志。

---

## 4.2 Logger 减少同步 flush

当前每条日志：

```text
write_all
flush
```

调整为：

```text
BufWriter
```

或：

```text
后台 channel logger
```

优先使用低改动方案：

```rust
BufWriter<File>
```

并在：

```text
错误
应用退出
显式 flush
```

时刷新。

不要引入大型 tracing 基础设施。

---

## 4.3 Progress Event 合并

当前约 500ms 推一次。

保留 500ms UI 更新即可，但同一 download：

```text
无新增进度
```

时无需发事件。

reporter 保存：

```text
last_bytes
last_parts_hash / snapshot
```

完全相同时跳过一次。

减少：

```text
Tauri event
React Query cache patch
React render
```

---

## 4.4 避免 Probe 重复请求

新建下载窗口 URL 输入过程中 debounce 后 probe。

优化：

- debounce 调整为 500~600ms
- URL 未变化不重复 probe
- 请求参数完全一致时复用最近一次结果
- 当前 probe 未完成时，新 probe 令旧结果失效

不需要复杂 cache。

---

## 4.5 新建下载 Probe 竞态修复

文件：

```text
src/NewDownloadWindow.tsx
```

使用：

```ts
const probeSeq = useRef(0)
```

每次 probe：

```ts
const seq = ++probeSeq.current
const info = await probe(...)
if (seq !== probeSeq.current) return
```

保证旧 URL 的 probe 永远不能覆盖新 URL。

---

## 4.6 Auto Connections 状态修正

当前：

```text
Auto = 0
probe 后 setConnections(suggested)
```

Auto 会失去 Auto 状态。

改：

```ts
connectionMode: "auto" | "manual"
manualConnections: number
suggestedConnections: number
```

UI：

```text
Auto (16)
1
4
8
16
32
64
```

选择 Auto 时传：

```text
connections = 0
```

后端继续自己决定。

---

# Phase 5 — 主窗口 UI 精简

优先级：P1

设计原则：

```text
小
快
一眼看懂
操作少
常用操作直接可见
低频操作收起
```

保持当前单窗口表格结构。

不要新增左侧导航。

---

## 5.1 Toolbar 精简

当前工具栏按钮过多。

调整为：

```text
[+ 新建] [继续/暂停] [删除]      [限速]        [⚙] [···]
```

其中：

```text
⚙ = Settings

··· =
  浏览器扩展
  日志
  关于
  退出
```

Redownload 不放顶栏。

放到：

```text
右键菜单
详情窗口
```

主界面高度保持现有紧凑风格。

---

## 5.2 筛选栏精简

保留：

```text
全部
下载中
已完成
未完成
```

样式继续用紧凑 segmented buttons。

右侧：

```text
[搜索................] [筛选]
```

类型筛选：

```text
全部
压缩包
视频
音频
文档
其他
```

收进一个 Filter 下拉菜单。

减少常驻控件。

---

## 5.3 Download Table 默认列

建议：

```text
☐
文件名
大小
状态 / 进度
速度
剩余
代理
```

Threads 列默认删除。

线程数属于高级信息，放详情窗口。

状态列：

下载时：

```text
[进度条] 42%
```

特殊状态：

```text
连接中…
重试中 · 42%
合并中 · 92%
```

失败：

```text
失败 · HTTP 403
```

完整 error message 放 tooltip / 详情。

---

## 5.4 整行选择

支持：

```text
单击行 → 选中
Ctrl/Cmd + Click → 多选
Shift + Click → 范围选
```

checkbox 保留。

双击：

```text
Completed → 打开文件
其他 → 打开详情
```

---

## 5.5 Context Menu 改为 Radix

删除手写：

```text
fixed div
x/y positioning
```

使用现有 Radix Dropdown/ContextMenu。

菜单：

```text
继续 / 暂停
打开
打开所在文件夹
重新下载
复制 URL
详情
────────
删除
```

规则：

```text
打开
仅 Completed 显示

继续
仅 Paused 显示

暂停
仅 active 显示
```

---

## 5.6 修复右键 Resume 错对象

当前：

```text
右键 B
→ Resume
→ actions.onResumeSelected()
```

改成明确 row action：

```ts
onResume(id)
```

所有右键菜单动作都只作用于当前右键行。

---

## 5.7 修复 Toolbar Stop 条件不一致

目前按钮显示条件包含：

```text
downloading
connecting
retrying
```

执行时只处理：

```text
downloading
```

统一调用：

```ts
isActiveStatus(status)
```

所有 active 状态都可以 Pause。

---

# Phase 6 — 新建下载窗口 UI

保持现在独立小窗口设计。

建议尺寸继续：

```text
640 × 560
```

不继续扩大。

布局：

```text
URL
[________________________________]

文件名
[________________________________]

保存到
[________________________] [浏览]

连接数                  代理
[Auto (16) ▼]           [Direct ▼]

────────────────────────────
文件大小     1.4 GB
类型         application/zip
Range        支持
最终地址     cdn.example.com/...
────────────────────────────

                  [稍后下载] [下载]
```

## 6.1 Probe 状态

四种：

```text
等待输入
探测中
探测成功
探测失败
```

探测失败不要整块消失。

显示：

```text
无法探测文件信息
连接超时

仍可直接开始下载
```

---

## 6.2 URL 输入

URL 输入变化：

```text
debounce 500~600ms
```

Enter：

```text
立即 probe / 开始下载
```

保持用户操作简单。

---

## 6.3 Proxy 下拉

显示：

```text
Direct
Proxy A
Proxy B
```

不要增加 Proxy Group 等复杂 UI。

代理配置继续统一在 Settings 管理。

---

# Phase 7 — 下载详情窗口精简

当前详情窗口信息较合理，只做整理。

建议顺序：

```text
filename
status + percent
progress

速度     ETA
大小     已下载
代理     连接数

[暂停/继续] [打开] [文件夹]

连接数
[Auto / 4 / 8 / ...]

限速
[不限速 / 256K / 1M / 5M / ...]

Progress Map

URL
[复制]
```

Refresh URL 只在：

```text
Failed
Paused
```

时显示。

避免正常下载时长期占空间。

错误状态显示：

```text
错误
HTTP 403
Forbidden
```

不要直接在主表格展示超长错误。

---

# Phase 8 — Settings 精简

不要做复杂多页配置中心。

保留单个 Settings Dialog。

用三个小 section：

```text
下载
网络
应用
```

## 下载

```text
下载目录
最大连接数
最大重试
文件冲突策略
```

## 网络

```text
默认代理
代理列表
全局限速
User-Agent
```

代理列表保持当前表格。

## 应用

```text
开机启动
静默启动
语言
全局快捷键
```

高级参数尽量不增加。

---

# Phase 9 — 文档与 CI 修复

优先级：P2

## 9.1 README 与实现同步

修复：

```text
src/stores/ 旧目录
Windows MSI 描述与实际构建不一致
代理自动切换等当前未实现的描述
过时项目结构
```

README 只描述已经稳定存在的能力。

---

## 9.2 ProxyDM-design.md

该文件历史内容过多。

处理：

```text
ProxyDM-design.md
→ docs/archive/legacy-design.md
```

顶部写：

```text
Archived historical design.
Do not use as implementation reference.
```

新增：

```text
docs/architecture.md
```

只描述当前真实代码：

```text
Tauri
React
WorkerPool
DownloadManager
ProgressLedger
ConcurrentDownloader
SingleDownloader
HlsDownloader
NetworkPool
```

保持文档短。

---

## 9.3 恢复 CI Check

新增：

```text
.github/workflows/check.yml
```

运行：

```bash
pnpm install --frozen-lockfile
npx tsc --noEmit
pnpm test

cd src-tauri
cargo fmt --check
cargo check
cargo test
```

暂时不要强制 clippy `-D warnings`，避免第一次接入制造大量无关工作。

Release workflow：

```text
needs check
```

发布前必须通过测试。

---

# Phase 10 — 测试清单

所有修改完成后至少覆盖以下场景。

## HTTP 下载

```text
普通单线程
普通多线程
Range
服务器忽略 Range
错误 Content-Range
服务器提前断流
chunk 中途断流
HTTP 403
HTTP 404
HTTP 429
HTTP 503
timeout
```

## 文件

```text
同名并发下载
Rename
Overwrite
Skip
暂停
恢复
暂停时退出程序
下载完成边界暂停
finalize 失败
删除进行中的任务
删除暂停任务
```

## Worker

```text
动态增加连接数
动态降低连接数
worker failure
retry exhaustion
queue
cancel queued task
```

## HLS

```text
media playlist
master playlist
segment failure
pause
delete
大文件低内存
```

## UI

```text
右键 Resume 只作用当前行
Connecting 可以暂停
Retrying 可以暂停
Completed 才显示 Open
Auto connections 始终保持 Auto
快速切换 URL 不出现旧 probe 覆盖
多选操作正确
```

---

# Git 提交建议

按阶段提交：

```text
fix(engine): enforce download integrity checks

refactor(engine): unify concurrent worker loop

fix(storage): isolate partial files by download id

fix(resume): finalize completed partial downloads safely

refactor(hls): run hls downloads through worker lifecycle

perf(engine): reduce progress and logging overhead

fix(ui): correct row actions and active-state controls

refactor(ui): simplify download manager toolbar and filters

refactor(ui): streamline new-download and details windows

docs: align documentation with current architecture

ci: restore pull request validation workflow
```

---

# 最终验收标准

Agent 完成后必须满足：

```text
1. 不完整文件不会显示 Completed
2. Range 返回错误内容时不会写入错误 offset
3. 两个同名任务不会共用临时文件
4. pause/resume/finalize 不会产生假 Completed
5. HLS 可以被暂停、删除，并受到 worker 生命周期管理
6. HLS 大文件不会全部驻留内存
7. 动态 worker 与初始 worker 使用同一错误处理逻辑
8. 主窗口工具栏明显更精简
9. 右键操作始终作用于当前行
10. Auto connections 不会被 probe 悄悄改成固定值
11. 快速输入 URL 不会出现旧 probe 覆盖
12. README / architecture / CI 与当前代码一致
```

优先完成 Phase 1 → Phase 4，再处理 UI。UI 只做精简和纠错，保持 ProxyDownloadManager 当前的小型桌面下载器定位。
