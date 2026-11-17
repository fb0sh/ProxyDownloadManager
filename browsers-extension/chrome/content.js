// =============================================================================
// content.js — the page-side half of the extension.
//
// Two jobs: keep the "hold a key to let the browser take this one" shortcut
// working, and host the media panel. The panel's *visibility* is decided by
// the service worker (it owns the switches and the hidden-site list) and
// arrives with every update; what lives here is only this page's own state.
// =============================================================================

let bypass = false;
let media = [];
let closedHere = false;

const PANEL = globalThis.__proxydmPanel;

function setBypass(on) {
  if (bypass === on) return;
  bypass = on;
  chrome.runtime.sendMessage({ action: "proxydm-bypass-change", bypass: on }).catch(() => {});
}

window.addEventListener(
  "keydown",
  (e) => {
    if (e.altKey || e.key === "Delete" || e.key === "Alt") setBypass(true);
  },
  true
);
window.addEventListener(
  "keyup",
  (e) => {
    if (!e.altKey && e.key !== "Delete") setBypass(false);
  },
  true
);
window.addEventListener("blur", () => setBypass(false));

function handlers() {
  return {
    async onDownload(item) {
      const reply = await chrome.runtime
        .sendMessage({
          action: "download-media",
          key: item.key,
          url: item.url,
          contentType: item.contentType,
          tabId: item.tabId,
          referrer: item.referrer,
          pageUrl: item.pageUrl,
          size: item.size,
        })
        .catch(() => null);
      return !!reply?.ok;
    },
    onHideSite() {
      closedHere = true;
      render(false);
      chrome.runtime.sendMessage({ action: "hide-panel-site", host: location.hostname }).catch(() => {});
    },
    onClose() {
      // For this page only: a reload, or turning the switch off and on again,
      // brings the panel back.
      closedHere = true;
    },
  };
}

function render(visible) {
  const show = visible && !closedHere;
  if (show) PANEL?.update(media, handlers(), true);
  else PANEL?.setVisible(false);
}

function resolveHttpUrl(raw) {
  if (!raw || raw.startsWith("blob:") || raw.startsWith("data:") || raw.startsWith("javascript:")) {
    return "";
  }
  try {
    const resolved = new URL(raw, location.href).href;
    return /^https?:/i.test(resolved) ? resolved : "";
  } catch {
    return "";
  }
}

function extractPageTitle() {
  const og =
    document.querySelector('meta[property="og:title"]')?.getAttribute("content") ||
    document.querySelector('meta[name="twitter:title"]')?.getAttribute("content") ||
    "";
  return (document.title || og || "").trim();
}

function collectDomMediaElements() {
  const items = [];
  const seen = new Set();
  for (const el of document.querySelectorAll("video, audio")) {
    const kind = el.tagName.toLowerCase() === "audio" ? "audio" : "video";
    const resolution =
      kind === "video" && el.videoWidth > 0 && el.videoHeight > 0
        ? `${el.videoWidth}x${el.videoHeight}`
        : "";
    const candidates = [
      { raw: el.currentSrc, type: "" },
      { raw: el.src, type: "" },
      { raw: el.getAttribute("src"), type: el.getAttribute("type") || "" },
    ];
    for (const source of el.querySelectorAll("source")) {
      candidates.push({
        raw: source.src || source.getAttribute("src"),
        type: source.getAttribute("type") || "",
      });
    }
    for (const c of candidates) {
      const url = resolveHttpUrl(c.raw);
      if (!url || seen.has(url)) continue;
      seen.add(url);
      items.push({ url, kind, type: c.type, resolution });
    }
  }
  return items;
}

function extractSelectionUrls() {
  const urls = [];
  const push = (raw) => {
    const u = resolveHttpUrl(raw);
    if (u && !urls.includes(u)) urls.push(u);
  };
  const sel = window.getSelection?.();
  if (!sel) return urls;

  const anchorLink = sel.anchorNode?.parentElement?.closest?.("a[href]");
  if (anchorLink) push(anchorLink.getAttribute("href") || anchorLink.href);

  for (let i = 0; i < sel.rangeCount; i += 1) {
    try {
      const frag = sel.getRangeAt(i).cloneContents();
      for (const a of frag.querySelectorAll?.("a[href]") || []) {
        push(a.getAttribute("href") || a.href);
      }
      for (const m of frag.querySelectorAll?.("video[src], audio[src], source[src]") || []) {
        push(m.getAttribute("src") || m.src);
      }
    } catch {
      /* detached range */
    }
  }

  const text = sel.toString() || "";
  const matches = text.match(/https?:\/\/[^\s<>"')]+/gi) || [];
  for (const m of matches) push(m);
  return urls;
}

let metaTimer = null;
let lastMetaSig = "";

function reportPageMeta() {
  const title = extractPageTitle();
  const mediaElements = collectDomMediaElements();
  if (!title && mediaElements.length === 0) return;
  const sig = `${title}|${location.href}|${mediaElements.map((m) => `${m.url}:${m.resolution}`).join(",")}`;
  if (sig === lastMetaSig) return;
  lastMetaSig = sig;
  chrome.runtime
    .sendMessage({
      action: "page-meta",
      title,
      pageUrl: location.href,
      mediaElements,
    })
    .catch(() => {});
}

function schedulePageMeta() {
  if (metaTimer) return;
  metaTimer = setTimeout(() => {
    metaTimer = null;
    reportPageMeta();
  }, 150);
}

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg?.action === "proxydm-media") {
    media = Array.isArray(msg.media) ? msg.media : [];
    // The switch went back on: forget this page's own close.
    if (msg.reset) closedHere = false;
    render(!!msg.visible);
    return false;
  }
  if (msg?.action === "proxydm-bypass-state") {
    sendResponse({ bypass });
    return true;
  }
  if (msg?.action === "proxydm-extract-selection") {
    sendResponse({ urls: extractSelectionUrls() });
    return true;
  }
  return false;
});

// The panel is created by the answer or by the first push from the service
// worker, so an empty page shows nothing at all. A push can be missed (it
// arrived while this document was loading, or the tab's list was cleared by a
// navigation), so ask again as the page settles and whenever it comes back.
function askForMedia() {
  chrome.runtime
    .sendMessage({ action: "media-for-tab" })
    .then((reply) => {
      if (reply && Array.isArray(reply.media)) {
        media = reply.media;
        if (reply.reset) closedHere = false;
        render(!!reply.visible);
      }
    })
    .catch(() => {});
}

let titleObserver = null;

function observeTitleChanges() {
  if (titleObserver || typeof MutationObserver === "undefined") return;
  const target = document.head || document.querySelector("title");
  if (!target) return;
  titleObserver = new MutationObserver(schedulePageMeta);
  titleObserver.observe(target, { subtree: true, childList: true, characterData: true });
}

askForMedia();
schedulePageMeta();
document.addEventListener(
  "DOMContentLoaded",
  () => {
    askForMedia();
    schedulePageMeta();
    observeTitleChanges();
  },
  { once: true },
);
if (document.readyState !== "loading") {
  observeTitleChanges();
}
document.addEventListener("loadedmetadata", schedulePageMeta, true);
window.addEventListener("pageshow", () => {
  askForMedia();
  schedulePageMeta();
});
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    askForMedia();
    schedulePageMeta();
  }
});
