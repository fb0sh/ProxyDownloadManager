import { describe, it, expect } from "vitest";
import {
  filterHeaders,
  parseAck,
  buildDownloadRequest,
  shouldSkipMediaUrl,
  mediaDedupKey,
  mediaHeaders,
  interceptDecision,
  ignoreReason,
} from "../../browsers-extension/shared/protocol.js";

describe("extension protocol", () => {
  it("keeps download headers and drops hop-by-hop / sec-", () => {
    const out = filterHeaders({
      Cookie: "sid=1",
      Referer: "https://example.com/",
      Host: "cdn.example",
      Connection: "keep-alive",
      "Sec-Fetch-Mode": "navigate",
      Authorization: "Bearer x",
    });
    const headers = out as Record<string, string>;
    expect(headers.Cookie).toBe("sid=1");
    expect(headers.Referer).toBe("https://example.com/");
    expect(headers.Authorization).toBe("Bearer x");
    expect(headers.Host).toBeUndefined();
    expect(headers["Sec-Fetch-Mode"]).toBeUndefined();
  });

  it("accepts structured ACK", () => {
    const ack = parseAck(JSON.stringify({ protocol_version: 1, request_id: "abc", accepted: true, reason: "" }));
    expect(ack.accepted).toBe(true);
    expect(ack.requestId).toBe("abc");
  });

  it("rejects failed ACK and non-json", () => {
    expect(parseAck(JSON.stringify({ request_id: "x", accepted: false, reason: "offline" })).accepted).toBe(false);
    expect(parseAck("not-json").accepted).toBe(false);
  });

  it("legacy {status:ok} still counts as success", () => {
    expect(parseAck('{"status":"ok"}').accepted).toBe(true);
  });

  it("buildDownloadRequest assigns request_id and merges cookies", () => {
    const req = buildDownloadRequest({
      url: "https://cdn.example/a.zip",
      cookies: "a=b",
      referrer: "https://example.com/p",
    });
    expect(req.url).toBe("https://cdn.example/a.zip");
    expect(req.request_id.length).toBeGreaterThan(4);
    const hdrs = req.headers as Record<string, string>;
    expect(hdrs.Cookie).toBe("a=b");
    expect(hdrs.Referer).toBe("https://example.com/p");
  });

  it("skips blob/data/ts segments", () => {
    expect(shouldSkipMediaUrl("blob:https://x/1")).toBe(true);
    expect(shouldSkipMediaUrl("data:video/mp4,xxx")).toBe(true);
    expect(shouldSkipMediaUrl("https://cdn.example/seg12.ts")).toBe(true);
    expect(shouldSkipMediaUrl("https://cdn.example/master.m3u8")).toBe(false);
  });

  const base = {
    enabled: true,
    connected: true,
    url: "https://cdn.example/notes.txt",
    filename: "notes.txt",
    fileSize: 7000,
    settings: { minSize: 0, ignoredDomains: "", ignoredExtensions: "ico,svg" },
    bypass: false,
    restored: false,
  };

  it("takes a small file when ProxyDM is connected", () => {
    expect(interceptDecision(base)).toEqual({ take: true, reason: "take" });
  });

  it("leaves the browser download alone when ProxyDM is offline or disabled", () => {
    expect(interceptDecision({ ...base, connected: false }).reason).toBe("offline");
    expect(interceptDecision({ ...base, enabled: false }).reason).toBe("disabled");
  });

  it("skips blob, data, bypass, ignored extension, and ignored domain", () => {
    expect(interceptDecision({ ...base, url: "blob:https://x/1" }).reason).toBe("blob");
    expect(interceptDecision({ ...base, url: "data:text/plain,hi" }).reason).toBe("data");
    expect(interceptDecision({ ...base, bypass: true }).reason).toBe("bypass");
    expect(interceptDecision({ ...base, filename: "favicon.ico", url: "https://cdn.example/favicon.ico" }).reason).toBe("ignored-extension");
    expect(interceptDecision({
      ...base,
      url: "https://skip.example/a.txt",
      settings: { ...base.settings, ignoredDomains: "skip.example" },
    }).reason).toBe("ignored-domain");
  });

  it("applies min size only when the browser already knows a smaller size", () => {
    const settings = { ...base.settings, minSize: 1024 };
    expect(ignoreReason(base.url, "notes.txt", 100, settings)).toBe("min-size");
    expect(ignoreReason(base.url, "notes.txt", -1, settings)).toBe("");
    expect(interceptDecision({ ...base, fileSize: 7000, settings }).reason).toBe("take");
  });

  it("dedupes by origin+path", () => {
    expect(mediaDedupKey("https://a/x.mp4?t=1", "video/mp4")).toBe(mediaDedupKey("https://a/x.mp4?t=2", "video/mp4"));
  });
});

describe("media request headers", () => {
  const page = "https://video.example.com/watch/1234#t=42";
  const cdn = "https://cdn.other.example/seg/1.mp4?token=abc";

  it("derives Origin from the page and Referer from the whole page URL", () => {
    const headers = mediaHeaders({ url: cdn, pageUrl: page, cookies: "sid=1", userAgent: "UA" });
    expect(headers.Origin).toBe("https://video.example.com");
    // The fragment is dropped; the path is not — a hotlink check compares it.
    expect(headers.Referer).toBe("https://video.example.com/watch/1234");
    expect(headers.Cookie).toBe("sid=1");
    expect(headers["User-Agent"]).toBe("UA");
  });

  it("keeps what the browser actually sent over anything derived", () => {
    const headers = mediaHeaders({
      captured: {
        Origin: "https://player.example",
        Referer: "https://player.example/embed/x",
        Accept: "video/webm,*/*",
        Authorization: "Bearer t",
      },
      url: cdn,
      pageUrl: page,
      cookies: "sid=1",
      userAgent: "UA",
    });
    expect(headers.Origin).toBe("https://player.example");
    expect(headers.Referer).toBe("https://player.example/embed/x");
    expect(headers.Accept).toBe("video/webm,*/*");
    expect(headers.Authorization).toBe("Bearer t");
  });

  it("prefers the fresh cookie jar but keeps a captured cookie when it is empty", () => {
    expect(mediaHeaders({ captured: { Cookie: "old=1" }, url: cdn, cookies: "new=2" }).Cookie).toBe("new=2");
    expect(mediaHeaders({ captured: { Cookie: "captured=1" }, url: cdn, cookies: "" }).Cookie).toBe("captured=1");
  });

  it("falls back through the frame, the referer, then the media url for Origin", () => {
    expect(mediaHeaders({ url: cdn, pageUrl: "", captured: { Referer: "https://a.example/p" } }).Origin).toBe(
      "https://a.example",
    );
    expect(mediaHeaders({ url: cdn, pageUrl: "" }).Origin).toBe("https://cdn.other.example");
    expect(mediaHeaders({ url: cdn, pageUrl: "" }).Referer).toBeUndefined();
  });

  it("never replays hop-by-hop, sec-* or sniffed x-* headers", () => {
    const headers = mediaHeaders({
      captured: {
        Host: "cdn.other.example",
        Connection: "keep-alive",
        "Sec-Fetch-Mode": "cors",
        "Content-Length": "10",
        "X-Playback-Session-Id": "deadbeef",
        Accept: "*/*",
      },
      url: cdn,
      pageUrl: page,
    });
    expect(headers.Accept).toBe("*/*");
    for (const name of ["Host", "Connection", "Sec-Fetch-Mode", "Content-Length", "X-Playback-Session-Id"]) {
      expect(headers[name]).toBeUndefined();
    }
  });
});

