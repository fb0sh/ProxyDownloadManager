import { describe, it, expect } from "vitest";
import {
  partPercent,
  applyPartDownloaded,
} from "../utils/progressMap";
import type { DownloadPart } from "../types";
import { patchDownloadProgress } from "../hooks/useDownloadEvents";
import type { DownloadItem } from "../types";

function part(index: number, start: number, end: number, downloaded = 0): DownloadPart {
  return {
    index,
    start,
    end,
    downloaded,
    temp_path: "",
    status: "pending",
    retries: 0,
  };
}

function makeItem(id: number, parts: DownloadPart[]): DownloadItem {
  return {
    id,
    url: "https://example.com/f.zip",
    file_name: "f.zip",
    save_path: "/tmp/f.zip",
    total_size: 300,
    downloaded: 0,
    status: "downloading",
    parts,
    proxy_name: "",
    connections: 4,
    resumable: true,
    created_at: "1",
    last_try: "",
  };
}

describe("partPercent", () => {
  it("computes 0–100 for a part range", () => {
    expect(partPercent(0, 0, 100)).toBe(0);
    expect(partPercent(50, 0, 100)).toBe(50);
    expect(partPercent(100, 0, 100)).toBe(100);
    expect(partPercent(200, 0, 100)).toBe(100);
    expect(partPercent(0, 0, 0)).toBe(0);
  });
});

describe("applyPartDownloaded", () => {
  it("updates fixed parts by index", () => {
    const parts = [part(0, 0, 100), part(1, 100, 200)];
    const next = applyPartDownloaded(parts, [40, 80], 200);
    expect(next[0]!.downloaded).toBe(40);
    expect(next[1]!.downloaded).toBe(80);
    expect(next[0]!.status).toBe("downloading");
  });

  it("resets to single part when requested", () => {
    const parts = [part(0, 0, 100), part(1, 100, 200)];
    const next = applyPartDownloaded(parts, [50], 200, true);
    expect(next).toHaveLength(1);
    expect(next[0]!.start).toBe(0);
    expect(next[0]!.end).toBe(200);
    expect(next[0]!.downloaded).toBe(50);
  });
});

describe("patchDownloadProgress with parts", () => {
  it("patches parts on matching id", () => {
    const cache = [makeItem(1, [part(0, 0, 100), part(1, 100, 200)])];
    const result = patchDownloadProgress(cache, 1, 120, [100, 20]);
    expect(result?.[0].downloaded).toBe(120);
    expect(result?.[0].parts[0]!.downloaded).toBe(100);
    expect(result?.[0].parts[1]!.downloaded).toBe(20);
  });
});
