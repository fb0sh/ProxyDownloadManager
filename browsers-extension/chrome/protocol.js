export const PROTOCOL_VERSION = 1;

/**
 * What must never be replayed: hop-by-hop and framing headers would corrupt the
 * request, pseudo-headers are HTTP/2 internals, and Range / If-Range belong to
 * the engine. Everything else the browser sent is kept — a media CDN can key on
 * any of it, and `x-*` playback session ids in particular are how an origin
 * tells a legitimate player from a stranger.
 */
function blocked(name) {
  const n = String(name || "").toLowerCase();
  return (
    n === "host" ||
    n === "connection" ||
    n === "keep-alive" ||
    n === "proxy-connection" ||
    n === "proxy-authenticate" ||
    n === "proxy-authorization" ||
    n === "te" ||
    n === "trailer" ||
    n === "transfer-encoding" ||
    n === "upgrade" ||
    n === "content-length" ||
    n === "content-encoding" ||
    n === "expect" ||
    n === "range" ||
    n === "if-range" ||
    n.startsWith(":")
  );
}

function canonical(name) {
  const n = String(name || "").toLowerCase();
  const map = {
    cookie: "Cookie",
    referer: "Referer",
    origin: "Origin",
    "user-agent": "User-Agent",
    authorization: "Authorization",
    accept: "Accept",
    "accept-language": "Accept-Language",
    "accept-encoding": "Accept-Encoding",
  };
  return map[n] || name;
}

export function filterHeaders(input) {
  const out = {};
  if (!input) return out;
  const entries = Array.isArray(input)
    ? input.map((h) => [h.name, h.value])
    : Object.entries(input);
  for (const [k, v] of entries) {
    if (!k || v == null || v === "") continue;
    if (blocked(k)) continue;
    out[canonical(k)] = String(v);
  }
  return out;
}

export function buildDownloadRequest(partial) {
  const headers = filterHeaders(partial.headers || {});
  if (partial.cookies && !headers.Cookie) headers.Cookie = partial.cookies;
  if (partial.referrer && !headers.Referer) headers.Referer = partial.referrer;
  if (partial.userAgent && !headers["User-Agent"]) headers["User-Agent"] = partial.userAgent;
  return {
    protocol_version: PROTOCOL_VERSION,
    request_id: partial.requestId || crypto.randomUUID(),
    action: partial.action || "add",
    url: partial.url || "",
    final_url: partial.finalUrl || partial.url || "",
    filename: partial.filename || "",
    method: partial.method || "GET",
    referrer: partial.referrer || "",
    user_agent: partial.userAgent || "",
    cookies: partial.cookies || headers.Cookie || "",
    headers,
    tab_url: partial.tabUrl || "",
    content_type: partial.contentType || "",
    content_length: Number(partial.contentLength || 0) || 0,
    proxy_name: partial.proxyName || "",
    connections: partial.connections ?? 0,
  };
}

export function parseAck(data) {
  if (data == null) return { ok: false, accepted: false, reason: "empty ack" };
  let parsed = data;
  if (typeof data === "string") {
    try {
      parsed = JSON.parse(data);
    } catch {
      return { ok: false, accepted: false, reason: "non-json ack" };
    }
  }
  if (typeof parsed !== "object") return { ok: false, accepted: false, reason: "invalid ack" };
  if (parsed.accepted === true) return { ok: true, accepted: true, reason: "", requestId: parsed.request_id || "" };
  if (parsed.status === "ok" && parsed.accepted !== false) {
    return { ok: true, accepted: true, reason: "legacy", requestId: parsed.request_id || "" };
  }
  return {
    ok: false,
    accepted: false,
    reason: parsed.reason || "rejected",
    requestId: parsed.request_id || "",
  };
}

export function mediaDedupKey(url, contentType) {
  try {
    const u = new URL(url);
    u.hash = "";
    return `${u.origin}${u.pathname}|${contentType || ""}`;
  } catch {
    return `${url}|${contentType || ""}`;
  }
}

/**
 * Extensions a media file (or a manifest of one) is served under. The response
 * content type alone is not enough: Bilibili's DASH tracks (`…-1-30232.m4s`,
 * a whole 60 MB video or audio track fetched with Range requests) come back as
 * `application/octet-stream`, so a type-only test silently misses every one of
 * them, which reads as "sniffing stopped working".
 */
const MEDIA_EXTENSION = /\.(m4s|m4v|m4a|mp4|webm|mkv|flv|mov|aac|flac|mp3|opus|ogg|oga|wav|mpd|m3u8)(\?|$)/i;

/**
 * Is this response a media file worth offering?
 *
 * A media content type or manifest type is taken at face value. Otherwise a
 * media-looking extension counts, but only when the response is binary or
 * untyped — an error page served for a `.mp4` path is HTML, not a video.
 *
 * @param {{url?: string, contentType?: string}} response
 * @returns {boolean}
 */
export function isMediaResponse(response) {
  // Parameters (`; charset=…`) are not part of the type.
  const type = String(response?.contentType || "").toLowerCase().split(";")[0].trim();
  const url = String(response?.url || "");
  if (type.startsWith("video/") || type.startsWith("audio/")) return true;
  if (type === "application/vnd.apple.mpegurl" || type === "application/x-mpegurl") return true;
  if (type === "application/dash+xml") return true;
  if (!MEDIA_EXTENSION.test(url)) return false;
  return (
    type === "" ||
    type === "application/octet-stream" ||
    type === "binary/octet-stream" ||
    type === "application/binary"
  );
}

export function shouldSkipMediaUrl(url) {
  if (!url) return true;
  if (url.startsWith("blob:") || url.startsWith("data:")) return true;
  const lower = url.toLowerCase();
  if (/\.ts(\?|$)/.test(lower) && !lower.includes(".m3u8")) return true;
  return false;
}

/**
 * Origin of a URL (`scheme://host[:port]`), or "" when it cannot be parsed.
 * @param {string} url
 * @returns {string}
 */
export function originOf(url) {
  try {
    return new URL(url).origin;
  } catch {
    return "";
  }
}

/**
 * A URL fit to replay as Referer: the whole thing, minus its fragment.
 * @param {string} url
 * @returns {string}
 */
export function refererFor(url) {
  if (!url) return "";
  try {
    const u = new URL(url);
    u.hash = "";
    return u.href;
  } catch {
    return "";
  }
}

/**
 * The headers to hand the desktop for one media download.
 *
 * `captured` is what the browser itself sent for that request, straight from
 * `webRequest.onBeforeSendHeaders`. It wins wherever it exists: a site's own
 * Origin / Referer / Accept / Authorization is ground truth, and replaying
 * something else is how a sniffed download turns into a 403.
 *
 * What the browser did *not* send gets derived from the page: a plain
 * `<video>` load is not a CORS request, so there is no Origin to capture while
 * the CDN checking it still expects one, and `details.initiator` only ever
 * carries the origin — never the page path a hotlink check compares against.
 *
 * @param {{captured?: Record<string, string>, url?: string, pageUrl?: string, cookies?: string, userAgent?: string}} input
 * @returns {Record<string, string>}
 */
export function mediaHeaders({ captured, url, pageUrl, cookies, userAgent }) {
  const headers = { ...(captured || {}) };
  if (!headers.Origin) {
    const origin = originOf(pageUrl) || originOf(headers.Referer) || originOf(url);
    if (origin) headers.Origin = origin;
  }
  if (!headers.Referer) {
    const referer = refererFor(pageUrl);
    if (referer) headers.Referer = referer;
  }
  // The cookie jar is fresher than a header captured minutes ago.
  if (cookies) headers.Cookie = cookies;
  if (userAgent) headers["User-Agent"] = userAgent;
  return filterHeaders(headers);
}

function extensionOf(filename, url) {
  let path = filename || "";
  if (!path) {
    try {
      path = new URL(url).pathname;
    } catch {
      path = url || "";
    }
  }
  const ext = path.split(".").pop()?.toLowerCase() || "";
  if (!ext || /[\\/]/.test(ext)) return "";
  return ext;
}

/** Split a comma/space separated host list, or pass an array through. */
function hostList(patterns) {
  if (Array.isArray(patterns)) return patterns.map((p) => String(p).trim().toLowerCase()).filter(Boolean);
  return String(patterns || "")
    .split(/[,\s]+/)
    .map((p) => p.trim().toLowerCase())
    .filter(Boolean);
}

/**
 * Does a hostname match one of the patterns, exactly or as a subdomain?
 * `patterns` is either an array (the panel's hidden sites) or the comma
 * separated string the settings inputs hold.
 * @param {string} hostname
 * @param {string[] | string} patterns
 * @returns {boolean}
 */
export function hostMatches(hostname, patterns) {
  const host = String(hostname || "").toLowerCase();
  if (!host) return false;
  return hostList(patterns).some((p) => host === p || host.endsWith("." + p));
}

/**
 * Whether the on-page panel belongs on this page right now. An empty page has
 * nothing to show, so it only appears once something was sniffed.
 * @param {{sniff?: boolean, panel?: boolean, hiddenHosts?: string[] | string, hostname?: string, mediaCount?: number, closed?: boolean}} state
 * @returns {boolean}
 */
export function panelVisible({ sniff, panel, hiddenHosts, hostname, mediaCount, closed }) {
  if (panel === false || sniff === false || closed === true) return false;
  if (hostMatches(hostname, hiddenHosts)) return false;
  return Number(mediaCount || 0) > 0;
}

/**
 * Add a host to the hidden list: lower-cased, deduped, order preserved.
 * @param {string[]} [list]
 * @param {string} host
 * @returns {string[]}
 */
export function addHiddenHost(list, host) {
  const next = Array.isArray(list) ? list.slice() : [];
  const h = String(host || "").trim().toLowerCase();
  if (h && !next.map((x) => String(x).toLowerCase()).includes(h)) next.push(h);
  return next;
}

/**
 * Drop a host from the hidden list.
 * @param {string[]} [list]
 * @param {string} host
 * @returns {string[]}
 */
export function removeHiddenHost(list, host) {
  const h = String(host || "").trim().toLowerCase();
  return (Array.isArray(list) ? list : []).filter((x) => String(x).toLowerCase() !== h);
}

// Why a browser download must be left alone. Empty string means it can be taken.
export function ignoreReason(url, filename, fileSize, settings) {
  const cfg = settings || {};
  let hostname = "";
  try {
    hostname = new URL(url).hostname;
  } catch {
    return "";
  }
  if (hostMatches(hostname, cfg.ignoredDomains)) return "ignored-domain";
  const ext = extensionOf(filename, url);
  const ignoredExt = String(cfg.ignoredExtensions || "")
    .split(/[,\s]+/)
    .filter(Boolean)
    .map((s) => s.replace(/^\./, "").toLowerCase());
  if (ext && ignoredExt.includes(ext)) return "ignored-extension";
  const minSize = Number(cfg.minSize || 0);
  const size = Number(fileSize || 0);
  if (minSize > 0 && size > 0 && size < minSize) return "min-size";
  return "";
}

// Fast-path decision. Reads only the values the caller already has in memory.
export function interceptDecision(input) {
  const src = input || {};
  if (!src.enabled) return { take: false, reason: "disabled" };
  if (!src.connected) return { take: false, reason: "offline" };
  const url = src.url || "";
  if (!url) return { take: false, reason: "empty-url" };
  if (url.startsWith("blob:")) return { take: false, reason: "blob" };
  if (url.startsWith("data:")) return { take: false, reason: "data" };
  if (src.restored) return { take: false, reason: "restored" };
  if (src.bypass) return { take: false, reason: "bypass" };
  const ignored = ignoreReason(url, src.filename || "", src.fileSize, src.settings);
  if (ignored) return { take: false, reason: ignored };
  return { take: true, reason: "take" };
}
