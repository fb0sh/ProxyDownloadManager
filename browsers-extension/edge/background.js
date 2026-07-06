// =============================================================================
// background.js — ProxyDM Browser Extension
//
// Click toolbar icon → toggle extension on/off (icon changes).
// When enabled: intercept downloads, send URL to ProxyDM via WebSocket,
// cancel the browser download so ProxyDM handles it.
// =============================================================================

const WS_URL = 'ws://127.0.0.1:18999';
let ws = null;
let reconnectTimer = null;

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
  if (ws && ws.readyState === WebSocket.OPEN) return;
  try { ws = new WebSocket(WS_URL); } catch { scheduleReconnect(); return; }
  ws.onopen = () => { if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; } };
  ws.onclose = () => { ws = null; scheduleReconnect(); };
  ws.onerror = () => { ws = null; scheduleReconnect(); };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => { reconnectTimer = null; connect(); }, 3000);
}

function disconnect() {
  if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
  if (ws) { ws.close(); ws = null; }
}

function send(url, referrer, tabTitle) {
  if (!ws || ws.readyState !== WebSocket.OPEN) { connect(); return false; }
  try {
    ws.send(url);
    return true;
  } catch { return false; }
}

function sendReliable(url, referrer, tabTitle) {
  return new Promise((resolve) => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      try { ws.send(url); resolve(true); } catch { resolve(false); }
      return;
    }

    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }

    let done = false;
    const finish = (ok) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      resolve(ok);
    };
    const timer = setTimeout(() => finish(false), 2000);

    try {
      ws = new WebSocket(WS_URL);
      ws.onopen = () => {
        try { ws.send(url); finish(true); } catch { finish(false); }
      };
      ws.onclose = () => { ws = null; if (!done) finish(false); else scheduleReconnect(); };
      ws.onerror = () => { if (!done) finish(false); };
    } catch {
      finish(false);
    }
  });
}

// ─── Context menus ────────────────────────────────────────────────────────────

function createContextMenus() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: 'dl-link', title: 'Download with ProxyDM', contexts: ['link', 'video', 'audio'] });
    chrome.contextMenus.create({ id: 'dl-page', title: 'Download page with ProxyDM', contexts: ['page'] });
    chrome.contextMenus.create({ id: 'dl-sel',  title: 'Download selected link with ProxyDM', contexts: ['selection'] });
  });
}

function destroyContextMenus() {
  chrome.contextMenus.removeAll();
}

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  let url = null;
  switch (info.menuItemId) {
    case 'dl-link': url = info.linkUrl || info.srcUrl; break;
    case 'dl-page': url = tab?.url; break;
    case 'dl-sel':  url = extractUrl(info.selectionText); break;
  }
  if (url && !(await sendReliable(url, tab?.url || '', tab?.title || ''))) notifyNotRunning();
});

// ─── Download interception ────────────────────────────────────────────────────

chrome.downloads.onCreated.addListener(async (item) => {
  if (!(await isEnabled())) {
    console.log('[ProxyDM] disabled, letting browser handle:', item.url);
    return;
  }
  if (!item.url || item.url.startsWith('blob:')) return;

  console.log('[ProxyDM] download intercepted:', item.url, 'file:', item.filename);

  chrome.tabs.query({ active: true, currentWindow: true }, async ([tab]) => {
    const ok = await sendReliable(item.url, tab?.url || '', tab?.title || '');
    console.log('[ProxyDM] sent to app:', ok);
    if (ok) {
      chrome.downloads.cancel(item.id, () => {
        if (chrome.runtime.lastError) console.debug(chrome.runtime.lastError.message);
        chrome.downloads.erase({ id: item.id });
      });
    } else {
      notifyNotRunning();
    }
  });
});

// ─── Initialization ───────────────────────────────────────────────────────────

chrome.runtime.onInstalled.addListener(async () => {
  const on = await isEnabled();
  updateIcon(on);
  if (on) { createContextMenus(); connect(); }
});

chrome.runtime.onStartup.addListener(async () => {
  const on = await isEnabled();
  updateIcon(on);
  if (on) { createContextMenus(); connect(); }
});

// ─── Message from content script ──────────────────────────────────────────────

chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.action === 'sendUrl') {
    sendReliable(request.url, '', '').then((ok) => sendResponse({ ok }));
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

function notifyNotRunning() {
  notify('ProxyDM is not running', 'Using the browser download instead. Start ProxyDM to capture downloads.');
}

function looksLikeDownload(url) {
  const path = url.split(/[?#]/)[0].toLowerCase();
  const exts = ['.zip','.rar','.7z','.tar','.gz','.xz','.bz2','.zst',
    '.tar.gz','.tar.xz','.tar.bz2','.tgz','.txz',
    '.exe','.msi','.dmg','.pkg','.apk','.deb','.rpm',
    '.iso','.img','.vhd',
    '.pdf','.epub','.mobi',
    '.mp4','.mkv','.avi','.mov','.wmv','.webm','.m4v',
    '.mp3','.flac','.wav','.aac','.ogg','.opus','.m4a',
    '.jar','.war','.nupkg','.whl',
    '.ttf','.otf','.woff','.woff2',
    '.dmp','.core','.crx','.xpi'];
  return exts.some(e => path.endsWith(e));
}

function extractUrl(text) {
  if (!text) return null;
  const m = text.match(/https?:\/\/[^\s<>"']+/);
  return m ? m[0] : null;
}
