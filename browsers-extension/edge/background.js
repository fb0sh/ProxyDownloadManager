// =============================================================================
// background.js — ProxyDM Browser Extension
//
// Intercept downloads with full request context, ACK-gated cancel, media
// sniffing, and a toolbar popup for connection / capture / skip-once.
// Source of truth: shared/. Run build.sh to sync browser folders.
// =============================================================================

import {
  buildDownloadRequest,
  parseAck,
  filterHeaders,
  mediaDedupKey,
  shouldSkipMediaUrl,
} from "./protocol.js";

const WS_URL = "ws://127.0.0.1:18999";
let ws = null;
let reconnectTimer = null;
let lastNotRunningNotificationAt = 0;
let connected = false;

const startedAt = Date.now();
const STARTUP_GRACE_MS = 10000;
const NOT_RUNNING_NOTIFICATION_COOLDOWN_MS = 15000;

const STORAGE_KEY = "proxydm_enabled";
const SETTINGS_KEY = "proxydm_settings";
const BYPASS_NEXT_KEY = "proxydm_bypass_next";

const defaultSettings = {
  minSize: 0,
  ignoredDomains: "",
  ignoredExtensions: "ico,svg,json,woff,woff2,ttf,map",
  interceptTypes: "",
};

const mediaByTab = new Map();
const requestCtx = new Map();

async function isEnabled() {
  const r = await chrome.storage.local.get(STORAGE_KEY);
  return r[STORAGE_KEY] !== false;
}

async function getSettings() {
  const r = await chrome.storage.local.get(SETTINGS_KEY);
  return { ...defaultSettings, ...(r[SETTINGS_KEY] || {}) };
}

async function setEnabled(enabled) {
  await chrome.storage.local.set({ [STORAGE_KEY]: enabled });
  updateIcon(enabled);
  broadcastStatus();
  if (enabled) {
    createContextMenus();
    connect();
  } else {
    destroyContextMenus();
    disconnect();
  }
}

function updateIcon(enabled) {
  const suffix = enabled ? "" : "_off";
  chrome.action.setIcon({
    path: {
      16: `icons/icon16${suffix}.png`,
      48: `icons/icon48${suffix}.png`,
      128: `icons/icon128${suffix}.png`,
    },
  });
  const title = connected
    ? "ProxyDM connected"
    : enabled
      ? "ProxyDM enabled — desktop offline"
      : "ProxyDM disabled";
  chrome.action.setTitle({ title });
  if (!enabled) {
    chrome.action.setBadgeText({ text: "✕" });
    chrome.action.setBadgeBackgroundColor({ color: "#cf222e" });
  } else if (!connected) {
    chrome.action.setBadgeText({ text: "!" });
    chrome.action.setBadgeBackgroundColor({ color: "#bf8700" });
  } else {
    chrome.action.setBadgeText({ text: "" });
  }
}

function connect() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  let socket;
  try {
    socket = new WebSocket(WS_URL);
  } catch {
    scheduleReconnect();
    return;
  }
  ws = socket;
  socket.onopen = () => {
    if (ws !== socket) return;
    connected = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    isEnabled().then(updateIcon);
    broadcastStatus();
  };
  socket.onclose = () => {
    if (ws !== socket) return;
    connected = false;
    ws = null;
    isEnabled().then(updateIcon);
    broadcastStatus();
    scheduleReconnect();
  };
  socket.onerror = () => {
    if (ws !== socket) return;
    connected = false;
    ws = null;
    scheduleReconnect();
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, 3000);
}

function disconnect() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  if (ws) {
    ws.close();
    ws = null;
  }
  connected = false;
}

function sendReliable(payload) {
  const body = typeof payload === "string" ? payload : JSON.stringify(payload);
  const requestId = payload && payload.request_id;
  return new Promise((resolve) => {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    let socket = null;
    let done = false;
    const finish = (ok) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
        try {
          socket.close();
        } catch {
          /* closing a one-shot socket */
        }
      }
      resolve(ok);
    };
    const timer = setTimeout(() => finish(false), 3000);
    try {
      socket = new WebSocket(WS_URL);
      socket.onopen = () => {
        try {
          socket.send(body);
        } catch {
          finish(false);
        }
      };
      socket.onmessage = (evt) => {
        const ack = parseAck(evt.data);
        if (requestId && ack.requestId && ack.requestId !== requestId) return;
        finish(ack.accepted === true);
      };
      socket.onclose = () => {
        if (!done) finish(false);
      };
      socket.onerror = () => {
        if (!done) finish(false);
      };
    } catch {
      finish(false);
    }
  });
}

function createContextMenus() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "dl-link", title: "Download with ProxyDM", contexts: ["link", "video", "audio"] });
    chrome.contextMenus.create({ id: "dl-page", title: "Download page with ProxyDM", contexts: ["page"] });
    chrome.contextMenus.create({ id: "dl-sel", title: "Download selected link with ProxyDM", contexts: ["selection"] });
  });
}

function destroyContextMenus() {
  chrome.contextMenus.removeAll();
}

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  let url = null;
  switch (info.menuItemId) {
    case "dl-link":
      url = info.linkUrl || info.srcUrl;
      break;
    case "dl-page":
      url = tab?.url;
      break;
    case "dl-sel":
      url = extractUrl(info.selectionText);
      break;
  }
  if (!url) return;
  const req = await buildFromTab(url, tab);
  if (!(await sendReliable(req))) notifyNotRunning();
});

chrome.downloads.onCreated.addListener(async (item) => {
  if (!(await isEnabled())) return;
  const downloadUrl = getDownloadUrl(item);
  if (!downloadUrl || downloadUrl.startsWith("blob:")) return;
  if (isRestoredDownloadEvent(item)) return;

  const bypass = await consumeBypass(item);
  if (bypass) return;

  const settings = await getSettings();
  if (shouldIgnore(downloadUrl, item, settings)) return;

  const tab = await activeTab();
  const req = await buildFromDownload(item, tab);
  const ok = await sendReliable(req);
  if (ok) {
    chrome.downloads.cancel(item.id, () => {
      chrome.downloads.erase({ id: item.id });
    });
  } else {
    notifyNotRunning({ allowStartupGrace: true });
  }
});

async function consumeBypass(item) {
  const stored = await chrome.storage.session?.get?.(BYPASS_NEXT_KEY).catch(() => ({}));
  if (stored && stored[BYPASS_NEXT_KEY]) {
    await chrome.storage.session.set({ [BYPASS_NEXT_KEY]: false });
    return true;
  }
  const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
  const tab = tabs[0];
  if (!tab?.id) return false;
  try {
    const resp = await chrome.tabs.sendMessage(tab.id, { action: "proxydm-bypass-state" });
    if (resp && resp.bypass) return true;
  } catch {
    /* no content script on this page */
  }
  if (item.danger === "accepted") return false;
  return false;
}

function shouldIgnore(url, item, settings) {
  try {
    const u = new URL(url);
    const ignored = String(settings.ignoredDomains || "")
      .split(/[,\s]+/)
      .filter(Boolean);
    if (ignored.some((d) => u.hostname === d || u.hostname.endsWith("." + d))) return true;
    const ext = (item.filename || u.pathname).split(".").pop()?.toLowerCase() || "";
    const ignoredExt = String(settings.ignoredExtensions || "")
      .split(/[,\s]+/)
      .filter(Boolean)
      .map((s) => s.replace(/^\./, "").toLowerCase());
    if (ext && ignoredExt.includes(ext)) return true;
    const minSize = Number(settings.minSize || 0);
    if (minSize > 0 && item.fileSize > 0 && item.fileSize < minSize) return true;
  } catch {
    return false;
  }
  return false;
}

async function activeTab() {
  const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
  return tabs[0];
}

async function buildFromTab(url, tab) {
  const cookies = await cookiesFor(url);
  return buildDownloadRequest({
    url,
    filename: filenameFromUrl(url),
    referrer: tab?.url || "",
    tabUrl: tab?.url || "",
    cookies,
    userAgent: navigator.userAgent,
    headers: { Cookie: cookies, Referer: tab?.url || "", "User-Agent": navigator.userAgent },
  });
}

async function buildFromDownload(item, tab) {
  const url = getDownloadUrl(item);
  const cookies = await cookiesFor(url);
  const ctx = requestCtx.get(url) || {};
  const headers = filterHeaders({
    ...(ctx.headers || {}),
    Cookie: cookies,
    Referer: item.referrer || tab?.url || ctx.referrer || "",
    "User-Agent": navigator.userAgent,
  });
  return buildDownloadRequest({
    url: item.url,
    finalUrl: item.finalUrl || item.url,
    filename: (item.filename || "").split(/[/\\]/).pop() || filenameFromUrl(url),
    method: item.method || "GET",
    referrer: item.referrer || tab?.url || "",
    tabUrl: tab?.url || "",
    cookies,
    userAgent: navigator.userAgent,
    headers,
    contentType: item.mime || ctx.contentType || "",
    contentLength: item.fileSize > 0 ? item.fileSize : ctx.contentLength || 0,
  });
}

async function cookiesFor(url) {
  if (!chrome.cookies?.getAll) return "";
  try {
    const list = await chrome.cookies.getAll({ url });
    return list.map((c) => `${c.name}=${c.value}`).join("; ");
  } catch {
    return "";
  }
}

function filenameFromUrl(url) {
  try {
    const u = new URL(url);
    const last = u.pathname.split("/").filter(Boolean).pop() || "";
    return decodeURIComponent(last);
  } catch {
    return "";
  }
}

if (chrome.webRequest?.onHeadersReceived) {
  chrome.webRequest.onHeadersReceived.addListener(
    (details) => {
      const headers = {};
      for (const h of details.responseHeaders || []) {
        if (h.name && h.value) headers[h.name] = h.value;
      }
      const type = (headers["content-type"] || headers["Content-Type"] || "").split(";")[0].trim();
      const length = Number(headers["content-length"] || headers["Content-Length"] || 0);
      requestCtx.set(details.url, {
        headers: filterHeaders(headers),
        contentType: type,
        contentLength: length,
        referrer: details.initiator || "",
      });
      if (shouldSkipMediaUrl(details.url)) return;
      const isMedia =
        type.startsWith("video/") ||
        type.startsWith("audio/") ||
        type === "application/vnd.apple.mpegurl" ||
        type === "application/x-mpegURL" ||
        /\.m3u8(\?|$)/i.test(details.url);
      if (!isMedia) return;
      const tabId = details.tabId;
      if (tabId < 0) return;
      const list = mediaByTab.get(tabId) || [];
      const key = mediaDedupKey(details.url, type);
      if (list.some((m) => m.key === key)) return;
      list.unshift({
        key,
        url: details.url,
        contentType: type,
        size: length,
        tabId,
        referrer: details.initiator || "",
        capturedAt: Date.now(),
      });
      mediaByTab.set(tabId, list.slice(0, 50));
      broadcastStatus();
    },
    { urls: ["<all_urls>"] },
    ["responseHeaders", "extraHeaders"]
  );
}

chrome.tabs?.onRemoved?.addListener((tabId) => {
  mediaByTab.delete(tabId);
});

chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.action === "sendUrl") {
    (async () => {
      const req = await buildFromTab(request.url, sender.tab);
      const ok = await sendReliable(req);
      sendResponse({ ok });
    })();
    return true;
  }
  if (request.action === "popup-status") {
    (async () => {
      const enabled = await isEnabled();
      const tab = await activeTab();
      const media = mediaByTab.get(tab?.id ?? -1) || [];
      const settings = await getSettings();
      sendResponse({ enabled, connected, mediaCount: media.length, media, settings });
    })();
    return true;
  }
  if (request.action === "set-enabled") {
    setEnabled(!!request.enabled).then(() => sendResponse({ ok: true }));
    return true;
  }
  if (request.action === "save-settings") {
    chrome.storage.local.set({ [SETTINGS_KEY]: request.settings }).then(() => sendResponse({ ok: true }));
    return true;
  }
  if (request.action === "bypass-next") {
    chrome.storage.session?.set?.({ [BYPASS_NEXT_KEY]: true });
    sendResponse({ ok: true });
    return true;
  }
  if (request.action === "download-media") {
    (async () => {
      const tab = await activeTab();
      const req = await buildFromTab(request.url, tab);
      req.content_type = request.contentType || req.content_type;
      const ok = await sendReliable(req);
      sendResponse({ ok });
    })();
    return true;
  }
});

function broadcastStatus() {
  chrome.runtime.sendMessage({ action: "status-changed" }).catch(() => {});
}

chrome.runtime.onInstalled.addListener(async () => {
  const on = await isEnabled();
  updateIcon(on);
  if (on) {
    createContextMenus();
    connect();
  }
});

chrome.runtime.onStartup.addListener(async () => {
  const on = await isEnabled();
  updateIcon(on);
  if (on) {
    createContextMenus();
    connect();
  }
});

function notify(title, message) {
  if (!chrome.notifications) return;
  chrome.notifications.create({
    type: "basic",
    iconUrl: "icons/icon128.png",
    title,
    message,
  });
}

function notifyNotRunning({ allowStartupGrace = false } = {}) {
  const now = Date.now();
  const inStartupGrace = allowStartupGrace && now - startedAt < STARTUP_GRACE_MS;
  const inCooldown = now - lastNotRunningNotificationAt < NOT_RUNNING_NOTIFICATION_COOLDOWN_MS;
  if (!inStartupGrace && !inCooldown) {
    notify("ProxyDM is not running", "Using the browser download instead. Start ProxyDM to capture downloads.");
    lastNotRunningNotificationAt = now;
  }
  isEnabled().then(updateIcon);
}

function getDownloadUrl(item) {
  return item.finalUrl || item.url || "";
}

function isRestoredDownloadEvent(item) {
  const startTime = Date.parse(item.startTime || "");
  return Number.isFinite(startTime) && startTime + STARTUP_GRACE_MS < startedAt;
}

function extractUrl(text) {
  if (!text) return null;
  const m = text.match(/https?:\/\/[^\s<>"']+/);
  return m ? m[0] : null;
}
