import { describe, it, expect } from "vitest";
import {
  partPercent,
  applyPartDownloaded,
} from "../utils/progressMap";
import type { DownloadPart } from "../types";
import { patchDownloadProgress } from "../downloadEvents";
import { cellPercents, overallPercent } from "../utils/progressMap";
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

describe("cellPercents (Progress Map status rules)", () => {
  it("completed forces every cell to 100% even with stale part data", () => {
    const parts = [part(0, 0, 100), part(1, 100, 200)];
    parts[0]!.downloaded = 100;
    parts[1]!.downloaded = 30; // last part event lost
    expect(cellPercents(parts, "completed")).toEqual([100, 100]);
  });

  it("other statuses show recorded per-part progress", () => {
    const parts = [part(0, 0, 100), part(1, 100, 200)];
    parts[0]!.downloaded = 50;
    expect(cellPercents(parts, "downloading")).toEqual([50, 0]);
    expect(cellPercents(parts, "paused")).toEqual([50, 0]); // frozen
    expect(cellPercents(parts, { failed: "x" })).toEqual([50, 0]); // kept
  });

  it("empty parts yields no cells", () => {
    expect(cellPercents([] as DownloadPart[], "downloading")).toEqual([]);
  });
});

describe("overallPercent (the one formula)", () => {
  it("floors instead of rounding — never shows 100% early", () => {
    expect(overallPercent(999, 1000, "downloading")).toBe(99);
    expect(overallPercent(995, 1000, "downloading")).toBe(99);
  });

  it("completed always reads 100", () => {
    expect(overallPercent(0, 1000, "completed")).toBe(100);
    expect(overallPercent(0, 0, "completed")).toBe(100);
  });

  it("clamps and handles unknown size", () => {
    expect(overallPercent(2000, 1000, "downloading")).toBe(100);
    expect(overallPercent(500, 0, "downloading")).toBe(0);
  });
});

import { threadBars } from "../components/ProgressMap";

describe("threadBars", () => {
  it("folds parts onto connection slots", () => {
    const parts = [
      part(0, 0, 100, 100),
      part(1, 100, 200, 50),
      part(2, 200, 300, 0),
      part(3, 300, 400, 0),
    ];
    const bars = threadBars(parts, 2);
    expect(bars).toHaveLength(2);
    expect(bars[0]!.index).toBe(1);
    expect(bars[0]!.percent).toBe(50); // parts 0+2: 100/200
    expect(bars[1]!.percent).toBe(25); // parts 1+3: 50/200
  });

  it("returns empty bars when there are no parts yet", () => {
    expect(threadBars([], 4).map((b) => b.percent)).toEqual([0, 0, 0, 0]);
  });
});
