# Edge 隐私页声明（逐字段对位）

> 对应 Partner Center → 扩展条目 → **隐私** 页的五个区块。
> 审核人员以英文为工作语言，所以每段先给英文（直接粘贴），再给中文备查。
> **最后三项（数据使用、证明、隐私政策）是你签署的声明，请自行过目后再提交。**

---

## 1. 单一用途 · Single purpose

**EN**

> ProxyDM hands browser downloads to the user's own locally installed ProxyDM desktop app, so that
> the file can be fetched through a proxy the user configured, with multi-threaded and resumable
> transfers. That is the extension's only purpose. It does not modify page content, does not inject
> advertising, and does not talk to any server operated by the developer — its only network peer is
> the desktop app on 127.0.0.1:18999.

**中文**

> 把浏览器下载交给用户自己安装在本机的 ProxyDM 桌面端，以便用用户配置的代理、以多线程和断点续传
> 的方式抓取文件。这是扩展唯一的目的。它不修改页面内容、不注入广告，也不与开发者运营的任何服务器
> 通信——唯一的网络对端是本机 127.0.0.1:18999 上的桌面端。

---

## 2. 权限理由 · Permission justifications

### `downloads`

**EN**

> Needed to detect that a download started (`chrome.downloads.onCreated` — the only event that
> carries the download's final URL, filename and MIME type), and, once the desktop app has accepted
> the transfer, to call `chrome.downloads.cancel` and `chrome.downloads.erase` so the file is not
> fetched twice. Without it the extension could neither notice the download nor stop the browser's
> own copy of it.

**中文**

> 用于感知下载开始（`chrome.downloads.onCreated` 是唯一携带最终 URL、文件名和 MIME 类型的事件），
> 并在桌面端接受任务后调用 `chrome.downloads.cancel` / `chrome.downloads.erase`，避免同一个文件被
> 下载两次。没有它，扩展既不知道下载发生，也无法停掉浏览器自己那一份。

### `webRequest`

**EN**

> Used read-only, through `chrome.webRequest.onHeadersReceived`, to read the `Content-Type` and
> `Content-Length` of responses. This lets the extension tell a real downloadable file apart from a
> streaming response, report the correct size, and build the media list shown in the popup. The
> listener never returns a blocking response, never redirects, and never modifies a request or a
> response.

**中文**

> 以只读方式使用 `chrome.webRequest.onHeadersReceived` 读取响应的 `Content-Type` 与
> `Content-Length`，用于区分可下载文件与流式响应、显示正确体积，并生成 popup 里的媒体列表。
> 该监听器不返回阻断响应、不做重定向、不修改任何请求或响应。

### `cookies`

**EN**

> Many downloads need the session the user already has in the browser (a file behind a login, for
> example). For the single URL being handed over, the extension reads that URL's cookies with
> `chrome.cookies.getAll({ url })` and includes them in the header set sent to the local desktop app,
> so the app's request matches the one the browser would have made. Cookies are never stored, logged
> or sent anywhere else: the receiving app binds to 127.0.0.1 and redacts cookie and authorization
> values in its own logs.

**中文**

> 很多下载需要用户已有的浏览器会话（例如登录后才能访问的文件）。对正在交接的那一个 URL，扩展用
> `chrome.cookies.getAll({ url })` 读取它的 Cookie，并放进发给本机桌面端的请求头里，使桌面端的
> 请求与浏览器原本的请求一致。Cookie 不会被存储、记录或发送到其他任何地方：接收端应用只监听
> 127.0.0.1，并且在自己的日志里把 Cookie 与 Authorization 的值抹成 `<redacted>`。

### `host_permissions: <all_urls>`

**EN**

> The user may download a file from any site, and the extension cannot know in advance which ones.
> `<all_urls>` is required for three things: the content script that detects media elements on the
> page the user is viewing, the read-only `webRequest` filter that observes response headers, and
> reading cookies for the download URL. The extension adds no UI of its own to pages, does not modify
> page content, and observes nothing beyond what those three uses require.

**中文**

> 用户可能从任何站点下载文件，扩展无法预先知道是哪些站点。`<all_urls>` 用于三件事：检测当前页面
> 媒体元素的内容脚本、只读观察响应头的 `webRequest` 过滤器，以及读取下载 URL 的 Cookie。
> 扩展不会往页面里添加自己的界面、不修改页面内容，也不会观察这三项用途之外的任何东西。

### `tabs`

**EN**

> The popup lists the media found on the tab the user is currently looking at, so the extension needs
> the active tab's id and URL (`chrome.tabs.query`), needs to message that tab's content script
> (`chrome.tabs.sendMessage`), and needs to drop per-tab state when the tab closes
> (`chrome.tabs.onRemoved`).

**中文**

> popup 要列出用户当前所在标签页里发现的媒体，因此需要活动标签页的 id 与 URL（`chrome.tabs.query`）、
> 需要给该标签页的内容脚本发消息（`chrome.tabs.sendMessage`），也需要在标签页关闭时清掉它的状态
> （`chrome.tabs.onRemoved`）。

### `storage`

**EN**

> Stores the user's own settings locally: capture on/off, the one-shot skip flag, minimum file size,
> ignored domains and ignored extensions (`chrome.storage.local`), plus the per-tab media list for
> the current session (`chrome.storage.session`). Nothing in storage leaves the device.

**中文**

> 在本机保存用户自己的设置：拦截开关、一次性跳过标记、最小文件体积、忽略的域名与扩展名
> （`chrome.storage.local`），以及当前会话内按标签页的媒体列表（`chrome.storage.session`）。
> 存储里的任何内容都不会离开本机。

### `contextMenus`

**EN**

> Adds the three "Download with ProxyDM" items (link, page, selected links) to the browser's
> right-click menu, which is the primary way to send a specific file to the desktop app.

**中文**

> 在浏览器右键菜单里添加三条「Download with ProxyDM」（链接 / 页面 / 选中的链接），这是把指定文件
> 交给桌面端的主要入口。

### `notifications`

**EN**

> Shows one notification when a download had to fall back to the browser because the desktop app was
> not running, so the user knows why the file was not handed over instead of assuming the extension
> is broken.

**中文**

> 当桌面端没运行、这次下载只能回退给浏览器时，弹一条通知说明原因，免得用户以为扩展坏了。

---

## 3. 远程代码 · Remote code

**选择：No — 我没有使用远程代码**

**EN**

> No remote code. Every file is contained in the submitted package: plain JavaScript files, no
> bundler-time fetch, no `eval`, no remotely hosted script, no WASM. The extension's only network
> traffic is a WebSocket to 127.0.0.1:18999 that carries JSON request data, never executable code.

**中文**

> 没有远程代码。所有文件都包含在提交的包内：纯 JavaScript，没有构建期拉取、没有 `eval`、没有远程
> 托管的脚本、没有 WASM。扩展唯一的网络流量是发往 127.0.0.1:18999 的 WebSocket，传输的是 JSON 请求
> 数据，从不传输可执行代码。

---

## 4. 数据使用 · Data usage

**⭐ 这一段是你要拍板的。** 下面是建议与依据，勾选动作和"我证明以下披露是真实的"都必须由你完成。

扩展**实际接触**的数据（代码里可验证）：

| 数据 | 来源 | 去向 |
| --- | --- | --- |
| Cookie（`Cookie` 头） | `chrome.cookies.getAll({ url })`，仅限正在下载的那个 URL | 发往 127.0.0.1:18999 |
| 身份验证信息（`Authorization` 头） | `webRequest` 响应头白名单 | 发往 127.0.0.1:18999 |
| 被下载的 URL、文件名、来源标签页地址 | `downloads.onCreated`、`tabs.query` | 发往 127.0.0.1:18999 |
| 页面里检测到的媒体元素 | 内容脚本读取 DOM | 本机，仅用于生成 popup 列表 |
| 用户自己的设置（体积阈值、忽略名单） | popup 输入 | 本机 `storage.local` |

关键事实：`protocol.js` 里的 `filterHeaders()` 是**白名单**，只放行 8 个请求头
（cookie / referer / origin / user-agent / authorization / accept / accept-language /
accept-encoding），其余一律丢弃；桌面端只监听 `127.0.0.1`，并在日志里抹掉 Cookie 与 Authorization 的值。
**没有任何数据被发送到开发者或第三方。**

**建议勾法（保守，且与隐私政策一致）**

- ☑ **身份验证信息** —— 确实读取了 Cookie 与 Authorization 头（虽然只交给本机 app）
- ☑ **网页浏览记录** —— 确实接触被下载的 URL 与来源标签页地址
- ☑ **网站内容** —— 内容脚本确实读取页面里的媒体元素

**另一种做法**：一项都不勾，主张"扩展不收集数据"。风险是审核看到 Cookie / Authorization 读取后
判定披露不完整 —— 官方文档明确写了"不完整、误导性或不准确的披露"可能被视为违反开发者策略，导致额外
审查或直接拒绝。

两种做法都**不能**与隐私政策内容冲突。当前隐私政策是按"只在本机处理、不外传、开发者不接收"写的，
上面两种选择都能自洽。

☐ 我证明以下披露是真实的 —— **你来勾**

---

## 5. 隐私政策 · Privacy policy

URL：

```
https://fb0sh.github.io/ProxyDownloadManager/privacy.html
```

（旧版 UI 在"属性"页里会问「隐私策略要求」：选 **是**，然后填同一个 URL。）
