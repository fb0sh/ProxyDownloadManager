# Proxy Download Manager — Store listing (English, en-US)

> Paste into Partner Center → **Microsoft Store listing → English (United States) → Details**.
> **Extension name** and **Short description** come from the package manifest (`_locales/en/messages.json`)
> and are read-only in Partner Center.

## Extension name

Proxy Download Manager

## Short description (from manifest, read-only)

Send downloads to the ProxyDM desktop app, with per-task proxy selection and multi-threaded acceleration.

## Description

Proxy Download Manager (ProxyDM) is a browser extension that hands your downloads to the free,
open-source ProxyDM desktop app instead of letting the browser handle them. ProxyDM is a
multi-proxy download manager: every download task can use its own HTTP or SOCKS5 proxy, with
automatic failover across a list of candidate proxies, up to 64 parallel connections per file,
and resumable transfers.

**Why install it**

- **Route downloads through a proxy.** Pick a proxy per task, or keep a pool of candidates and let
  ProxyDM move to the next one when a route fails.
- **Faster large downloads.** Multi-threaded, size-adaptive transfers with retry and User-Agent
  rotation on failure.
- **Keep the browser out of the way.** This extension catches a download before the browser starts
  it, and hands over the URL, cookies, referrer and User-Agent so the desktop app makes the same
  request you clicked.

**What the extension does**

- **Captures downloads.** When ProxyDM is running, downloads you start are sent to the app and
  opened in its New Download window, where you can confirm the folder, thread count and proxy
  before starting.
- **Right-click actions.** "Download with ProxyDM" on a link, on the page itself, or on a selection
  of links.
- **Media on this tab.** The popup lists media files found on the current page (video, audio, and
  common archive or installer types) so you can send any of them straight to ProxyDM.
- **Skip next download.** Arm a one-shot bypass, or hold Alt / Delete while clicking, when you want
  the browser to handle a single download itself.
- **Filters.** Set a minimum file size and ignore lists for domains and file extensions, so small or
  unwanted files keep using the browser.
- **Connection status.** The toolbar badge and popup show whether ProxyDM is connected, and a
  notification explains a download that had to fall back to the browser because the app was not
  running.
- **English and 简体中文 popup.**

**Requires the desktop app**

This extension is a companion, not a standalone downloader. Without the ProxyDM desktop app
running, it stays idle and reports "Disconnected": the badge dims and downloads continue to use the
browser normally. The app is free and open source (MIT):
https://github.com/fb0sh/ProxyDownloadManager/releases

**Privacy**

The extension talks only to the desktop app on your own machine (`ws://127.0.0.1:18999`). Nothing is
sent to the developer or to any third party. It reads the cookies and response headers for the URL
you are downloading solely so the desktop app can reproduce the same request.
Privacy policy: https://fb0sh.github.io/ProxyDownloadManager/privacy.html

## Search terms

`download manager`, `proxy download`, `socks5`, `multi-threaded download`, `download accelerator`,
`resumable download`, `video downloader`
