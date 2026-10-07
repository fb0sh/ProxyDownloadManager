// =============================================================================
// panel.js — the on-page media panel (ProxyDM)
//
// Content scripts cannot use ES modules, so this is a plain script that
// publishes one namespaced global for content.js to drive. The UI lives in a
// shadow root: page CSS cannot reach it and it cannot leak out, which is what
// lets us keep the panel's own theme instead of fighting `all: revert` rules
// the way the NeatDownloadManager extension does.
//
// Placement follows NeatDownloadManager: the panel sits just above the media
// element the page is playing, with a caret pointing at it. When no element can
// be found (MSE/blob players, HLS pulled by XHR, media we cannot see) it falls
// back to the bottom-right corner.
//
// Strings come from chrome.i18n (a content script has no module to import
// i18n.js from); the palette is the app's, written as hex because the manifest
// still allows Chrome 88 and oklch needs 111.
// =============================================================================

(() => {
  const HOST_ID = "proxydm-media-panel";
  const IDLE_FADE_MS = 15000;
  const GAP_ABOVE = 6;
  const EDGE = 8;
  const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB"];
  const MEDIA_TAGS = ["video", "audio"];

  function t(key, fallback) {
    try {
      return chrome.i18n.getMessage(key) || fallback;
    } catch {
      return fallback;
    }
  }

  function formatBytes(bytes) {
    const n = Number(bytes || 0);
    if (!n || n < 0) return "";
    let value = n;
    let unit = 0;
    while (value >= 1024 && unit < SIZE_UNITS.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return `${unit === 0 ? value : value.toFixed(1)} ${SIZE_UNITS[unit]}`;
  }

  function kindOf(item) {
    const type = String(item?.contentType || "").toLowerCase();
    const url = String(item?.url || "").toLowerCase();
    if (type.includes("mpegurl") || /\.m3u8(\?|$)/.test(url)) return "HLS";
    if (type.startsWith("audio/")) return "AUDIO";
    if (type.startsWith("video/")) return "VIDEO";
    return "MEDIA";
  }

  function nameOf(item) {
    try {
      const last = new URL(item.url).pathname.split("/").filter(Boolean).pop() || "";
      return decodeURIComponent(last) || item.url;
    } catch {
      return item.url || "";
    }
  }

  function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
  }

  /** Two URLs point at the same bytes when their parsed forms match. */
  function sameUrl(a, b) {
    if (!a || !b) return false;
    if (a === b) return true;
    try {
      const ua = new URL(a, location.href);
      const ub = new URL(b, location.href);
      return ua.origin === ub.origin && ua.pathname === ub.pathname;
    } catch {
      return false;
    }
  }

  /** Every URL a media element could be carrying for this item. */
  function elementUrls(el) {
    const urls = [el.currentSrc, el.src, el.getAttribute?.("src"), el.getAttribute?.("data")];
    for (const source of el.querySelectorAll?.("source") || []) {
      urls.push(source.src, source.getAttribute?.("src"));
    }
    return urls.filter(Boolean);
  }

  /**
   * The element playing `url`, NeatDownloadManager-style: an exact URL match
   * first, then the largest visible player on the page — which is what a
   * blob:/MSE player needs, since its src never carries the sniffed URL.
   * @param {string} url
   * @param {Document | Element} [root]
   * @returns {HTMLMediaElement | null}
   */
  function pickElement(url, root) {
    const scope = root || document;
    const candidates = [];
    for (const tag of MEDIA_TAGS) candidates.push(...scope.querySelectorAll(tag));
    if (!candidates.length) return null;

    for (const el of candidates) {
      if (elementUrls(el).some((candidate) => sameUrl(candidate, url))) return el;
    }

    let best = null;
    let bestArea = 0;
    for (const el of candidates) {
      const rect = el.getBoundingClientRect?.();
      if (!rect || rect.width <= 0 || rect.height <= 0) continue;
      const style = el.ownerDocument?.defaultView?.getComputedStyle?.(el);
      if (style && (style.visibility === "hidden" || style.display === "none")) continue;
      const area = rect.width * rect.height;
      if (area > bestArea) {
        bestArea = area;
        best = el;
      }
    }
    return best || candidates[0];
  }

  const STYLE = `
    :host { all: initial; }
    .panel, .panel * { box-sizing: border-box; }
    .panel {
      --bg:#ffffff; --fg:#0a0a0a; --muted:#f5f5f5; --muted-fg:#737373;
      --border:#e5e5e5; --primary:#171717; --primary-fg:#fafafa;
      --ok:#009689; --bad:#e7000b;
      position: fixed; right: 16px; bottom: 16px; width: 292px; z-index: 2147483000;
      background: var(--bg); color: var(--fg);
      border: 1px solid var(--border); border-radius: 10px;
      box-shadow: 0 10px 28px rgba(0,0,0,.22), 0 2px 6px rgba(0,0,0,.10);
      font: 12px/1.35 ui-sans-serif, system-ui, -apple-system, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
      transition: opacity .2s ease;
    }
    .panel.idle { opacity: .45; }
    .panel:hover { opacity: 1; }
    @media (prefers-color-scheme: dark) {
      .panel {
        --bg:#0a0a0a; --fg:#fafafa; --muted:#1c1c1c; --muted-fg:#a1a1a1;
        --border:#2a2a2a; --primary:#fafafa; --primary-fg:#171717;
        --ok:#00d3bb; --bad:#ff6467;
        box-shadow: 0 10px 28px rgba(0,0,0,.55), 0 2px 6px rgba(0,0,0,.4);
      }
    }
    .caret {
      position: absolute; bottom: -6px; width: 11px; height: 11px;
      background: var(--muted); border-right: 1px solid var(--border);
      border-bottom: 1px solid var(--border); transform: rotate(45deg); display: none;
    }
    .panel.anchored .caret { display: block; }
    .head {
      display: flex; align-items: center; gap: 6px; height: 24px; padding: 0 6px;
      background: var(--muted); border-bottom: 1px solid var(--border);
      border-radius: 9px 9px 0 0;
    }
    .grip { color: var(--muted-fg); font-size: 12px; letter-spacing: 1px; cursor: grab; user-select: none; touch-action: none; }
    .grip:active { cursor: grabbing; }
    .dot { width: 9px; height: 9px; border-radius: 99px; background: var(--primary); flex: 0 0 9px; }
    .title { font-size: 11px; font-weight: 650; letter-spacing: .2px; }
    .pill {
      font-size: 9.5px; color: var(--muted-fg); background: var(--bg);
      border: 1px solid var(--border); border-radius: 99px; padding: 0 5px;
    }
    .spacer { margin-left: auto; }
    button { font: inherit; border: 0; background: none; color: inherit; cursor: pointer; padding: 0; }
    .icon {
      width: 19px; height: 19px; border-radius: 6px; border: 1px solid var(--border);
      background: var(--bg); display: flex; align-items: center; justify-content: center;
      font-size: 11px; line-height: 1;
    }
    .icon:hover { background: var(--muted); }
    .list { list-style: none; margin: 0; padding: 0; max-height: 154px; overflow-y: auto; }
    .row {
      display: flex; align-items: center; gap: 6px; padding: 0 6px; height: 22px;
      border-top: 1px solid var(--border); cursor: pointer;
    }
    .list li:first-child .row { border-top: 0; }
    .row:hover { background: var(--muted); }
    .kind { flex: 0 0 34px; font-size: 9px; font-weight: 700; letter-spacing: .4px; color: var(--muted-fg); }
    .name { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 6px; }
    .name b { font-size: 11px; font-weight: 550; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .name span { font-size: 10px; color: var(--muted-fg); flex: 0 0 auto; }
    .row.sent .name span { color: var(--ok); }
    .row.failed .name span { color: var(--bad); }
    .go {
      width: 18px; height: 18px; flex: 0 0 18px; border-radius: 5px; border: 1px solid var(--border);
      background: var(--bg); display: flex; align-items: center; justify-content: center; font-size: 10px;
    }
    .go:hover { background: var(--primary); color: var(--primary-fg); border-color: var(--primary); }
    .row.sent .go { border-color: var(--ok); color: var(--ok); }
    .row.failed .go { border-color: var(--bad); color: var(--bad); }
    .panel.collapsed .list { display: none; }
    .panel.collapsed .head { border-bottom: 0; }
  `;

  let host = null;
  let shadow = null;
  let panel = null;
  let listEl = null;
  let countEl = null;
  let caretEl = null;
  let idleTimer = null;
  let followTimer = null;
  let frame = 0;
  let media = [];
  let handlers = {};
  let anchorEl = null;
  // A manual drag wins over auto-follow for the rest of the page's life.
  let freePos = null;

  function ensureMounted() {
    if (host?.isConnected) return;
    host = document.createElement("div");
    host.id = HOST_ID;
    shadow = host.attachShadow({ mode: "open" });
    shadow.innerHTML = `
      <style>${STYLE}</style>
      <div class="panel" role="dialog" aria-label="ProxyDM">
        <span class="caret"></span>
        <div class="head">
          <span class="grip" data-act="drag" title="${t("panelDrag", "Drag")}">⠿</span>
          <span class="dot"></span>
          <span class="title">ProxyDM</span>
          <span class="pill">0</span>
          <button class="icon spacer" data-act="hide-site" title="${t("panelHideSite", "Don't show on this site")}">⊘</button>
          <button class="icon" data-act="collapse" title="${t("panelCollapse", "Collapse")}">–</button>
          <button class="icon" data-act="close" title="${t("panelClose", "Close")}">✕</button>
        </div>
        <ul class="list"></ul>
      </div>`;
    panel = shadow.querySelector(".panel");
    caretEl = shadow.querySelector(".caret");
    listEl = shadow.querySelector(".list");
    countEl = shadow.querySelector(".pill");
    wire();
    const parent = document.documentElement || document.body;
    if (parent) parent.appendChild(host);
    else document.addEventListener("DOMContentLoaded", () => document.documentElement?.appendChild(host), { once: true });
    refreshIdleFade();
  }

  function wire() {
    shadow.addEventListener("click", (event) => {
      const target = event.target;
      const row = target.closest?.(".row");
      if (row) {
        event.preventDefault();
        event.stopPropagation();
        request(Number(row.dataset.index), row);
        return;
      }
      const act = target.closest?.("[data-act]")?.dataset.act;
      if (!act) return;
      event.preventDefault();
      event.stopPropagation();
      if (act === "close") {
        setVisible(false);
        handlers.onClose?.();
      } else if (act === "collapse") {
        panel.classList.toggle("collapsed");
        place();
      } else if (act === "hide-site") {
        handlers.onHideSite?.();
      }
    });

    // Clicks inside the panel must not reach the page's own handlers.
    for (const type of ["mousedown", "mouseup", "pointerdown", "pointerup"]) {
      shadow.addEventListener(type, (event) => event.stopPropagation());
    }

    shadow.addEventListener("mouseenter", () => {
      if (idleTimer) clearTimeout(idleTimer);
      panel.classList.remove("idle");
    });
    shadow.addEventListener("mouseleave", refreshIdleFade);

    // Dragging uses pointer capture: once the grip owns the pointer, moves keep
    // arriving even while the cursor is over the page's iframe or player, which
    // is exactly where a plain mousemove listener goes deaf.
    const grip = shadow.querySelector(".grip");
    let drag = null;
    grip.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      grip.setPointerCapture?.(event.pointerId);
      const rect = panel.getBoundingClientRect();
      drag = { dx: event.clientX - rect.left, dy: event.clientY - rect.top };
      panel.classList.remove("idle");
      if (idleTimer) clearTimeout(idleTimer);
    });
    grip.addEventListener("pointermove", (event) => {
      if (!drag) return;
      event.preventDefault();
      freePos = {
        left: clamp(event.clientX - drag.dx, EDGE, Math.max(EDGE, window.innerWidth - panel.offsetWidth - EDGE)),
        top: clamp(event.clientY - drag.dy, EDGE, Math.max(EDGE, window.innerHeight - panel.offsetHeight - EDGE)),
      };
      anchorEl = null;
      place();
    });
    const endDrag = (event) => {
      if (!drag) return;
      drag = null;
      grip.releasePointerCapture?.(event.pointerId);
      refreshIdleFade();
    };
    grip.addEventListener("pointerup", endDrag);
    grip.addEventListener("pointercancel", endDrag);

    // Follow the page: scrolling containers (capture), window resizes, and a
    // slow tick for layout that keeps changing on its own (a player that
    // resizes itself once the video's metadata arrives).
    window.addEventListener("scroll", schedulePlace, { capture: true, passive: true });
    window.addEventListener("resize", schedulePlace, { passive: true });
    followTimer = setInterval(() => {
      if (!freePos && anchorEl) schedulePlace();
    }, 1000);
  }

  function schedulePlace() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      place();
    });
  }

  /** Put the panel above its media element, or in the corner when unanchored. */
  function place() {
    if (!panel || !host || host.style.display === "none") return;
    if (freePos) {
      panel.classList.remove("anchored");
      panel.style.right = "auto";
      panel.style.bottom = "auto";
      panel.style.left = `${freePos.left}px`;
      panel.style.top = `${freePos.top}px`;
      return;
    }
    if (!anchorEl?.isConnected) {
      panel.classList.remove("anchored");
      panel.style.left = "auto";
      panel.style.top = "auto";
      panel.style.right = "16px";
      panel.style.bottom = "16px";
      return;
    }
    const rect = anchorEl.getBoundingClientRect();
    const width = panel.offsetWidth || 292;
    const height = panel.offsetHeight || 80;
    // Above the element when it fits, otherwise pinned to the top of the
    // viewport — what NeatDownloadManager does for a player that starts high.
    const top = rect.top - height - GAP_ABOVE >= EDGE ? rect.top - height - GAP_ABOVE : EDGE;
    const left = clamp(rect.left, EDGE, Math.max(EDGE, window.innerWidth - width - EDGE));
    panel.classList.add("anchored");
    panel.style.right = "auto";
    panel.style.bottom = "auto";
    panel.style.left = `${left}px`;
    panel.style.top = `${top}px`;
    // Point the caret at the middle of the element.
    const centre = rect.left + rect.width / 2 - left;
    caretEl.style.left = `${clamp(centre - 6, 10, width - 22)}px`;
  }

  function refreshIdleFade() {
    if (idleTimer) clearTimeout(idleTimer);
    idleTimer = setTimeout(() => panel?.classList.add("idle"), IDLE_FADE_MS);
  }

  function render() {
    if (!listEl) return;
    countEl.textContent = String(media.length);
    listEl.replaceChildren(
      ...media.map((item, index) => {
        const row = document.createElement("div");
        row.className = "row";
        row.dataset.index = String(index);
        const size = formatBytes(item.size);
        row.innerHTML = `
          <span class="kind"></span>
          <span class="name"><b></b><span></span></span>
          <span class="go" aria-hidden="true">↓</span>`;
        row.querySelector(".kind").textContent = kindOf(item);
        const name = row.querySelector(".name b");
        name.textContent = nameOf(item);
        name.title = item.url || "";
        row.querySelector(".name span").textContent = size || t("panelDownload", "Download");
        const li = document.createElement("li");
        li.appendChild(row);
        return li;
      }),
    );
  }

  async function request(index, row) {
    const item = media[index];
    if (!item) return;
    const status = row.querySelector(".name span");
    try {
      const ok = await handlers.onDownload?.(item);
      row.classList.toggle("sent", !!ok);
      row.classList.toggle("failed", !ok);
      status.textContent = ok ? t("panelSent", "Sent to ProxyDM") : t("panelFailed", "ProxyDM did not accept it");
    } catch {
      row.classList.add("failed");
      status.textContent = t("panelFailed", "ProxyDM did not accept it");
    }
  }

  function setVisible(visible) {
    // Never mount just to hide: a page the panel does not belong on keeps no
    // shadow host at all.
    if (!host) return;
    host.style.display = visible ? "" : "none";
    if (visible) schedulePlace();
  }

  globalThis.__proxydmPanel = {
    /**
     * Render the list and show or hide the panel. `visible` is authoritative:
     * content.js owns the settings, the site hide list and the per-page close,
     * so there is exactly one place that decides.
     */
    update(nextMedia, nextHandlers, visible) {
      ensureMounted();
      media = Array.isArray(nextMedia) ? nextMedia : [];
      if (nextHandlers) handlers = nextHandlers;
      anchorEl = null;
      if (!freePos) {
        // Anchor to the element playing the first item we can place.
        for (const item of media) {
          const el = pickElement(item.url);
          if (el) {
            anchorEl = el;
            break;
          }
        }
      }
      render();
      host.style.display = visible ? "" : "none";
      if (visible) place();
      return media.length;
    },
    setVisible,
    /** Exposed for the unit tests; the DOM walk is the fiddly part. */
    pickElement,
    destroy() {
      if (idleTimer) clearTimeout(idleTimer);
      if (followTimer) clearInterval(followTimer);
      followTimer = null;
      host?.remove();
      host = null;
      shadow = null;
      panel = null;
      listEl = null;
      countEl = null;
      caretEl = null;
      anchorEl = null;
      freePos = null;
    },
  };
})();
