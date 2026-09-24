# Certification testing notes

> Paste into Partner Center → **Submit your extension → Certification notes**.
> Reviewers read English; the Chinese summary at the end is for your own reference.

---

## What this extension is

Proxy Download Manager is a **companion extension** for the free, open-source ProxyDM desktop
application. It takes over downloads that start in the browser and hands them to that desktop app so
the file can be fetched through a proxy the user configured, with multi-threaded and resumable
transfers.

Source code for both the extension and the desktop app (so testers can verify every claim below):
https://github.com/fb0sh/ProxyDownloadManager

## Important: the desktop app is required for the main test

The extension is deliberately inert when the desktop app is not running. **This is expected
behaviour, not a failure.**

- Toolbar icon switches to the dimmed "off" artwork and the title reads "ProxyDM enabled — desktop
  offline".
- Clicking the icon shows the popup with **Disconnected** and no media list.
- Downloads proceed in the browser exactly as they normally would — nothing is cancelled or
  redirected.
- If a download is triggered while the app is unreachable, one notification appears:
  "ProxyDM is not running — using the browser download instead."

## Installing the desktop app (Windows)

1. Open https://github.com/fb0sh/ProxyDownloadManager/releases and download the latest Windows
   installer (NSIS `.exe`).
2. **The installer is not code-signed yet**, so SmartScreen shows "Windows protected your PC" —
   choose **More info → Run anyway**. This is a signing gap, not a detection.
3. Launch ProxyDM. It opens a local WebSocket listener on **127.0.0.1:18999** while running. Nothing
   is exposed beyond the loopback interface.
4. Keep it running for the tests below.

No account, licence key or test credentials are needed — the extension and the app communicate
locally only.

## Test steps

1. **Connect** — click the ProxyDM toolbar icon. The popup header shows **Connected** with a dark
   dot. (Nothing to log into.)
2. **Capture a download** — visit any page with a downloadable file and start the download. The
   browser download is cancelled and the desktop app opens its **New Download** window, pre-filled
   with the URL, file name and size. Confirm to start the transfer.
3. **Right-click** — right-click a link and choose **Download with ProxyDM**; the same window opens
   for that link. The same item is available on a page background and on a multi-link selection.
4. **Media on this tab** — open a page containing a video or audio element. The popup lists what it
   found under "Media on this tab"; clicking an entry sends it to the app.
5. **Skip next download** — press the bypass button in the popup (or hold Alt / Delete while
   clicking a download) and confirm that the browser handles that one download itself.
6. **Filters** — set a minimum size or add a domain to the ignore list; a download below the
   threshold or from an ignored domain stays in the browser.
7. **Offline behaviour** — quit ProxyDM and repeat step 2. The download must complete normally in
   the browser, and the notification described above appears.

## Technical notes for the reviewer

- **Single purpose.** Send browser downloads to the user's own locally installed desktop app. The
  extension adds no UI to pages, injects no advertising, and has no server of its own.
- **Network.** The only network peer is the desktop app on `ws://127.0.0.1:18999`, carrying JSON
  request metadata. There is no analytics, no telemetry and no third-party endpoint. The extension
  works with the machine offline.
- **Permissions.** Each of `downloads`, `webRequest`, `cookies`, `tabs`, `storage`, `contextMenus`
  and `notifications` is used for the features in the test steps above; the detailed justifications
  are in the Privacy page of this submission.
- **`webRequest` is read-only.** The listener reads `Content-Type` / `Content-Length` from
  `onHeadersReceived` and never returns a blocking response, never redirects, and never modifies a
  request.
- **Header handling.** Only a fixed allow-list of eight request headers is forwarded to the local app
  (cookie, referer, origin, user-agent, authorization, accept, accept-language, accept-encoding);
  everything else is dropped. The receiving app redacts cookie and authorization values in its logs
  (`src-tauri/src/headers.rs`).
- **No remote code.** All code is contained in the package: plain JavaScript files, no `eval`, no
  remotely hosted script, no WASM.

## Support contact

https://github.com/fb0sh/ProxyDownloadManager/issues

---

## 中文摘要（备查，不必粘贴）

这是一个配套扩展，主功能依赖本机安装的开源桌面端 ProxyDM。

- **桌面端没运行时扩展故意保持静默**：角标变暗、popup 显示「未连接」、下载照常走浏览器，并弹一条
  说明通知。这是预期行为，不是故障。
- Windows 安装包目前**未签名**，SmartScreen 会拦一次（更多信息 → 仍要运行）。
- 不需要任何账号或测试凭据，全部本地通信。
- 唯一的网络对端是 `ws://127.0.0.1:18999`；无统计、无遥测、无第三方端点。
- `webRequest` 只读，只用来看 `Content-Type` / `Content-Length`，不阻断不修改。
- 只向本机 app 转发 8 个白名单请求头，桌面端日志里会抹掉 Cookie 与 Authorization 的值。
- 无远程代码。
