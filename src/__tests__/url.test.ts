import { describe, it, expect } from "vitest";
import { looksLikeDownloadUrl, extractFilename, applyFilter } from "../utils/url";
import type { DownloadItem } from "../types";

function makeItem(overrides: Partial<DownloadItem> = {}): DownloadItem {
  return {
    id: 1,
    url: "https://example.com/file.zip",
    file_name: "file.zip",
    save_path: "/tmp/file.zip",
    total_size: 1000,
    downloaded: 0,
    status: "queued",
    parts: [],
    proxy_name: "",
    connections: 4,
    resumable: null,
    created_at: "1234567890",
    last_try: "",
    ...overrides,
  };
}

describe("looksLikeDownloadUrl", () => {
  it("returns true for URLs with download extensions", () => {
    expect(looksLikeDownloadUrl("https://example.com/file.zip")).toBe(true);
    expect(looksLikeDownloadUrl("https://example.com/file.tar.gz")).toBe(true);
    expect(looksLikeDownloadUrl("https://example.com/file.pdf")).toBe(true);
    expect(looksLikeDownloadUrl("https://example.com/file.exe")).toBe(true);
  });

  it("returns false for non-download URLs", () => {
    expect(looksLikeDownloadUrl("https://example.com/page")).toBe(false);
    expect(looksLikeDownloadUrl("https://example.com/file.txt")).toBe(false);
    expect(looksLikeDownloadUrl("https://example.com/")).toBe(false);
  });

  it("returns false for invalid URLs", () => {
    expect(looksLikeDownloadUrl("not a url")).toBe(false);
    expect(looksLikeDownloadUrl("")).toBe(false);
  });
});

describe("extractFilename", () => {
  it("extracts filename from URL path", () => {
    expect(extractFilename("https://example.com/file.zip")).toBe("file.zip");
    expect(extractFilename("https://example.com/path/to/file.tar.gz")).toBe("file.tar.gz");
  });

  it("handles URL-encoded filenames", () => {
    expect(extractFilename("https://example.com/my%20file.zip")).toBe("my file.zip");
  });

  it("extracts from filename= query param", () => {
    const url = "https://example.com/download?filename=report.pdf&other=1";
    expect(extractFilename(url)).toBe("report.pdf");
  });

  it("falls back to URL pattern matching when no path segments", () => {
    // Strategy 3 scans for name.extension patterns in the full URL
    expect(extractFilename("https://example.com/")).toBe("example.com");
  });

  it("returns empty string for invalid URLs", () => {
    expect(extractFilename("not a url")).toBe("");
  });
});

describe("applyFilter", () => {
  const items = [
    makeItem({ id: 1, status: "completed" }),
    makeItem({ id: 2, status: "downloading" }),
    makeItem({ id: 3, status: "paused" }),
    makeItem({ id: 4, status: "queued" }),
  ];

  it("returns all items for 'all' filter", () => {
    expect(applyFilter(items, "all")).toHaveLength(4);
  });

  it("returns only completed items", () => {
    const result = applyFilter(items, "completed");
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe(1);
  });

  it("returns incomplete items (downloading, paused, queued)", () => {
    const result = applyFilter(items, "incomplete");
    expect(result).toHaveLength(3);
    expect(result.map((d) => d.id)).toEqual([2, 3, 4]);
  });
});
