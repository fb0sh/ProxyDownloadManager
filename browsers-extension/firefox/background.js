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
  isMediaResponse,
  panelVisible,
  addHiddenHost,
  removeHiddenHost,
  shouldSkipMediaUrl,
  interceptDecision,
  parseResponseSizing,
  parseContentDispositionFilename,
  normalizeMediaUrl,
  navigationClearsMedia,
  describeMediaItem,
  originOf,
  PROTOCOL_VERSION,
} from "./protocol.js";
import { t } from "./i18n.js";

const DEBUG = false;
const WS_URL = "ws://127.0.0.1:18999";
const CLAIM_TIMEOUT_MS = 800;
const ADD_TIMEOUT_MS = 3000;
const WS_PING_MS = 20000;
const WAKE_CONNECT_MS = 450;

let ws = null;
let reconnectTimer = null;
let pingTimer = null;
let sessionSaveTimer = null;
let lastNotRunningNotificationAt = 0;
let connected = false;
let downloaderVersion = "";
const pendingRequests = new Map();
const openWaiters = new Set();

const startedAt = Date.now();
const STARTUP_GRACE_MS = 10000;
const NOT_RUNNING_NOTIFICATION_COOLDOWN_MS = 15000;

const STORAGE_KEY = "proxydm_enabled";
const SETTINGS_KEY = "proxydm_settings";
const BYPASS_NEXT_KEY = "proxydm_bypass_next";
const SESSION_MEDIA_KEY = "proxydm_session_media";
const SESSION_TITLES_KEY = "proxydm_session_titles";
// Media sniffing and the on-page panel are their own switches, deliberately
// separate from proxydm_enabled (which gates interception): a user may want
// downloads intercepted without a panel on every page, or the reverse.
const SNIFF_KEY = "proxydm_sniff";
const PANEL_KEY = "proxydm_panel";
const HIDDEN_HOSTS_KEY = "proxydm_panel_hidden_hosts";

const defaultSettings = {
  minSize: 0,
  ignoredDomains: "",
  ignoredExtensions: "ico,svg,json,woff,woff2,ttf,map",
  interceptTypes: "",
};

const mediaByTab = new Map();
const tabTitles = new Map();
const lastTabUrl = new Map();
const determinedFilenames = new Map();
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
let runtimeSniff = true;
let runtimePanel = true;
let runtimeHiddenHosts = [];
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

function settleOpenWaiters(ok) {
  for (const resolve of openWaiters) resolve(ok);
  openWaiters.clear();
}

function startPing() {
  stopPing();
  pingTimer = setInterval(() => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      try {
        ws.send(JSON.stringify({ action: "ping" }));
      } catch {
        /* closed between check and send */
      }
    }
  }, WS_PING_MS);
}

function stopPing() {
  if (pingTimer) {
    clearInterval(pingTimer);
    pingTimer = null;
  }
}

function ensureConnected(timeoutMs = WAKE_CONNECT_MS) {
  if (connected && ws && ws.readyState === WebSocket.OPEN) {
    return Promise.resolve(true);
  }
  if (!runtimeEnabled) return Promise.resolve(false);
  connect();
  if (connected && ws && ws.readyState === WebSocket.OPEN) {
    return Promise.resolve(true);
  }
  return new Promise((resolve) => {
    let done = false;
    const finish = (ok) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      openWaiters.delete(finish);
      resolve(ok);
    };
    const timer = setTimeout(() => finish(false), timeoutMs);
    openWaiters.add(finish);
  });
}

function scheduleSessionPersist() {
  if (!chrome.storage?.session?.set || sessionSaveTimer) return;
  sessionSaveTimer = setTimeout(() => {
    sessionSaveTimer = null;
    const mediaObj = {};
    let count = 0;
    for (const [tabId, list] of mediaByTab.entries()) {
      if (count >= 16) break;
      mediaObj[tabId] = list.slice(0, 30);
      count += 1;
    }
    const titleObj = {};
    for (const [tabId, title] of tabTitles.entries()) {
      titleObj[tabId] = title;
    }
    chrome.storage.session
      .set({
        [SESSION_MEDIA_KEY]: mediaObj,
        [SESSION_TITLES_KEY]: titleObj,
      })
      .catch?.(() => {});
  }, 250);
}

function connect() {
  if (!runtimeEnabled) return;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  let socket;
  try {
    socket = new WebSocket(WS_URL);
  } catch {
    settleOpenWaiters(false);
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
    startPing();
    settleOpenWaiters(true);
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
    if (parsed && parsed.type === "pong") return;
    if (parsed && parsed.type === "hello") {
      downloaderVersion = String(parsed.version || "");
      broadcastStatus();
      return;
    }
    settleAck(evt.data);
  };
  socket.onclose = () => {
    if (ws !== socket) return;
    stopPing();
    connected = false;
    downloaderVersion = "";
    ws = null;
    settleOpenWaiters(false);
    failPending();
    updateIcon(runtimeEnabled);
    broadcastStatus();
    scheduleReconnect();
  };
  socket.onerror = () => {
    if (ws !== socket) return;
    stopPing();
    connected = false;
    downloaderVersion = "";
    settleOpenWaiters(false);
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
  stopPing();
  settleOpenWaiters(false);
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

async function extractSelectedUrlsFromTab(tab, fallbackText) {
  if (tab?.id != null) {
    const reply = await new Promise((resolve) => {
      try {
        chrome.tabs.sendMessage(tab.id, { action: "proxydm-extract-selection" }, (res) => {
          void chrome.runtime.lastError;
          resolve(res || null);
        });
      } catch {
        resolve(null);
      }
    });
    if (Array.isArray(reply?.urls) && reply.urls.length > 0) {
      return [...new Set(reply.urls.filter((u) => /^https?:/i.test(u)))].slice(0, 20);
    }
  }
  const single = extractUrl(fallbackText);
  return single ? [single] : [];
}

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  await runtimeReady;
  if (info.menuItemId === "dl-sel") {
    const urls = await extractSelectedUrlsFromTab(tab, info.selectionText);
    if (!urls.length) return;
    await ensureConnected();
    let anyFailed = false;
    for (const url of urls) {
      const req = await buildFromTab(url, tab);
      if (!(await sendReliable(req))) anyFailed = true;
    }
    if (anyFailed) notifyNotRunning();
    return;
  }
  let url = null;
  switch (info.menuItemId) {
    case "dl-link":
      url = info.linkUrl || info.srcUrl;
      break;
    case "dl-page":
      url = tab?.url;
      break;
  }
  if (!url) return;
  await ensureConnected();
  const req = await buildFromTab(url, tab);
  if (!(await sendReliable(req))) notifyNotRunning();
});

chrome.downloads?.onDeterminingFilename?.addListener?.((item, suggest) => {
  if (item?.id != null && item.filename) {
    determinedFilenames.set(item.id, basename(item.filename));
    if (determinedFilenames.size > 200) {
      const oldest = determinedFilenames.keys().next().value;
      if (oldest !== undefined) determinedFilenames.delete(oldest);
    }
  }
  if (typeof suggest === "function") suggest();
});

function pauseBrowserDownload(id) {
  return new Promise((resolve) => {
    try {
      chrome.downloads.pause(id, () => {
        void chrome.runtime.lastError;
        resolve();
      });
    } catch {
      resolve();
    }
  });
}

function resumeBrowserDownload(id) {
  try {
    chrome.downloads.resume(id, () => {
      void chrome.runtime.lastError;
    });
  } catch {
    /* already completed or cancelled */
  }
}

chrome.downloads.onCreated.addListener((item) => {
  const downloadUrl = getDownloadUrl(item);
  if (isPassthrough(downloadUrl) || isPassthrough(item.url) || isPassthrough(item.finalUrl)) {
    debug("passthrough", item.id);
    return;
  }
  const once = bypassNext;
  const held = activeTabId != null && bypassTabs.has(activeTabId);
  const ctx = requestCtx.get(downloadUrl) || requestCtx.get(item.url) || null;
  const resolvedFilename =
    basename(item.filename) ||
    determinedFilenames.get(item.id) ||
    ctx?.filename ||
    filenameFromUrl(downloadUrl);
  const resolvedSize = item.fileSize > 0 ? item.fileSize : ctx?.totalSize || 0;
  const isHls =
    /\.m3u8(\?|$)/i.test(downloadUrl) ||
    String(ctx?.contentType || "").toLowerCase().includes("mpegurl");
  // Non-Range chunked dynamic streams (`Content-Length` absent/0 AND `Accept-Ranges != bytes`)
  // gain 0% from multi-threading and risk burning one-time export tokens on re-fetch.
  const knownDynamicStream = !!(
    ctx &&
    !isHls &&
    resolvedSize <= 0 &&
    ctx.statusCode === 200 &&
    ctx.acceptRanges !== "bytes"
  );

  const isSocketOpen = !!(connected && ws && ws.readyState === WebSocket.OPEN);
  const baseInput = {
    enabled: runtimeEnabled,
    connected: isSocketOpen,
    url: downloadUrl,
    method: item.method || "GET",
    filename: resolvedFilename,
    fileSize: resolvedSize,
    knownDynamicStream,
    settings: runtimeSettings,
    bypass: once || held,
    restored: isRestoredDownloadEvent(item),
  };

  const decision = interceptDecision(baseInput);
  if (decision.take) {
    void takeover(item, downloadUrl);
    return;
  }

  // MV3 Service Worker wake-up path: if the only reason we declined was `offline`
  // while `ws` has not yet finished reconnecting after SW suspension, pause the
  // browser download briefly, attempt a fast localhost handshake, and either
  // take over or seamlessly resume the browser download.
  if (
    decision.reason === "offline" &&
    runtimeEnabled &&
    interceptDecision({ ...baseInput, connected: true }).take
  ) {
    void (async () => {
      await pauseBrowserDownload(item.id);
      await runtimeReady;
      if (!runtimeEnabled) {
        resumeBrowserDownload(item.id);
        return;
      }
      const woke = await ensureConnected(WAKE_CONNECT_MS);
      if (woke) {
        await takeover(item, downloadUrl);
      } else {
        resumeBrowserDownload(item.id);
        notifyNotRunning({ allowStartupGrace: true });
      }
    })();
    return;
  }

  if (decision.reason === "bypass" && once) clearBypassNext();
  debug("skip", item.id, decision.reason);
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
  const ctx = requestCtx.get(downloadUrl) || requestCtx.get(item.url) || {};
  const filename =
    basename(item.filename) ||
    determinedFilenames.get(item.id) ||
    ctx.filename ||
    filenameFromUrl(downloadUrl);
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
  const finalCtx = requestCtx.get(url);
  const originCtx = requestCtx.get(item.url);
  const crossOriginRedirect =
    item.finalUrl && item.url && originOf(item.finalUrl) !== originOf(item.url);
  const ctx = finalCtx || originCtx || {};
  // Cross-origin redirects (e.g. app.site.com -> s3.amazonaws.com presigned URL):
  // never replay the first hop's `Authorization` header onto a third-party CDN
  // unless Chrome itself sent it on the final hop (S3 rejects dual auth with 400).
  let capturedHeaders = finalCtx?.requestHeaders;
  if (!capturedHeaders && originCtx?.requestHeaders) {
    if (crossOriginRedirect) {
      const safe = { ...originCtx.requestHeaders };
      delete safe.Authorization;
      delete safe.Cookie;
      capturedHeaders = safe;
    } else {
      capturedHeaders = originCtx.requestHeaders;
    }
  }
  const pageUrl = item.referrer || ctx.pageUrl || tab?.url || "";
  const headers = mediaHeaders({
    captured: capturedHeaders,
    url,
    pageUrl,
    cookies,
    userAgent: navigator.userAgent,
  });
  const resolvedName =
    basename(item.filename) ||
    determinedFilenames.get(item.id) ||
    ctx.filename ||
    filenameFromUrl(url);
  determinedFilenames.delete(item.id);
  return buildDownloadRequest({
    requestId,
    action: "add",
    url: item.url,
    finalUrl: item.finalUrl || item.url,
    filename: resolvedName,
    method: item.method || "GET",
    referrer: headers.Referer || pageUrl,
    tabUrl: tab?.url || "",
    cookies,
    userAgent: navigator.userAgent,
    headers,
    contentType: item.mime || ctx.contentType || "",
    contentLength: item.fileSize > 0 ? item.fileSize : ctx.totalSize || ctx.contentLength || 0,
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
  if (details.type === "main_frame" && details.frameId === 0 && details.tabId >= 0) {
    lastTabUrl.set(details.tabId, details.url);
    if (mediaByTab.delete(details.tabId)) {
      scheduleSessionPersist();
      broadcastStatus();
    }
  }
  sentHeaders.set(details.requestId, filterHeaders(details.requestHeaders || []));
}

/** A request is over: its headers are no longer needed. */
function forgetSentHeaders(details) {
  sentHeaders.delete(details.requestId);
}

function refreshMediaLabels(entry, fallbackTitle = "") {
  const pageTitle = entry.pageTitle || fallbackTitle || "";
  if (pageTitle) entry.pageTitle = pageTitle;
  const desc = describeMediaItem({
    url: entry.url,
    contentType: entry.contentType,
    pageTitle: entry.pageTitle,
    filename: entry.filename,
    resolution: entry.resolution,
  });
  entry.kind = desc.kind;
  entry.badge = desc.badge;
  entry.displayName = desc.displayName;
  entry.suggestedFilename = desc.suggestedFilename;
  return entry;
}

function upsertMediaForTab(tabId, candidate) {
  if (tabId == null || tabId < 0) return false;
  const cleanUrl = normalizeMediaUrl(candidate.url);
  if (!cleanUrl) return false;
  const type = String(candidate.contentType || "").split(";")[0].trim();
  const key = mediaDedupKey(cleanUrl, type);
  const baseKey = mediaDedupKey(cleanUrl, "");
  const list = mediaByTab.get(tabId) || [];
  const pageTitle = candidate.pageTitle || tabTitles.get(tabId) || "";
  const existing = list.find(
    (m) =>
      m.key === key ||
      ((candidate.fromDom || m.fromDom || !type || !m.contentType) &&
        mediaDedupKey(m.url, "") === baseKey),
  );
  if (existing) {
    if (type && (!candidate.fromDom || !existing.contentType)) {
      existing.contentType = type;
      existing.key = mediaDedupKey(cleanUrl, type);
    }
    if (!candidate.fromDom) existing.fromDom = false;
    existing.url = cleanUrl;
    if (candidate.rawUrl) existing.rawUrl = candidate.rawUrl;
    if (candidate.size > 0) existing.size = Math.max(existing.size || 0, candidate.size);
    if (candidate.captured && Object.keys(candidate.captured).length > 0) {
      existing.captured = candidate.captured;
    }
    if (candidate.filename) existing.filename = candidate.filename;
    if (candidate.resolution) existing.resolution = candidate.resolution;
    if (candidate.pageUrl && !existing.pageUrl) {
      existing.pageUrl = candidate.pageUrl;
      existing.referrer = candidate.pageUrl;
    }
    refreshMediaLabels(existing, pageTitle);
    return true;
  }

  const entry = refreshMediaLabels(
    {
      key,
      url: cleanUrl,
      rawUrl: candidate.rawUrl || candidate.url,
      contentType: type,
      size: candidate.size || 0,
      tabId,
      referrer: candidate.pageUrl || "",
      pageUrl: candidate.pageUrl || "",
      filename: candidate.filename || "",
      resolution: candidate.resolution || "",
      pageTitle,
      fromDom: !!candidate.fromDom,
      // Kept with the hit: the user may click Download long after the
      // request finished and sentHeaders was cleared.
      captured: candidate.captured || {},
      capturedAt: Date.now(),
    },
    pageTitle,
  );
  list.unshift(entry);
  mediaByTab.set(tabId, list.slice(0, 50));
  return true;
}

function onHeadersReceived(details) {
  const headers = {};
  for (const h of details.responseHeaders || []) {
    if (h.name && h.value != null) headers[h.name.toLowerCase()] = h.value;
  }
  const type = (headers["content-type"] || "").split(";")[0].trim();
  const sizing = parseResponseSizing(details.responseHeaders || []);
  const cdFilename = parseContentDispositionFilename(headers["content-disposition"] || "");
  const statusCode = details.statusCode || 0;
  // `documentUrl` is the frame the request came from — the page a hotlink
  // check compares against. `initiator` only ever carries its origin.
  const pageUrl = details.documentUrl || details.initiator || "";
  const sent = sentHeaders.get(details.requestId) || {};
  requestCtx.set(details.url, {
    requestHeaders: sent,
    contentType: type,
    contentLength: sizing.contentLength,
    totalSize: sizing.totalSize,
    acceptRanges: sizing.acceptRanges,
    hasContentRange: sizing.hasContentRange,
    statusCode,
    filename: cdFilename,
    pageUrl,
  });
  if (requestCtx.size > REQUEST_CTX_MAX) {
    const oldest = requestCtx.keys().next().value;
    if (oldest !== undefined) requestCtx.delete(oldest);
  }
  // requestCtx above is still filled while sniffing is off: interception
  // needs it. Only media detection is what the switch turns off.
  if (!runtimeSniff) return;
  if (statusCode >= 400) return;
  if (
    shouldSkipMediaUrl(details.url, {
      contentType: type,
      statusCode,
      size: sizing.totalSize,
      hasContentRange: sizing.hasContentRange,
    })
  ) {
    return;
  }
  // Content type alone loses every file served as application/octet-stream,
  // which is how Bilibili hands over its whole DASH tracks (`….m4s`).
  if (!isMediaResponse({ url: details.url, contentType: type })) return;
  const tabId = details.tabId;
  if (tabId < 0) return;
  upsertMediaForTab(tabId, {
    url: details.url,
    rawUrl: details.url,
    contentType: type,
    size: sizing.totalSize,
    pageUrl,
    filename: cdFilename,
    pageTitle: tabTitles.get(tabId) || "",
    captured: sent,
  });
  scheduleSessionPersist();
  broadcastStatus();
  pushMediaForTab(tabId);
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

chrome.tabs?.onUpdated?.addListener((tabId, changeInfo, tab) => {
  // Clear sniffed media only on genuine document/video navigations, NOT on
  // cosmetic SPA `history.replaceState` tracking-param cleanups (e.g. Bilibili
  // stripping `spm_id_from` 1s into playback) or lazy subframe loads.
  if (changeInfo.url) {
    const prev = lastTabUrl.get(tabId);
    lastTabUrl.set(tabId, changeInfo.url);
    if (prev && navigationClearsMedia(prev, changeInfo.url)) {
      mediaByTab.delete(tabId);
      tabTitles.delete(tabId);
      scheduleSessionPersist();
      broadcastStatus();
      pushMediaForTab(tabId);
    }
  } else if (tab?.url && !lastTabUrl.has(tabId)) {
    lastTabUrl.set(tabId, tab.url);
  }

  if (changeInfo.title) {
    tabTitles.set(tabId, changeInfo.title);
    const list = mediaByTab.get(tabId);
    if (list && list.length > 0) {
      for (const item of list) {
        item.pageTitle = changeInfo.title;
        refreshMediaLabels(item, changeInfo.title);
      }
      scheduleSessionPersist();
      broadcastStatus();
      pushMediaForTab(tabId);
    }
  }
});

chrome.tabs?.onRemoved?.addListener((tabId) => {
  mediaByTab.delete(tabId);
  tabTitles.delete(tabId);
  lastTabUrl.delete(tabId);
  bypassTabs.delete(tabId);
  scheduleSessionPersist();
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
    if (changes[SNIFF_KEY]) runtimeSniff = changes[SNIFF_KEY].newValue !== false;
    if (changes[PANEL_KEY]) {
      runtimePanel = changes[PANEL_KEY].newValue !== false;
      // Turning the panel back on clears each page's own close.
      pushMediaToAllTabs(true);
    }
    if (changes[HIDDEN_HOSTS_KEY]) {
      runtimeHiddenHosts = Array.isArray(changes[HIDDEN_HOSTS_KEY].newValue)
        ? changes[HIDDEN_HOSTS_KEY].newValue
        : [];
    }
    if (changes[SNIFF_KEY] || changes[HIDDEN_HOSTS_KEY]) pushMediaToAllTabs();
  }
  if (area === "session" && changes[BYPASS_NEXT_KEY]) {
    bypassNext = !!changes[BYPASS_NEXT_KEY].newValue;
  }
});

chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.action === "sendUrl") {
    (async () => {
      await runtimeReady;
      await ensureConnected();
      const req = await buildFromTab(request.url, sender.tab);
      const ok = await sendReliable(req);
      sendResponse({ ok });
    })();
    return true;
  }
  if (request.action === "page-meta") {
    (async () => {
      await runtimeReady;
      const tabId = sender.tab?.id;
      if (tabId == null || tabId < 0) {
        sendResponse({ ok: false });
        return;
      }
      let changed = false;
      if (request.title) {
        tabTitles.set(tabId, String(request.title));
        const existing = mediaByTab.get(tabId);
        if (existing && existing.length > 0) {
          for (const item of existing) {
            item.pageTitle = String(request.title);
            refreshMediaLabels(item, String(request.title));
          }
          changed = true;
        }
      }
      if (runtimeSniff && Array.isArray(request.mediaElements)) {
        const pageUrl = request.pageUrl || sender.tab?.url || "";
        for (const el of request.mediaElements) {
          const url = String(el?.url || "");
          if (!/^https?:/i.test(url) || shouldSkipMediaUrl(url)) continue;
          const fallbackType = el.type || (el.kind === "audio" ? "audio/mp4" : "video/mp4");
          if (
            upsertMediaForTab(tabId, {
              url,
              rawUrl: url,
              contentType: fallbackType,
              size: 0,
              pageUrl,
              resolution: el.resolution || "",
              pageTitle: tabTitles.get(tabId) || "",
              fromDom: true,
            })
          ) {
            changed = true;
          }
        }
      }
      if (changed) {
        scheduleSessionPersist();
        broadcastStatus();
        pushMediaForTab(tabId);
      }
      sendResponse({ ok: true });
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
        sniff: runtimeSniff,
        panel: runtimePanel,
        hiddenHosts: runtimeHiddenHosts,
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
  if (request.action === "media-for-tab") {
    (async () => {
      await runtimeReady;
      const tab = sender.tab || (await activeTab());
      sendResponse(mediaPayloadForTab(tab));
    })();
    return true;
  }
  if (request.action === "set-sniff") {
    runtimeSniff = request.sniff !== false;
    chrome.storage.local.set({ [SNIFF_KEY]: runtimeSniff }).then(() => {
      pushMediaToAllTabs();
      sendResponse({ ok: true, sniff: runtimeSniff });
    });
    return true;
  }
  if (request.action === "set-panel") {
    runtimePanel = request.panel !== false;
    chrome.storage.local.set({ [PANEL_KEY]: runtimePanel }).then(() => {
      // reset: a page the user closed the panel on gets it back when the
      // switch is turned on, which is the only way to undo that close.
      pushMediaToAllTabs(true);
      sendResponse({ ok: true, panel: runtimePanel });
    });
    return true;
  }
  if (request.action === "hide-panel-site") {
    const host = String(request.host || hostnameOf(sender.tab?.url || "")).toLowerCase();
    if (host) {
      runtimeHiddenHosts = addHiddenHost(runtimeHiddenHosts, host);
      chrome.storage.local.set({ [HIDDEN_HOSTS_KEY]: runtimeHiddenHosts });
    }
    pushMediaToAllTabs();
    sendResponse({ ok: true, hiddenHosts: runtimeHiddenHosts });
    return true;
  }
  if (request.action === "unhide-panel-site") {
    runtimeHiddenHosts = removeHiddenHost(runtimeHiddenHosts, request.host);
    chrome.storage.local.set({ [HIDDEN_HOSTS_KEY]: runtimeHiddenHosts });
    pushMediaToAllTabs();
    sendResponse({ ok: true, hiddenHosts: runtimeHiddenHosts });
    return true;
  }
  if (request.action === "download-media") {
    (async () => {
      await runtimeReady;
      await ensureConnected();
      const tab = request.tabId != null
        ? await chrome.tabs.get(request.tabId).catch(() => null)
        : sender.tab || (await activeTab());
      const list = mediaByTab.get(tab?.id ?? request.tabId ?? -1) || [];
      const hit =
        list.find(
          (m) =>
            (request.key && m.key === request.key) ||
            m.url === request.url ||
            m.rawUrl === request.url,
        ) || {};
      const targetUrl = hit.url || normalizeMediaUrl(request.url) || request.url;
      const cookies = await cookiesFor(targetUrl);
      // The page the media played on, in this order: the frame the request
      // came from, the tab it belongs to, then whatever the caller knew.
      // `details.initiator` is an origin, never a page path, so it is last.
      const pageUrl = hit.pageUrl || tab?.url || request.tabUrl || request.referrer || "";
      const headers = mediaHeaders({
        captured: hit.captured,
        url: targetUrl,
        pageUrl,
        cookies,
        userAgent: navigator.userAgent,
      });
      const filename =
        hit.suggestedFilename ||
        describeMediaItem({
          url: targetUrl,
          contentType: request.contentType || hit.contentType || "",
          pageTitle: tabTitles.get(tab?.id ?? -1) || hit.pageTitle || tab?.title || "",
          filename: hit.filename || "",
          resolution: hit.resolution || "",
        }).suggestedFilename ||
        filenameFromUrl(targetUrl);
      const req = buildDownloadRequest({
        url: targetUrl,
        finalUrl: targetUrl,
        filename,
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

function hostnameOf(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

/**
 * What the page-side panel needs to know: the sniffed list and whether it
 * belongs on this page. Visibility is decided here, not in the content script,
 * so the rule lives once — in protocol.panelVisible, under test — and the
 * content script never needs the settings or the hidden-site list.
 */
function mediaPayloadForTab(tab, reset) {
  const media = mediaByTab.get(tab?.id ?? -1) || [];
  return {
    media,
    visible: panelVisible({
      sniff: runtimeSniff,
      panel: runtimePanel,
      hiddenHosts: runtimeHiddenHosts,
      hostname: hostnameOf(tab?.url || ""),
      mediaCount: media.length,
    }),
    ...(reset ? { reset: true } : {}),
  };
}

function sendMedia(tab, reset) {
  if (tab?.id == null) return;
  chrome.tabs.sendMessage(
    tab.id,
    { action: "proxydm-media", ...mediaPayloadForTab(tab, reset) },
    () => {
      // No content script on this tab (chrome:// pages, the store, a PDF
      // viewer). Reading lastError keeps Chrome from logging it as unchecked.
      const error = chrome.runtime.lastError;
      if (error) debug("media push skipped", tab.id, error.message);
    },
  );
}

function pushMediaForTab(tabId, reset) {
  if (tabId == null || tabId < 0) return;
  chrome.tabs.get(tabId, (tab) => {
    if (chrome.runtime.lastError || !tab) return;
    sendMedia(tab, reset);
  });
}

function pushMediaToAllTabs(reset) {
  chrome.tabs.query({}, (tabs) => {
    if (chrome.runtime.lastError || !tabs) return;
    for (const tab of tabs) sendMedia(tab, reset);
  });
}

function broadcastStatus() {
  chrome.runtime.sendMessage({ action: "status-changed" }).catch(() => {});
}

async function loadRuntime() {
  try {
    const r = await chrome.storage.local.get([
      STORAGE_KEY,
      SETTINGS_KEY,
      SNIFF_KEY,
      PANEL_KEY,
      HIDDEN_HOSTS_KEY,
    ]);
    runtimeEnabled = r[STORAGE_KEY] !== false;
    runtimeSettings = { ...defaultSettings, ...(r[SETTINGS_KEY] || {}) };
    runtimeSniff = r[SNIFF_KEY] !== false;
    runtimePanel = r[PANEL_KEY] !== false;
    runtimeHiddenHosts = Array.isArray(r[HIDDEN_HOSTS_KEY]) ? r[HIDDEN_HOSTS_KEY] : [];
  } catch {
    /* keep defaults until storage is readable */
  }
  try {
    const stored = await chrome.storage.session?.get?.([
      BYPASS_NEXT_KEY,
      SESSION_MEDIA_KEY,
      SESSION_TITLES_KEY,
    ]);
    bypassNext = !!(stored && stored[BYPASS_NEXT_KEY]);
    const savedTitles = stored?.[SESSION_TITLES_KEY];
    if (savedTitles && typeof savedTitles === "object") {
      for (const [k, v] of Object.entries(savedTitles)) {
        const id = Number(k);
        if (Number.isFinite(id) && v && !tabTitles.has(id)) {
          tabTitles.set(id, String(v));
        }
      }
    }
    const savedMedia = stored?.[SESSION_MEDIA_KEY];
    if (savedMedia && typeof savedMedia === "object") {
      for (const [k, v] of Object.entries(savedMedia)) {
        const id = Number(k);
        if (Number.isFinite(id) && Array.isArray(v) && v.length > 0 && !mediaByTab.has(id)) {
          mediaByTab.set(id, v);
        }
      }
    }
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
