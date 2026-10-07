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
  return false;
});

// The panel is created by the answer or by the first push from the service
// worker, so an empty page shows nothing at all. A push can be missed (it
// arrived while this document was loading, or the tab's list was cleared by a
// navigation), so ask again as the page settles and whenever it comes back.
function askForMedia() {
  chrome.runtime.sendMessage({ action: "media-for-tab" }).catch(() => {});
}

askForMedia();
document.addEventListener("DOMContentLoaded", askForMedia, { once: true });
window.addEventListener("pageshow", askForMedia);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") askForMedia();
});
