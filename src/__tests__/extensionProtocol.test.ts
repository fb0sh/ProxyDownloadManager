import { describe, it, expect } from "vitest";
import {
  filterHeaders,
  parseAck,
  buildDownloadRequest,
  shouldSkipMediaUrl,
  mediaDedupKey,
  mediaHeaders,
  hostMatches,
  panelVisible,
  addHiddenHost,
  removeHiddenHost,
  interceptDecision,
  ignoreReason,
} from "../../browsers-extension/shared/protocol.js";
import "../../browsers-extension/shared/panel.js";

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


describe("panel visibility", () => {
  const host = "video.example.com";

  it("needs media, the switch on, and no site hide", () => {
    expect(panelVisible({ sniff: true, panel: true, hostname: host, mediaCount: 1 })).toBe(true);
    // Nothing sniffed yet: no panel on an empty page.
    expect(panelVisible({ sniff: true, panel: true, hostname: host, mediaCount: 0 })).toBe(false);
    expect(panelVisible({ sniff: true, panel: false, hostname: host, mediaCount: 3 })).toBe(false);
    expect(panelVisible({ sniff: false, panel: true, hostname: host, mediaCount: 3 })).toBe(false);
    // Defaults are on: an older stored profile has neither key.
    expect(panelVisible({ hostname: host, mediaCount: 1 })).toBe(true);
  });

  it("honours a hidden site, subdomains included", () => {
    const hiddenHosts = ["example.com"];
    expect(panelVisible({ hostname: host, mediaCount: 1, hiddenHosts })).toBe(false);
    expect(panelVisible({ hostname: "example.com", mediaCount: 1, hiddenHosts })).toBe(false);
    expect(panelVisible({ hostname: "notexample.com", mediaCount: 1, hiddenHosts })).toBe(true);
    // The settings inputs hold a comma separated string, not an array.
    expect(panelVisible({ hostname: host, mediaCount: 1, hiddenHosts: "a.com, example.com" })).toBe(false);
  });

  it("matches hosts exactly or as a subdomain, never as a suffix of another label", () => {
    expect(hostMatches("a.example.com", "example.com")).toBe(true);
    expect(hostMatches("example.com", "example.com")).toBe(true);
    expect(hostMatches("badexample.com", "example.com")).toBe(false);
    expect(hostMatches("", "example.com")).toBe(false);
    expect(hostMatches("example.com", "")).toBe(false);
  });

  it("adds and removes hidden hosts without duplicates or case noise", () => {
    let list = addHiddenHost([], "Example.COM");
    expect(list).toEqual(["example.com"]);
    list = addHiddenHost(list, "example.com");
    list = addHiddenHost(list, "  example.com  ");
    expect(list).toEqual(["example.com"]);
    list = addHiddenHost(list, "other.test");
    expect(list).toEqual(["example.com", "other.test"]);
    expect(removeHiddenHost(list, "EXAMPLE.com")).toEqual(["other.test"]);
    expect(removeHiddenHost(undefined, "x")).toEqual([]);
  });
});

type PanelApi = { pickElement: (url: string, root?: Document | Element) => HTMLMediaElement | null };
const mediaPanel = (globalThis as unknown as { __proxydmPanel: PanelApi }).__proxydmPanel;

describe("media panel: which element to sit above", () => {
  function mount(html: string) {
    document.body.innerHTML = html;
    return Array.from(document.querySelectorAll("video, audio")) as HTMLMediaElement[];
  }
  function size(el: Element, width: number, height: number) {
    el.getBoundingClientRect = () => ({ width, height }) as DOMRect;
  }

  it("matches the element carrying the sniffed url, query string aside", () => {
    const [first, second] = mount(
      '<video src="https://cdn.example/a.mp4"></video><video src="https://cdn.example/b.mp4"></video>',
    );
    expect(mediaPanel.pickElement("https://cdn.example/b.mp4?token=abc", document)).toBe(second);
    expect(mediaPanel.pickElement("https://cdn.example/a.mp4", document)).toBe(first);
  });

  it("looks through <source> children", () => {
    const [video] = mount(
      '<video><source src="https://cdn.example/sd.mp4" type="video/mp4"></video>',
    );
    expect(mediaPanel.pickElement("https://cdn.example/sd.mp4", document)).toBe(video);
  });

  it("falls back to the largest visible player for a blob/MSE source", () => {
    const [small, large] = mount('<video src="blob:https://x/1"></video><video src="blob:https://x/2"></video>');
    size(small, 320, 180);
    size(large, 1280, 720);
    // Neither src can match: the sniffed URL is the manifest, not the element's.
    expect(mediaPanel.pickElement("https://cdn.example/master.m3u8", document)).toBe(large);
  });

  it("ignores a hidden player", () => {
    const [hidden, visible] = mount(
      '<video src="blob:https://x/1" style="display:none"></video><video src="blob:https://x/2"></video>',
    );
    size(hidden, 1920, 1080);
    size(visible, 640, 360);
    expect(mediaPanel.pickElement("https://cdn.example/master.m3u8", document)).toBe(visible);
  });

  it("anchors nothing when the page has no player", () => {
    mount('<div>no media here</div>');
    expect(mediaPanel.pickElement("https://cdn.example/a.mp4", document)).toBe(null);
  });
});
