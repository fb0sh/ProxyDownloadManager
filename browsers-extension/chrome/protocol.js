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
  const clean = normalizeMediaUrl(url);
  try {
    const u = new URL(clean);
    u.hash = "";
    return `${u.origin}${u.pathname}|${contentType || ""}`;
  } catch {
    return `${clean}|${contentType || ""}`;
  }
}

/**
 * Extract the full stream byte length from response headers.
 * HTML5 `<video>` and DASH/MSE players (`Range: bytes=0-985`) receive a tiny
 * `Content-Length` (e.g. 986 B) alongside `Content-Range: bytes 0-985/64829104`.
 * Preferring the `Content-Range` total shows the real 61.8 MB track size.
 *
 * @param {Record<string, string> | Array<{name?: string, value?: string}>} headers
 * @returns {{ totalSize: number, contentLength: number, hasContentRange: boolean, acceptRanges: string }}
 */
export function parseResponseSizing(headers) {
  const map = {};
  if (Array.isArray(headers)) {
    for (const h of headers) {
      if (h?.name && h.value != null) map[String(h.name).toLowerCase()] = String(h.value);
    }
  } else if (headers && typeof headers === "object") {
    for (const [k, v] of Object.entries(headers)) {
      if (k && v != null) map[k.toLowerCase()] = String(v);
    }
  }
  const contentLength = Math.max(0, Number(map["content-length"] || 0) || 0);
  const acceptRanges = String(map["accept-ranges"] || "").trim().toLowerCase();
  const cr = String(map["content-range"] || "").trim();
  const m = /^bytes\s+(?:\d+-\d+|\*)\/(\d+)\s*$/i.exec(cr);
  if (m) {
    const total = Number(m[1]);
    if (Number.isFinite(total) && total > 0) {
      return { totalSize: total, contentLength, hasContentRange: true, acceptRanges };
    }
  }
  return {
    totalSize: contentLength,
    contentLength,
    hasContentRange: cr.toLowerCase().startsWith("bytes "),
    acceptRanges,
  };
}

/**
 * Extract RFC 6266 / RFC 5987 filename from a `Content-Disposition` header.
 * `filename*=` wins over `filename=`.
 * @param {string} header
 * @returns {string}
 */
export function parseContentDispositionFilename(header) {
  if (!header) return "";
  let extName = "";
  let plainName = "";
  for (const rawPart of String(header).split(";")) {
    const eq = rawPart.indexOf("=");
    if (eq <= 0) continue;
    const key = rawPart.slice(0, eq).trim().toLowerCase();
    let val = rawPart.slice(eq + 1).trim();
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
      val = val.slice(1, -1).trim();
    }
    if (!val) continue;
    if (key === "filename*") {
      const rfc = /^([^']*)'[^']*'(.*)$/.exec(val);
      const encoded = rfc ? rfc[2] : val;
      try {
        extName = decodeURIComponent(encoded);
      } catch {
        extName = encoded;
      }
    } else if (key === "filename" && !plainName) {
      try {
        plainName = decodeURIComponent(val);
      } catch {
        plainName = val;
      }
    }
  }
  return extName || plainName;
}

/**
 * Strip MSE player byte-slice query params (`range=0-999`, `bytestart`+`byteend`)
 * ONLY when they are strictly numeric and NOT listed in signed-parameter lists
 * (`sparams`, `X-Amz-SignedHeaders`, `SignedHeaders`). Never touches `part` or IDs.
 *
 * @param {string} url
 * @returns {string}
 */
export function normalizeMediaUrl(url) {
  if (!url) return "";
  try {
    const u = new URL(url);
    const params = u.searchParams;
    const signedRaw = [
      params.get("sparams"),
      params.get("X-Amz-SignedHeaders"),
      params.get("SignedHeaders"),
    ]
      .filter(Boolean)
      .join(",");
    const signed = new Set(
      signedRaw
        .split(/[,;]+/)
        .map((s) => s.trim().toLowerCase())
        .filter(Boolean),
    );
    let changed = false;
    const rangeVal = params.get("range");
    if (rangeVal != null && /^\d+-\d*$/.test(rangeVal) && !signed.has("range")) {
      params.delete("range");
      changed = true;
    }
    const bs = params.get("bytestart");
    const be = params.get("byteend");
    if (
      bs != null &&
      be != null &&
      /^\d+$/.test(bs) &&
      /^\d+$/.test(be) &&
      !signed.has("bytestart") &&
      !signed.has("byteend")
    ) {
      params.delete("bytestart");
      params.delete("byteend");
      changed = true;
    }
    return changed ? u.toString() : url;
  } catch {
    return url;
  }
}

/**
 * Clean a document/tab title into a human filename stem (IDM-style): drops
 * leading `(3)` unread badges, play glyphs, and common site suffixes.
 * @param {string} rawTitle
 * @returns {string}
 */
export function cleanPageTitle(rawTitle) {
  if (!rawTitle) return "";
  let s = String(rawTitle)
    .replace(/^\s*[(\uff08]\d+[)\uff09]\s*/, "")
    .replace(/[\u25B6\u23F5\r\n\t]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  s = s.replace(
    /\s*(?:[-_|—·]+\s*)(?:YouTube|哔哩哔哩_bilibili|哔哩哔哩|bilibili|Vimeo|Twitch|TikTok|抖音|西瓜视频)\s*$/i,
    "",
  );
  s = s.replace(/[\\/:*?"<>|]+/g, "_").replace(/[. ]+$/g, "").trim();
  if (s.length > 96) s = s.slice(0, 96).trim();
  return s;
}

const CORE_MEDIA_PARAMS = ["v", "bvid", "aid", "p", "ep_id", "season_id", "id", "vid", "clip"];

/**
 * Whether a `tabs.onUpdated` URL change represents a genuinely different media
 * page (true) vs a cosmetic SPA `history.replaceState` tracking-param cleanup
 * like Bilibili stripping `spm_id_from` 1s after playback starts (false).
 *
 * @param {string} prevUrl
 * @param {string} nextUrl
 * @returns {boolean}
 */
export function navigationClearsMedia(prevUrl, nextUrl) {
  if (!prevUrl || !nextUrl) return false;
  if (prevUrl === nextUrl) return false;
  try {
    const a = new URL(prevUrl);
    const b = new URL(nextUrl);
    if (a.origin !== b.origin || a.pathname !== b.pathname) return true;
    for (const key of CORE_MEDIA_PARAMS) {
      if ((a.searchParams.get(key) || "") !== (b.searchParams.get(key) || "")) {
        return true;
      }
    }
    return false;
  } catch {
    return prevUrl !== nextUrl;
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

/**
 * Container-semantic segment noise filter.
 * - `.ts` / `video/mp2t` / `audio/mp2t` and `.f4f` HDS fragments are individual
 *   streaming chunks, not standalone files.
 * - `.m4s` / `.cmfv` / `.cmfa` served with `200 OK` (no `206 Content-Range`) and
 *   small size (`< 4 MiB`) are ISO-BMFF `moof+mdat` fragments without `ftyp/moov`
 *   headers and cannot be played alone; `206 Partial Content` `.m4s` (or `>= 4 MiB`)
 *   are full On-Demand tracks (e.g. Bilibili DASH) and ARE kept.
 * - Standalone `200 OK` audio clips of any size (dictionary pronunciations, etc.)
 *   are preserved.
 *
 * @param {string} url
 * @param {{contentType?: string, statusCode?: number, size?: number, hasContentRange?: boolean}} [meta]
 * @returns {boolean}
 */
export function shouldSkipMediaUrl(url, meta) {
  if (!url) return true;
  if (url.startsWith("blob:") || url.startsWith("data:")) return true;
  const lower = url.toLowerCase();
  const isM3u8 = /\.m3u8(\?|$)/.test(lower);
  if (/\.ts(\?|$)/.test(lower) && !isM3u8) return true;
  if (/\.f4f(\?|$)/.test(lower) || /seg\d+-frag\d+/i.test(lower)) return true;

  if (meta && typeof meta === "object") {
    const type = String(meta.contentType || "").toLowerCase().split(";")[0].trim();
    if ((type === "video/mp2t" || type === "audio/mp2t") && !isM3u8) return true;

    const isIsoFragmentExt = /\.(m4s|cmfv|cmfa)(\?|$)/i.test(lower);
    if (isIsoFragmentExt) {
      const status = Number(meta.statusCode || 0);
      const size = Number(meta.size || 0);
      const ranged = status === 206 || meta.hasContentRange === true;
      if (!ranged && status === 200 && size > 0 && size < 4 * 1024 * 1024) {
        return true;
      }
    }
    // Tiny MP4 init header segments (`init.mp4`, `init-stream0.mp4`) in DASH/CMAF
    if (/\/init(?:[-_a-z0-9]*)?\.(?:mp4|m4s)(\?|$)/i.test(lower)) {
      const size = Number(meta.size || 0);
      if (size > 0 && size < 64 * 1024 && meta.statusCode === 200 && !meta.hasContentRange) {
        return true;
      }
    }
  }
  return false;
}

const BILI_AUDIO_IDS = new Set(["30216", "30232", "30280", "30250", "30251", "30255"]);
const BILI_VIDEO_QUALITY = {
  "30016": "360p",
  "30032": "480p",
  "30064": "720p",
  "30077": "1080p",
  "30080": "1080p",
  "30112": "1080p+",
  "30120": "4K",
  "30121": "4K",
  "30125": "HDR",
  "30127": "8K",
};

function detectResolutionHint(url, resolution) {
  if (resolution) {
    const m = /(\d{3,4})x(\d{3,4})/i.exec(String(resolution));
    if (m) {
      const h = Math.min(Number(m[1]), Number(m[2]));
      if (h >= 2160) return "4K";
      if (h >= 1440) return "1440p";
      if (h >= 1080) return "1080p";
      if (h >= 720) return "720p";
      if (h >= 480) return "480p";
      if (h >= 360) return "360p";
    }
    return String(resolution);
  }
  const biliMatch = /-1-(30\d{3})\.m4s(\?|$)/i.exec(url);
  if (biliMatch && BILI_VIDEO_QUALITY[biliMatch[1]]) {
    return BILI_VIDEO_QUALITY[biliMatch[1]];
  }
  const urlRes = /(?:^|[/_.-])(2160p|4k|1440p|1080p|720p|480p|360p)(?:[/_.-]|\?|$)/i.exec(url);
  if (urlRes) {
    return urlRes[1].toUpperCase() === "4K" ? "4K" : urlRes[1].toLowerCase();
  }
  return "";
}

/**
 * Classify a sniffed media item into human-readable kind/badge/filename so split
 * DASH tracks (e.g. Bilibili silent video `.m4s` vs audio `.m4s`) and HLS streams
 * never collide or appear as cryptic hashes.
 *
 * @param {{url?: string, contentType?: string, pageTitle?: string, filename?: string, resolution?: string}} item
 * @returns {{ kind: "HLS" | "DASH" | "AUDIO" | "VIDEO", badge: string, displayName: string, suggestedFilename: string }}
 */
export function describeMediaItem(item) {
  const url = String(item?.url || "");
  const type = String(item?.contentType || "").toLowerCase().split(";")[0].trim();
  const lower = url.toLowerCase();
  const isM3u8 = type.includes("mpegurl") || /\.m3u8(\?|$)/.test(lower);
  const isMpd = type === "application/dash+xml" || /\.mpd(\?|$)/.test(lower);
  const isM4s = /\.m4s(\?|$)/.test(lower);
  const biliCode = /-1-(\d+)\.m4s(\?|$)/i.exec(url)?.[1] || "";

  const isAudio =
    !isM3u8 &&
    !isMpd &&
    (type.startsWith("audio/") ||
      /\.(mp3|m4a|aac|flac|opus|ogg|oga|wav)(\?|$)/i.test(lower) ||
      (isM4s && BILI_AUDIO_IDS.has(biliCode)));

  const kind = isM3u8 ? "HLS" : isMpd ? "DASH" : isAudio ? "AUDIO" : "VIDEO";
  const quality = kind === "AUDIO" ? "" : detectResolutionHint(url, item?.resolution);
  const badge = quality ? `${kind} ${quality}` : kind;

  const cdName = String(item?.filename || "").trim();
  const titleStem = cleanPageTitle(item?.pageTitle || "");
  let rawLeaf = "";
  try {
    rawLeaf = decodeURIComponent(new URL(url).pathname.split("/").filter(Boolean).pop() || "");
  } catch {
    rawLeaf = "";
  }

  let ext = extensionOf(cdName, url);
  if (isM3u8) ext = "ts";
  else if (isM4s) ext = isAudio ? "m4a" : "mp4";
  else if (!ext) {
    if (isAudio) ext = type.includes("mpeg") ? "mp3" : "m4a";
    else if (type.includes("webm")) ext = "webm";
    else ext = "mp4";
  }

  let suggestedFilename = cdName;
  if (!suggestedFilename) {
    if (titleStem) {
      if (isM4s) {
        const tag = isAudio ? "Audio" : quality ? `Video ${quality}` : "Video";
        suggestedFilename = `${titleStem} [${tag}].${ext}`;
      } else if (isM3u8 && quality) {
        suggestedFilename = `${titleStem} [${quality}].${ext}`;
      } else {
        suggestedFilename = `${titleStem}.${ext}`;
      }
    } else if (rawLeaf) {
      if (/\.m3u8$/i.test(rawLeaf)) {
        suggestedFilename = rawLeaf.replace(/\.m3u8$/i, `.${ext}`);
      } else if (/\.m4s$/i.test(rawLeaf)) {
        suggestedFilename = rawLeaf.replace(/\.m4s$/i, `.${ext}`);
      } else {
        suggestedFilename = rawLeaf;
      }
    } else {
      suggestedFilename = `media.${ext}`;
    }
  }

  return {
    kind,
    badge,
    displayName: suggestedFilename,
    suggestedFilename,
  };
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
  if (!/^https?:/i.test(url)) return { take: false, reason: "unsupported-scheme" };
  if (src.method && String(src.method).toUpperCase() !== "GET") {
    return { take: false, reason: "non-get" };
  }
  if (src.restored) return { take: false, reason: "restored" };
  if (src.bypass) return { take: false, reason: "bypass" };
  if (src.knownDynamicStream === true) {
    return { take: false, reason: "dynamic-stream" };
  }
  const ignored = ignoreReason(url, src.filename || "", src.fileSize, src.settings);
  if (ignored) return { take: false, reason: ignored };
  return { take: true, reason: "take" };
}
