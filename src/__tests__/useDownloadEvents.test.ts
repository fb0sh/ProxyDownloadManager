import { describe, it, expect } from "vitest";
import { patchDownloadProgress } from "../hooks/useDownloadEvents";
import type { DownloadItem } from "../types";

function makeItem(id: number, downloaded: number): DownloadItem {
  return {
    id,
    url: `https://example.com/file${id}.zip`,
    file_name: `file${id}.zip`,
    save_path: `/tmp/file${id}.zip`,
    total_size: 1000,
    downloaded,
    status: "downloading",
    parts: [],
    proxy_name: "",
    connections: 4,
    resumable: true,
    created_at: "1234567890",
    last_try: "",
  };
}

describe("patchDownloadProgress", () => {
  it("updates downloaded bytes for matching id", () => {
    const cache = [makeItem(1, 0), makeItem(2, 0)];
    const result = patchDownloadProgress(cache, 1, 500);
    expect(result?.[0].downloaded).toBe(500);
    expect(result?.[1].downloaded).toBe(0); // unchanged
  });

  it("returns undefined when cache is undefined", () => {
    expect(patchDownloadProgress(undefined, 1, 500)).toBeUndefined();
  });

  it("returns cache unchanged when id not found", () => {
    const cache = [makeItem(1, 0)];
    const result = patchDownloadProgress(cache, 999, 500);
    expect(result?.[0].downloaded).toBe(0);
  });

  it("preserves other fields when updating progress", () => {
    const cache = [makeItem(1, 100)];
    const result = patchDownloadProgress(cache, 1, 200);
    expect(result?.[0].id).toBe(1);
    expect(result?.[0].url).toBe("https://example.com/file1.zip");
    expect(result?.[0].file_name).toBe("file1.zip");
    expect(result?.[0].downloaded).toBe(200);
  });

  it("handles empty cache", () => {
    expect(patchDownloadProgress([], 1, 500)).toEqual([]);
  });

  it("updates part downloaded when provided", () => {
    const item = makeItem(1, 0);
    item.parts = [
      { index: 0, start: 0, end: 500, downloaded: 0, temp_path: "", status: "pending", retries: 0 },
      { index: 1, start: 500, end: 1000, downloaded: 0, temp_path: "", status: "pending", retries: 0 },
    ];
    const result = patchDownloadProgress([item], 1, 600, [500, 100]);
    expect(result?.[0].parts[0]!.downloaded).toBe(500);
    expect(result?.[0].parts[1]!.downloaded).toBe(100);
  });
});
