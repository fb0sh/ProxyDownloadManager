import { describe, it, expect } from "vitest";
import { formatETA, computeETA, computeBps } from "../hooks/useDownloadSpeed";
import type { DownloadItem } from "../types";

function makeItem(overrides: Partial<DownloadItem> = {}): DownloadItem {
  return {
    id: 1,
    url: "https://example.com/file.zip",
    file_name: "file.zip",
    save_path: "/tmp/file.zip",
    total_size: 1000,
    downloaded: 0,
    status: "downloading",
    parts: [],
    proxy_name: "",
    connections: 4,
    resumable: null,
    created_at: "1234567890",
    last_try: "",
    ...overrides,
  };
}

describe("formatETA", () => {
  it("returns dash for zero or negative", () => {
    expect(formatETA(0)).toBe("—");
    expect(formatETA(-5)).toBe("—");
  });

  it("returns dash for infinite", () => {
    expect(formatETA(Infinity)).toBe("—");
    expect(formatETA(NaN)).toBe("—");
  });

  it("formats seconds", () => {
    expect(formatETA(30)).toBe("30s");
    expect(formatETA(59)).toBe("59s");
  });

  it("formats minutes and seconds", () => {
    expect(formatETA(60)).toBe("1m 0s");
    expect(formatETA(90)).toBe("1m 30s");
    expect(formatETA(3599)).toBe("59m 59s");
  });

  it("formats hours and minutes", () => {
    expect(formatETA(3600)).toBe("1h 0m");
    expect(formatETA(7200)).toBe("2h 0m");
    expect(formatETA(86399)).toBe("23h 59m");
  });

  it("formats days and hours", () => {
    expect(formatETA(86400)).toBe("1d 0h");
    expect(formatETA(172800)).toBe("2d 0h");
  });
});

describe("computeETA", () => {
  it("returns dash when bps is zero", () => {
    const item = makeItem({ total_size: 1000, downloaded: 500 });
    expect(computeETA(item, 0)).toBe("—");
  });

  it("returns dash when total_size is zero", () => {
    const item = makeItem({ total_size: 0, downloaded: 0 });
    expect(computeETA(item, 1000)).toBe("—");
  });

  it("returns dash when already complete", () => {
    const item = makeItem({ total_size: 1000, downloaded: 1000 });
    expect(computeETA(item, 1000)).toBe("—");
  });

  it("computes remaining time", () => {
    const item = makeItem({ total_size: 1000, downloaded: 0 });
    // 1000 bytes at 100 bytes/sec = 10 seconds
    expect(computeETA(item, 100)).toBe("10s");
  });

  it("computes remaining time for partial download", () => {
    const item = makeItem({ total_size: 10000, downloaded: 5000 });
    // 5000 remaining at 1000 bytes/sec = 5 seconds
    expect(computeETA(item, 1000)).toBe("5s");
  });
});

describe("computeBps (the code behind the speed display)", () => {
  it("needs at least two samples", () => {
    expect(computeBps([], undefined)).toBeNull();
    expect(computeBps([{ downloaded: 0, time: 0 }], undefined)).toBeNull();
  });

  it("computes bytes/second over the window", () => {
    const samples = [
      { downloaded: 0, time: 0 },
      { downloaded: 1_000_000, time: 1000 },
    ];
    expect(computeBps(samples, undefined)).toBe(1_000_000);
  });

  it("smooths against the previous estimate", () => {
    const samples = [
      { downloaded: 0, time: 0 },
      { downloaded: 1_000_000, time: 1000 },
    ];
    expect(computeBps(samples, 500_000)).toBe(750_000);
  });

  it("decays instead of dashing when the window is flat (the speed-dash bug)", () => {
    const flat = [
      { downloaded: 500, time: 0 },
      { downloaded: 500, time: 1000 },
    ];
    expect(computeBps(flat, 100_000)).toBe(92_000);
    expect(computeBps(flat, undefined)).toBeNull();
  });
});
