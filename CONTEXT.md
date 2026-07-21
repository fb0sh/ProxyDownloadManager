# ProxyDownloadManager — 领域语言

## 核心概念

### 下载任务 (DownloadItem)

一条 URL 的一次下载生命周期。每次下载尝试对应一个独立的 `DownloadItem.id`。

- **新建下载** — 分配新 ID，全新开始
- **暂停/恢复** — 同一次下载，同一 ID，继续进度
- **重新下载 (redownload)** — 新的一次下载尝试，分配新 ID，旧记录保持原状
- **恢复失败（gob 文件丢失）** — 仍为同一 ID，以无保存状态的方式重新下载（不回退到 redownload 分配新 ID）

### 状态机

```
Queued → Downloading → Completed
                      → Paused → Downloading (resume, 同一 ID)
                      → Failed(String)
```

### 异常情况

- **文件在磁盘但 DB 记录丢失**：直接覆盖文件，不特殊处理
- **删除下载**：先执行 `cancel_and_wait`（设 flag + 等 worker 完全停止），再删 DB 记录和文件，避免文件删除竞态

### 代理 (Proxy)

每个下载任务独立选代理，三级回退：

```
下载指定代理 → Settings.default_proxy → 直连（无代理）
```

- **代理仓库**：`Settings.proxies` — `HashMap<名称, ProxyConfig>`，每项包含协议（HTTP/SOCKS5）、host、port
- **下载引用**：`DownloadItem.proxy_name` 存储代理名称
- **引擎解析**：`resolve_proxy_url()` 将名称解析为 URL（`http://host:port` 或 `socks5://host:port`）

### 下载引擎

| 引擎 | 条件 | 支持暂停 | 降级 |
|------|------|---------|------|
| **ConcurrentDownloader** | 支持 Range (206) | ✅ | Concurrent 失败 → SessionReset → Truncate → Single 重新下载 |
| **SingleDownloader** | 不支持 Range (200) / 降级 | ❌ | — |

- Concurrent 降级到 Single 时：truncate .pdm 文件 → 发送 DownloadProgress 0（重置前端进度） → 用 Single 重新下载
- Single 目前直接写入最终路径（不统一 .pdm 临时文件策略）

### 分片与进度地图

**分片 (Part)**：
Concurrent 下载时，文件上一段连续字节区间（`start`..`end`）。对应 `DownloadPart`。分片数由切分策略决定，**通常大于连接数**（多片由较少 worker 抢领），不等于「线程数」。对同一 `DownloadItem`，分片边界在开始规划后固定，暂停/恢复只更新各分片进度，不重切、不重排。
_Avoid_: 线程格、block、sector；不要把「连接数」说成「分片数」

**进度地图 (Progress Map)**：
下载详情中按分片实时展示完成度的视图：一格对应一个固定分片，绿色自下而上按该分片已完成百分比填充。像磁盘修复图，不是按连接/线程一格。布局：固定每行 8 格，按分片 `index` 从左到右、再从上到下排列；行数随分片数变化。任意引擎形态都显示：Concurrent 为多格；Single / 不支持 Range / 降级为 Single 时为 1 格（整文件进度）；降级时地图随真实下载方式重置。Downloading 时实时更新；Paused/Queued 冻结当前各格进度；Completed 全格 100%；Failed 保留已下载进度（失败高亮为可选表现，非必须）。每格同时显示百分比数字与绿色自下而上填充。仅出现在独立下载详情窗（DownloadDetailsWindow），主窗口属性弹窗不包含。
_Avoid_: 线程矩阵、worker 格、连接热力图
