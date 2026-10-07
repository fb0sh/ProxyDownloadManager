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
  mediaHeaders,
  shouldSkipMediaUrl,
  interceptDecision,
  PROTOCOL_VERSION,
} from "./protocol.js";
import { t } from "./i18n.js";

const DEBUG = false;
const WS_URL = "ws://127.0.0.1:18999";
const CLAIM_TIMEOUT_MS = 800;
const ADD_TIMEOUT_MS = 3000;

let ws = null;
let reconnectTimer = null;
let lastNotRunningNotificationAt = 0;
let connected = false;
let downloaderVersion = "";
const pendingRequests = new Map();

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
// Request headers the browser actually sent, by requestId, waiting for the
// response that follows. This is the ground truth a sniffed download must
// replay: Origin / Referer / Accept / Authorization as the site saw them.
const sentHeaders = new Map();
// Entries are read long after the response, when the user clicks Download, so
// the map is bounded rather than cleaned per request.
const REQUEST_CTX_MAX = 500;
const bypassTabs = new Set();
const passthroughUntil = new Map();

let runtimeEnabled = true;
let runtimeSettings = { ...defaultSettings };
let bypassNext = false;
let activeTabId = null;

function debug(...args) {
  if (DEBUG) console.debug("[ProxyDM]", ...args);
}

function isEnabled() {
  return runtimeEnabled;
}

function getSettings() {
  return runtimeSettings;
}

async function setEnabled(enabled) {
  runtimeEnabled = !!enabled;
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
    ? t("actionConnected")
    : enabled
      ? t("actionOffline")
      : t("actionDisabled");
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
  if (!runtimeEnabled) return;
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
    updateIcon(runtimeEnabled);
    broadcastStatus();
  };
  socket.onmessage = (evt) => {
    if (ws !== socket) return;
    let parsed = null;
    try {
      parsed = JSON.parse(evt.data);
    } catch {
      parsed = null;
    }
    if (parsed && parsed.type === "hello") {
      downloaderVersion = String(parsed.version || "");
      broadcastStatus();
      return;
    }
    settleAck(evt.data);
  };
  socket.onclose = () => {
    if (ws !== socket) return;
    connected = false;
    downloaderVersion = "";
    ws = null;
    failPending();
    updateIcon(runtimeEnabled);
    broadcastStatus();
    scheduleReconnect();
  };
  socket.onerror = () => {
    if (ws !== socket) return;
    connected = false;
    downloaderVersion = "";
    failPending();
    ws = null;
    scheduleReconnect();
  };
}

function scheduleReconnect() {
  if (!runtimeEnabled) return;
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
    const socket = ws;
    ws = null;
    connected = false;
    downloaderVersion = "";
    failPending();
    try {
      socket.close();
    } catch {
      /* already closing */
    }
  } else {
    connected = false;
    downloaderVersion = "";
    failPending();
  }
}

function failPending() {
  const pending = [...pendingRequests.values()];
  pendingRequests.clear();
  for (const item of pending) item.finish(false);
}

function settleAck(data) {
  const ack = parseAck(data);
  if (ack.requestId && pendingRequests.has(ack.requestId)) {
    const pending = pendingRequests.get(ack.requestId);
    pendingRequests.delete(ack.requestId);
    pending.finish(ack.accepted === true);
    return;
  }
  // Older desktop builds omit request_id. Only accept that when a single
  // request is in flight, so two downloads cannot take each other's ACK.
  if (!ack.requestId && pendingRequests.size === 1) {
    const [id, pending] = pendingRequests.entries().next().value;
    pendingRequests.delete(id);
    pending.finish(ack.accepted === true);
  }
}

function sendReliable(payload, timeoutMs = ADD_TIMEOUT_MS) {
  const requestId = (payload && payload.request_id) || crypto.randomUUID();
  if (payload && typeof payload === "object") payload.request_id = requestId;
  return new Promise((resolve) => {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      resolve(false);
      return;
    }
    let finished = false;
    const finish = (ok) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      pendingRequests.delete(requestId);
      resolve(ok);
    };
    const timer = setTimeout(() => finish(false), timeoutMs);
    pendingRequests.set(requestId, { finish });
    try {
      ws.send(JSON.stringify(payload));
    } catch {
      finish(false);
    }
  });
}

function createContextMenus() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "dl-link", title: t("menuLink"), contexts: ["link", "video", "audio"] });
    chrome.contextMenus.create({ id: "dl-page", title: t("menuPage"), contexts: ["page"] });
    chrome.contextMenus.create({ id: "dl-sel", title: t("menuSel"), contexts: ["selection"] });
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

chrome.downloads.onCreated.addListener((item) => {
  const downloadUrl = getDownloadUrl(item);
  if (isPassthrough(downloadUrl) || isPassthrough(item.url) || isPassthrough(item.finalUrl)) {
    debug("passthrough", item.id);
    return;
  }
  const once = bypassNext;
  const held = activeTabId != null && bypassTabs.has(activeTabId);
  const decision = interceptDecision({
    enabled: runtimeEnabled,
    connected: !!(connected && ws && ws.readyState === WebSocket.OPEN),
    url: downloadUrl,
    filename: item.filename || "",
    fileSize: item.fileSize,
    settings: runtimeSettings,
    bypass: once || held,
    restored: isRestoredDownloadEvent(item),
  });
  if (!decision.take) {
    if (decision.reason === "bypass" && once) clearBypassNext();
    debug("skip", item.id, decision.reason);
    return;
  }
  void takeover(item, downloadUrl);
});

function clearBypassNext() {
  bypassNext = false;
  chrome.storage.session?.set?.({ [BYPASS_NEXT_KEY]: false });
}

function allowPassthrough(url) {
  if (!url) return;
  passthroughUntil.set(url, Date.now() + 15000);
}

function isPassthrough(url) {
  if (!url || !passthroughUntil.has(url)) return false;
  const until = passthroughUntil.get(url);
  passthroughUntil.delete(url);
  return Date.now() <= until;
}

async function takeover(item, downloadUrl) {
  const started = Date.now();
  const requestId = crypto.randomUUID();
  const filename = basename(item.filename) || filenameFromUrl(downloadUrl);
  debug(`intercept #${item.id}`);
  const claimed = await sendReliable(
    {
      protocol_version: PROTOCOL_VERSION,
      request_id: requestId,
      action: "claim",
      url: downloadUrl,
      filename,
    },
    CLAIM_TIMEOUT_MS,
  );
  if (!claimed) {
    debug(`claim failed #${item.id}`);
    notifyNotRunning({ allowStartupGrace: true });
    return;
  }
  debug(`claim accepted in ${Date.now() - started}ms`);

  const state = await cancelDownload(item.id);
  debug(`browser download cancelled in ${Date.now() - started}ms state=${state}`);
  if (state === "complete") {
    debug(`already complete #${item.id}; leaving the browser file`);
    return;
  }
  if (state === "in_progress") {
    debug(`cancel did not stop #${item.id}`);
    return;
  }

  const tab = await activeTab();
  const req = await buildFromDownload(item, tab, requestId);
  let sent = await sendReliable(req);
  if (!sent) sent = await sendReliable(req);
  if (!sent) {
    debug(`add failed, restoring browser download #${item.id}`);
    restoreBrowserDownload(item, downloadUrl);
    notifyNotRunning({ allowStartupGrace: true });
    return;
  }
  debug(`full request sent in ${Date.now() - started}ms`);
  chrome.downloads.erase({ id: item.id });
}

function downloadState(id) {
  return new Promise((resolve) => {
    try {
      chrome.downloads.search({ id }, (items) => {
        void chrome.runtime.lastError;
        resolve((items && items[0] && items[0].state) || "interrupted");
      });
    } catch {
      resolve("interrupted");
    }
  });
}

async function cancelDownload(id) {
  await new Promise((resolve) => {
    try {
      chrome.downloads.cancel(id, () => {
        void chrome.runtime.lastError;
        resolve();
      });
    } catch {
      resolve();
    }
  });
  let state = await downloadState(id);
  // Cancel is sometimes visible one tick later. One short re-read avoids
  // treating a successful cancel as "still downloading".
  if (state === "in_progress") {
    await new Promise((resolve) => setTimeout(resolve, 40));
    state = await downloadState(id);
  }
  return state;
}

function restoreBrowserDownload(item, url) {
  allowPassthrough(url);
  allowPassthrough(item.url);
  allowPassthrough(item.finalUrl);
  const filename = basename(item.filename);
  const start = (withName) => {
    const options = { url, conflictAction: "uniquify", saveAs: false };
    if (withName && filename) options.filename = filename;
    try {
      chrome.downloads.download(options, (downloadId) => {
        const err = chrome.runtime.lastError;
        if ((err || downloadId == null) && withName && filename) {
          start(false);
          return;
        }
        if (!err && downloadId != null) chrome.downloads.erase({ id: item.id });
      });
    } catch {
      /* cancelled entry stays so the user can retry it */
    }
  };
  start(true);
}

function basename(path) {
  if (!path) return "";
  const parts = String(path).split(/[/\\]/);
  return parts[parts.length - 1] || "";
}

async function activeTab() {
  const focused = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const usable = focused.find((t) => t.id >= 0 && t.url && !t.url.startsWith("chrome-extension:") && !t.url.startsWith("moz-extension:"));
  if (usable) return usable;
  const all = await chrome.tabs.query({ active: true });
  return all.find((t) => t.url && !String(t.url).startsWith("chrome-extension:") && !String(t.url).startsWith("moz-extension:")) || all[0];
}

async function buildFromTab(url, tab) {
  const cookies = await cookiesFor(url);
  const pageUrl = tab?.url || "";
  const headers = mediaHeaders({ url, pageUrl, cookies, userAgent: navigator.userAgent });
  return buildDownloadRequest({
    url,
    filename: filenameFromUrl(url),
    referrer: headers.Referer || pageUrl,
    tabUrl: pageUrl,
    cookies,
    userAgent: navigator.userAgent,
    headers,
  });
}

async function buildFromDownload(item, tab, requestId) {
  const url = getDownloadUrl(item);
  const cookies = await cookiesFor(url);
  const ctx = requestCtx.get(url) || requestCtx.get(item.url) || {};
  // A browser download carries the page it started from; the captured request
  // headers carry what the site's own fetch looked like.
  const pageUrl = item.referrer || ctx.pageUrl || tab?.url || "";
  const headers = mediaHeaders({
    captured: ctx.requestHeaders,
    url,
    pageUrl,
    cookies,
    userAgent: navigator.userAgent,
  });
  return buildDownloadRequest({
    requestId,
    action: "add",
    url: item.url,
    finalUrl: item.finalUrl || item.url,
    filename: (item.filename || "").split(/[/\\]/).pop() || filenameFromUrl(url),
    method: item.method || "GET",
    referrer: headers.Referer || pageUrl,
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

/** Headers the browser sent for one request, filtered to what we may replay. */
function onBeforeSendHeaders(details) {
  sentHeaders.set(details.requestId, filterHeaders(details.requestHeaders || []));
}

/** A request is over: its headers are no longer needed. */
function forgetSentHeaders(details) {
  sentHeaders.delete(details.requestId);
}

function onHeadersReceived(details) {
  const headers = {};
  for (const h of details.responseHeaders || []) {
    if (h.name && h.value) headers[h.name] = h.value;
  }
  const type = (headers["content-type"] || headers["Content-Type"] || "").split(";")[0].trim();
  const length = Number(headers["content-length"] || headers["Content-Length"] || 0);
  // `documentUrl` is the frame the request came from — the page a hotlink
  // check compares against. `initiator` only ever carries its origin.
  const pageUrl = details.documentUrl || details.initiator || "";
  const sent = sentHeaders.get(details.requestId) || {};
  requestCtx.set(details.url, {
    requestHeaders: sent,
    contentType: type,
    contentLength: length,
    pageUrl,
  });
  if (requestCtx.size > REQUEST_CTX_MAX) {
    const oldest = requestCtx.keys().next().value;
    if (oldest !== undefined) requestCtx.delete(oldest);
  }
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
    referrer: pageUrl,
    pageUrl,
    // Kept with the hit: the user may click Download long after the
    // request finished and sentHeaders was cleared.
    captured: sent,
    capturedAt: Date.now(),
  });
  mediaByTab.set(tabId, list.slice(0, 50));
  broadcastStatus();
}

if (chrome.webRequest?.onBeforeSendHeaders) {
  const filter = { urls: ["<all_urls>"] };
  try {
    chrome.webRequest.onBeforeSendHeaders.addListener(onBeforeSendHeaders, filter, [
      "requestHeaders",
      "extraHeaders",
    ]);
  } catch {
    chrome.webRequest.onBeforeSendHeaders.addListener(onBeforeSendHeaders, filter, ["requestHeaders"]);
  }
}

if (chrome.webRequest?.onHeadersReceived) {
  const filter = { urls: ["<all_urls>"] };
  try {
    chrome.webRequest.onHeadersReceived.addListener(onHeadersReceived, filter, ["responseHeaders", "extraHeaders"]);
  } catch {
    chrome.webRequest.onHeadersReceived.addListener(onHeadersReceived, filter, ["responseHeaders"]);
  }
}

if (chrome.webRequest?.onCompleted) {
  const filter = { urls: ["<all_urls>"] };
  chrome.webRequest.onCompleted.addListener(forgetSentHeaders, filter);
  chrome.webRequest.onErrorOccurred.addListener(forgetSentHeaders, filter);
}

chrome.tabs?.onRemoved?.addListener((tabId) => {
  mediaByTab.delete(tabId);
  bypassTabs.delete(tabId);
  if (activeTabId === tabId) activeTabId = null;
});

chrome.tabs?.onActivated?.addListener((info) => {
  activeTabId = info.tabId;
});

chrome.storage?.onChanged?.addListener((changes, area) => {
  if (area === "local") {
    if (changes[STORAGE_KEY]) runtimeEnabled = changes[STORAGE_KEY].newValue !== false;
    if (changes[SETTINGS_KEY]) {
      runtimeSettings = { ...defaultSettings, ...(changes[SETTINGS_KEY].newValue || {}) };
    }
  }
  if (area === "session" && changes[BYPASS_NEXT_KEY]) {
    bypassNext = !!changes[BYPASS_NEXT_KEY].newValue;
  }
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
  if (request.action === "proxydm-bypass-change") {
    const tabId = sender.tab?.id;
    if (tabId != null) {
      if (request.bypass) bypassTabs.add(tabId);
      else bypassTabs.delete(tabId);
    }
    sendResponse({ ok: true });
    return;
  }
  if (request.action === "popup-status") {
    (async () => {
      await runtimeReady;
      const enabled = isEnabled();
      const tab = await activeTab();
      let media = mediaByTab.get(tab?.id ?? -1) || [];
      if (media.length === 0) {
        for (const list of mediaByTab.values()) {
          if (list.length) { media = list; break; }
        }
      }
      const settings = getSettings();
      sendResponse({
        enabled,
        connected,
        downloaderVersion,
        mediaCount: media.length,
        media,
        settings,
      });
    })();
    return true;
  }
  if (request.action === "set-enabled") {
    setEnabled(!!request.enabled).then(() => sendResponse({ ok: true }));
    return true;
  }
  if (request.action === "save-settings") {
    runtimeSettings = { ...defaultSettings, ...(request.settings || {}) };
    chrome.storage.local.set({ [SETTINGS_KEY]: request.settings }).then(() => sendResponse({ ok: true }));
    return true;
  }
  if (request.action === "bypass-next") {
    bypassNext = true;
    chrome.storage.session?.set?.({ [BYPASS_NEXT_KEY]: true });
    sendResponse({ ok: true });
    return;
  }
  if (request.action === "download-media") {
    (async () => {
      const tab = request.tabId != null
        ? await chrome.tabs.get(request.tabId).catch(() => null)
        : await activeTab();
      const list = mediaByTab.get(tab?.id ?? request.tabId ?? -1) || [];
      const hit = list.find((m) => m.url === request.url) || {};
      const cookies = await cookiesFor(request.url);
      // The page the media played on, in this order: the frame the request
      // came from, the tab it belongs to, then whatever the caller knew.
      // `details.initiator` is an origin, never a page path, so it is last.
      const pageUrl = hit.pageUrl || tab?.url || request.tabUrl || request.referrer || "";
      const headers = mediaHeaders({
        captured: hit.captured,
        url: request.url,
        pageUrl,
        cookies,
        userAgent: navigator.userAgent,
      });
      const req = buildDownloadRequest({
        url: request.url,
        finalUrl: request.url,
        filename: filenameFromUrl(request.url),
        referrer: headers.Referer || pageUrl,
        tabUrl: tab?.url || "",
        cookies,
        userAgent: navigator.userAgent,
        headers,
        contentType: request.contentType || hit.contentType || "",
        contentLength: request.size || hit.size || 0,
      });
      const ok = await sendReliable(req);
      if (!ok) notifyNotRunning();
      sendResponse({ ok });
    })();
    return true;
  }
});

function broadcastStatus() {
  chrome.runtime.sendMessage({ action: "status-changed" }).catch(() => {});
}

async function loadRuntime() {
  try {
    const r = await chrome.storage.local.get([STORAGE_KEY, SETTINGS_KEY]);
    runtimeEnabled = r[STORAGE_KEY] !== false;
    runtimeSettings = { ...defaultSettings, ...(r[SETTINGS_KEY] || {}) };
  } catch {
    /* keep defaults until storage is readable */
  }
  try {
    const stored = await chrome.storage.session?.get?.(BYPASS_NEXT_KEY);
    bypassNext = !!(stored && stored[BYPASS_NEXT_KEY]);
  } catch {
    /* session storage is optional */
  }
  updateIcon(runtimeEnabled);
  if (runtimeEnabled) {
    createContextMenus();
    connect();
  } else {
    disconnect();
  }
  try {
    const tabs = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
    if (tabs[0]?.id != null) activeTabId = tabs[0].id;
  } catch {
    /* tabs permission missing */
  }
}

const runtimeReady = loadRuntime();

chrome.runtime.onInstalled.addListener(() => {
  void loadRuntime();
});

chrome.runtime.onStartup.addListener(() => {
  void loadRuntime();
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
    notify(t("notifyOfflineTitle"), t("notifyOfflineBody"));
    lastNotRunningNotificationAt = now;
  }
  updateIcon(runtimeEnabled);
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
