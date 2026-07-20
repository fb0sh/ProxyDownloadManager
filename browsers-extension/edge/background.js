// =============================================================================
// background.js — ProxyDM Browser Extension (shared source)
//
// Click toolbar icon → toggle extension on/off (icon changes).
// When enabled: intercept downloads, send URL to ProxyDM via WebSocket,
// cancel the browser download so ProxyDM handles it.
//
// This file is the single source of truth. Run build.sh to sync to
// chrome/, edge/, firefox/ directories.
// =============================================================================

const WS_URL = 'ws://127.0.0.1:18999';
let ws = null;
let reconnectTimer = null;
let lastNotRunningNotificationAt = 0;

const startedAt = Date.now();
const STARTUP_GRACE_MS = 10000;
const NOT_RUNNING_NOTIFICATION_COOLDOWN_MS = 15000;

// ─── Toggle on/off ────────────────────────────────────────────────────────────

const STORAGE_KEY = 'proxydm_enabled';

async function isEnabled() {
  const r = await chrome.storage.local.get(STORAGE_KEY);
  return r[STORAGE_KEY] !== false;
}

async function setEnabled(enabled) {
  await chrome.storage.local.set({ [STORAGE_KEY]: enabled });
  updateIcon(enabled);
  if (enabled) {
    createContextMenus();
    connect();
  } else {
    destroyContextMenus();
    disconnect();
  }
}

chrome.action.onClicked.addListener(async () => {
  const on = await isEnabled();
  await setEnabled(!on);
});

// ─── Icon ─────────────────────────────────────────────────────────────────────

function updateIcon(enabled) {
  const suffix = enabled ? '' : '_off';
  chrome.action.setIcon({
    path: {
      16: `icons/icon16${suffix}.png`,
      48: `icons/icon48${suffix}.png`,
      128: `icons/icon128${suffix}.png`
    }
  });
  chrome.action.setTitle({ title: enabled ? 'ProxyDM enabled (click to disable)' : 'ProxyDM disabled (click to enable)' });
  if (enabled) {
    chrome.action.setBadgeText({ text: '' });
  } else {
    chrome.action.setBadgeText({ text: '✕' });
    chrome.action.setBadgeBackgroundColor({ color: '#cf222e' });
  }
}

// ─── WebSocket ────────────────────────────────────────────────────────────────

function connect() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    console.log('[ProxyDM] WS already connected, skipping');
    return;
  }
  console.log('[ProxyDM] WS connecting to', WS_URL);
  let socket;
  try { socket = new WebSocket(WS_URL); } catch (e) {
    console.warn('[ProxyDM] WS new WebSocket failed:', e);
    scheduleReconnect(); return;
  }
  ws = socket;
  socket.onopen = () => {
    if (ws !== socket) return;
    console.log('[ProxyDM] WS connected');
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
  };
  socket.onclose = (evt) => {
    if (ws !== socket) return;
    console.warn('[ProxyDM] WS closed code=' + evt.code + ' reason=' + evt.reason);
    ws = null;
    scheduleReconnect();
  };
  socket.onerror = () => {
    if (ws !== socket) return;
    console.warn('[ProxyDM] WS error');
    ws = null;
    scheduleReconnect();
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => { reconnectTimer = null; connect(); }, 3000);
}

function disconnect() {
  if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
  if (ws) { ws.close(); ws = null; }
}

// ─── Reliable send (fresh WebSocket per message) ──────────────────────────────

function sendReliable(url) {
  console.log('[ProxyDM] WS sendReliable:', url);
  return new Promise((resolve) => {
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }

    let socket = null;
    let done = false;
    const finish = (ok) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
        try { socket.close(); } catch {}
      }
      console.log('[ProxyDM] WS sendReliable result:', ok ? 'ACK' : 'FAIL');
      resolve(ok);
    };
    const timer = setTimeout(() => {
      console.warn('[ProxyDM] WS sendReliable timeout (3s)');
      finish(false);
    }, 3000);

    try {
      socket = new WebSocket(WS_URL);
      socket.onopen = () => {
        try { socket.send(url); } catch (e) {
          console.warn('[ProxyDM] WS sendReliable send failed:', e);
          finish(false);
        }
      };
      socket.onmessage = (evt) => {
        console.debug('[ProxyDM] WS sendReliable ack:', evt.data);
        finish(true);
      };
      socket.onclose = (evt) => {
        console.warn('[ProxyDM] WS sendReliable closed code=' + evt.code);
        if (!done) finish(false);
      };
      socket.onerror = () => {
        console.warn('[ProxyDM] WS sendReliable error');
        if (!done) finish(false);
      };
    } catch (e) {
      console.error('[ProxyDM] WS sendReliable exception:', e);
      finish(false);
    }
  });
}

// ─── Context menus ────────────────────────────────────────────────────────────

function createContextMenus() {
  console.log('[ProxyDM] creating context menus');
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: 'dl-link', title: 'Download with ProxyDM', contexts: ['link', 'video', 'audio'] });
    chrome.contextMenus.create({ id: 'dl-page', title: 'Download page with ProxyDM', contexts: ['page'] });
    chrome.contextMenus.create({ id: 'dl-sel',  title: 'Download selected link with ProxyDM', contexts: ['selection'] });
  });
}

function destroyContextMenus() {
  console.log('[ProxyDM] destroying context menus');
  chrome.contextMenus.removeAll();
}

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  let url = null;
  switch (info.menuItemId) {
    case 'dl-link': url = info.linkUrl || info.srcUrl; break;
    case 'dl-page': url = tab?.url; break;
    case 'dl-sel':  url = extractUrl(info.selectionText); break;
  }
  if (url && !(await sendReliable(url))) notifyNotRunning();
});

// ─── Download interception ────────────────────────────────────────────────────

chrome.downloads.onCreated.addListener(async (item) => {
  if (!(await isEnabled())) {
    console.log('[ProxyDM] disabled, letting browser handle:', item.url);
    return;
  }
  const downloadUrl = getDownloadUrl(item);
  if (!downloadUrl || downloadUrl.startsWith('blob:')) return;
  if (isRestoredDownloadEvent(item)) {
    console.log('[ProxyDM] restored browser download ignored:', downloadUrl, item.startTime);
    return;
  }

  console.log('[ProxyDM] download intercepted:', downloadUrl, 'file:', item.filename);

  chrome.tabs.query({ active: true, currentWindow: true }, async ([tab]) => {
    const ok = await sendReliable(downloadUrl);
    console.log('[ProxyDM] sent to app:', ok);
    if (ok) {
      chrome.downloads.cancel(item.id, () => {
        if (chrome.runtime.lastError) console.warn('[ProxyDM] cancel error:', chrome.runtime.lastError.message);
        chrome.downloads.erase({ id: item.id });
        console.log('[ProxyDM] browser download cancelled + erased:', item.id);
      });
    } else {
      notifyNotRunning({ allowStartupGrace: true });
    }
  });
});

// ─── Initialization ───────────────────────────────────────────────────────────

chrome.runtime.onInstalled.addListener(async () => {
  const on = await isEnabled();
  console.log('[ProxyDM] runtime.onInstalled enabled=', on);
  updateIcon(on);
  if (on) { createContextMenus(); connect(); }
});

chrome.runtime.onStartup.addListener(async () => {
  const on = await isEnabled();
  console.log('[ProxyDM] runtime.onStartup enabled=', on);
  updateIcon(on);
  if (on) { createContextMenus(); connect(); }
});

// ─── Message from content script (if present) ─────────────────────────────────

chrome.runtime.onMessage.addListener((request, _sender, sendResponse) => {
  console.log('[ProxyDM] content script message:', request.action, request.url?.slice(0, 80));
  if (request.action === 'sendUrl') {
    sendReliable(request.url).then((ok) => sendResponse({ ok }));
    return true;
  }
});

// ─── Helpers ──────────────────────────────────────────────────────────────────

function notify(title, message) {
  if (!chrome.notifications) return;
  chrome.notifications.create({
    type: 'basic',
    iconUrl: 'icons/icon128.png',
    title,
    message
  });
}

function notifyNotRunning({ allowStartupGrace = false } = {}) {
  const now = Date.now();
  const inStartupGrace = allowStartupGrace && now - startedAt < STARTUP_GRACE_MS;
  const inCooldown = now - lastNotRunningNotificationAt < NOT_RUNNING_NOTIFICATION_COOLDOWN_MS;

  if (!inStartupGrace && !inCooldown) {
    notify('ProxyDM is not running', 'Using the browser download instead. Start ProxyDM to capture downloads.');
    lastNotRunningNotificationAt = now;
  }

  // Show a warning badge until the extension is enabled again
  chrome.action.setBadgeText({ text: '!' });
  chrome.action.setBadgeBackgroundColor({ color: '#cf222e' });
  setTimeout(() => {
    chrome.action.setBadgeText({ text: '' });
  }, 10000);
}

function getDownloadUrl(item) {
  return item.finalUrl || item.url || '';
}

function isRestoredDownloadEvent(item) {
  const startTime = Date.parse(item.startTime || '');
  return Number.isFinite(startTime) && startTime + STARTUP_GRACE_MS < startedAt;
}

function extractUrl(text) {
  if (!text) return null;
  const m = text.match(/https?:\/\/[^\s<>"']+/);
  return m ? m[0] : null;
}
