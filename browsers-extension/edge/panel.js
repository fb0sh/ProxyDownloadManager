// =============================================================================
// panel.js — the on-page media panel (ProxyDM)
//
// Content scripts cannot use ES modules, so this is a plain script that
// publishes one namespaced global for content.js to drive. The UI lives in a
// shadow root: page CSS cannot reach it and it cannot leak out, which is what
// lets us keep the panel's own theme instead of fighting `all: revert` rules
// the way the NeatDownloadManager extension does.
//
// Strings come from chrome.i18n (a content script has no module to import
// i18n.js from); the palette is the app's, written as hex because the manifest
// still allows Chrome 88 and oklch needs 111.
// =============================================================================

(() => {
  const HOST_ID = "proxydm-media-panel";
  const IDLE_FADE_MS = 15000;
  const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB"];

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

  const STYLE = `
    :host { all: initial; }
    .panel, .panel * { box-sizing: border-box; }
    .panel {
      --bg:#ffffff; --fg:#0a0a0a; --muted:#f5f5f5; --muted-fg:#737373;
      --border:#e5e5e5; --primary:#171717; --primary-fg:#fafafa;
      --ok:#009689; --bad:#e7000b;
      position: fixed; right: 16px; bottom: 16px; width: 272px; z-index: 2147483000;
      background: var(--bg); color: var(--fg);
      border: 1px solid var(--border); border-radius: 12px; overflow: hidden;
      box-shadow: 0 12px 32px rgba(0,0,0,.22), 0 2px 6px rgba(0,0,0,.10);
      font: 13px/1.4 ui-sans-serif, system-ui, -apple-system, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
      transition: opacity .2s ease;
    }
    .panel.idle { opacity: .45; }
    .panel:hover { opacity: 1; }
    @media (prefers-color-scheme: dark) {
      .panel {
        --bg:#0a0a0a; --fg:#fafafa; --muted:#1c1c1c; --muted-fg:#a1a1a1;
        --border:#2a2a2a; --primary:#fafafa; --primary-fg:#171717;
        --ok:#00d3bb; --bad:#ff6467;
        box-shadow: 0 12px 32px rgba(0,0,0,.55), 0 2px 6px rgba(0,0,0,.4);
      }
    }
    .head {
      display: flex; align-items: center; gap: 7px; height: 34px; padding: 0 8px;
      background: var(--muted); border-bottom: 1px solid var(--border);
    }
    .grip { color: var(--muted-fg); font-size: 13px; letter-spacing: 1px; cursor: grab; user-select: none; }
    .grip:active { cursor: grabbing; }
    .dot { width: 11px; height: 11px; border-radius: 99px; background: var(--primary); flex: 0 0 11px; }
    .title { font-size: 12px; font-weight: 650; letter-spacing: .2px; }
    .pill {
      font-size: 10.5px; color: var(--muted-fg); background: var(--bg);
      border: 1px solid var(--border); border-radius: 99px; padding: 1px 6px;
    }
    .spacer { margin-left: auto; }
    button { font: inherit; border: 0; background: none; color: inherit; cursor: pointer; }
    .icon {
      width: 22px; height: 22px; border-radius: 7px; border: 1px solid var(--border);
      background: var(--bg); display: flex; align-items: center; justify-content: center;
      font-size: 12px; line-height: 1;
    }
    .icon:hover { background: var(--muted); }
    .list { list-style: none; margin: 0; padding: 0; max-height: 216px; overflow-y: auto; }
    .row {
      display: flex; align-items: center; gap: 8px; padding: 7px 8px;
      border-top: 1px solid var(--border); cursor: pointer;
    }
    .list li:first-child .row, .row:first-child { border-top: 0; }
    .row:hover { background: var(--muted); }
    .kind { flex: 0 0 38px; font-size: 9.5px; font-weight: 700; letter-spacing: .4px; color: var(--muted-fg); }
    .name { flex: 1; min-width: 0; }
    .name b { display: block; font-size: 12px; font-weight: 550; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .name span { font-size: 10.5px; color: var(--muted-fg); }
    .go {
      width: 24px; height: 24px; flex: 0 0 24px; border-radius: 7px; border: 1px solid var(--border);
      background: var(--bg); display: flex; align-items: center; justify-content: center;
    }
    .go:hover { background: var(--primary); color: var(--primary-fg); border-color: var(--primary); }
    .row.sent .go { border-color: var(--ok); color: var(--ok); }
    .row.sent .status { color: var(--ok); }
    .row.failed .go { border-color: var(--bad); color: var(--bad); }
    .row.failed .status { color: var(--bad); }
    .status { font-size: 10.5px; color: var(--muted-fg); }
    .foot {
      display: flex; align-items: center; gap: 8px; height: 28px; padding: 0 8px;
      background: var(--muted); border-top: 1px solid var(--border);
    }
    .ghost { font-size: 11px; color: var(--muted-fg); text-decoration: underline; padding: 0; }
    .ghost:hover { color: var(--fg); }
    .hint { margin-left: auto; font-size: 10.5px; color: var(--muted-fg); }
    .panel.collapsed .list, .panel.collapsed .foot { display: none; }
  `;

  let host = null;
  let shadow = null;
  let panel = null;
  let listEl = null;
  let countEl = null;
  let idleTimer = null;
  let media = [];
  let handlers = {};

  function ensureMounted() {
    if (host?.isConnected) return;
    host = document.createElement("div");
    host.id = HOST_ID;
    shadow = host.attachShadow({ mode: "open" });
    shadow.innerHTML = `
      <style>${STYLE}</style>
      <div class="panel" role="dialog" aria-label="ProxyDM">
        <div class="head">
          <span class="grip" data-act="drag" title="${t("panelDrag", "Drag")}">⠿</span>
          <span class="dot"></span>
          <span class="title">ProxyDM</span>
          <span class="pill">0</span>
          <button class="icon spacer" data-act="collapse" title="${t("panelCollapse", "Collapse")}">–</button>
          <button class="icon" data-act="close" title="${t("panelClose", "Close")}">✕</button>
        </div>
        <ul class="list"></ul>
        <div class="foot">
          <button class="ghost" data-act="hide-site">${t("panelHideSite", "Don't show on this site")}</button>
          <span class="hint">${t("panelHint", "Drag ⠿ to move")}</span>
        </div>
      </div>`;
    panel = shadow.querySelector(".panel");
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
      if (row && !target.closest(".ghost")) {
        event.preventDefault();
        event.stopPropagation();
        const index = Number(row.dataset.index);
        request(index, row);
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
      } else if (act === "hide-site") {
        handlers.onHideSite?.();
      }
    });

    // Clicks inside the panel must not reach the page's own handlers.
    shadow.addEventListener("mousedown", (event) => event.stopPropagation());
    shadow.addEventListener("mouseup", (event) => event.stopPropagation());
    shadow.addEventListener("mouseenter", () => {
      if (idleTimer) clearTimeout(idleTimer);
      panel.classList.remove("idle");
    });
    shadow.addEventListener("mouseleave", refreshIdleFade);

    // Dragging: only from the grip, so a row click stays a download.
    let drag = null;
    shadow.addEventListener("mousedown", (event) => {
      if (event.target.closest?.('[data-act="drag"]') == null) return;
      event.preventDefault();
      const rect = panel.getBoundingClientRect();
      drag = { dx: event.clientX - rect.left, dy: event.clientY - rect.top };
      panel.style.right = "auto";
      panel.style.bottom = "auto";
      panel.style.left = `${rect.left}px`;
      panel.style.top = `${rect.top}px`;
      idleTimer && clearTimeout(idleTimer);
      panel.classList.remove("idle");
    });
    window.addEventListener("mousemove", (event) => {
      if (!drag) return;
      const left = Math.max(0, Math.min(window.innerWidth - panel.offsetWidth, event.clientX - drag.dx));
      const top = Math.max(0, Math.min(window.innerHeight - panel.offsetHeight, event.clientY - drag.dy));
      panel.style.left = `${left}px`;
      panel.style.top = `${top}px`;
    });
    window.addEventListener("mouseup", () => {
      if (!drag) return;
      drag = null;
      refreshIdleFade();
    });
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
          <span class="name"><b></b><span class="status"></span></span>
          <span class="go" aria-hidden="true">↓</span>`;
        row.querySelector(".kind").textContent = kindOf(item);
        row.querySelector(".name b").textContent = nameOf(item);
        row.querySelector(".name b").title = item.url || "";
        const status = row.querySelector(".status");
        status.textContent = size ? `${size} · ${t("panelDownload", "Download")}` : t("panelDownload", "Download");
        const li = document.createElement("li");
        li.appendChild(row);
        return li;
      }),
    );
  }

  async function request(index, row) {
    const item = media[index];
    if (!item) return;
    const status = row.querySelector(".status");
    const wasSent = row.classList.contains("sent");
    try {
      const ok = await handlers.onDownload?.(item);
      row.classList.toggle("sent", !!ok);
      row.classList.toggle("failed", !ok);
      status.textContent = ok
        ? t("panelSent", "Sent to ProxyDM")
        : t("panelFailed", "ProxyDM did not accept it");
    } catch {
      row.classList.add("failed");
      status.textContent = t("panelFailed", "ProxyDM did not accept it");
    }
    if (wasSent && !row.classList.contains("sent")) status.textContent = t("panelDownload", "Download");
  }

  function setVisible(visible) {
    // Never mount just to hide: a page the panel does not belong on keeps no
    // shadow host at all.
    if (!host) return;
    host.style.display = visible ? "" : "none";
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
      render();
      host.style.display = visible ? "" : "none";
      return media.length;
    },
    setVisible,
    destroy() {
      if (idleTimer) clearTimeout(idleTimer);
      host?.remove();
      host = null;
      shadow = null;
      panel = null;
      listEl = null;
      countEl = null;
    },
  };
})();
