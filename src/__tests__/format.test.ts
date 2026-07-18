import { describe, it, expect } from "vitest";
import { isFailed, getErrorMessage, statusString, statusColor, formatTimestamp } from "../utils/format";
import type { DownloadStatus } from "../types";

describe("isFailed", () => {
  it("returns true for failed status", () => {
    expect(isFailed({ failed: "timeout" })).toBe(true);
  });

  it("returns false for non-failed statuses", () => {
    expect(isFailed("downloading")).toBe(false);
    expect(isFailed("paused")).toBe(false);
    expect(isFailed("completed")).toBe(false);
    expect(isFailed("queued")).toBe(false);
  });
});

describe("getErrorMessage", () => {
  it("extracts message from failed status", () => {
    expect(getErrorMessage({ failed: "connection refused" })).toBe("connection refused");
  });

  it("returns undefined for non-failed statuses", () => {
    expect(getErrorMessage("downloading")).toBeUndefined();
    expect(getErrorMessage("completed")).toBeUndefined();
  });
});

describe("statusString", () => {
  it("returns string for string statuses", () => {
    expect(statusString("downloading")).toBe("downloading");
    expect(statusString("completed")).toBe("completed");
  });

  it("returns 'failed' for failed status", () => {
    expect(statusString({ failed: "timeout" })).toBe("failed");
  });
});

describe("statusColor", () => {
  it("returns danger for failed", () => {
    expect(statusColor({ failed: "timeout" })).toBe("danger");
  });

  it("returns success for completed", () => {
    expect(statusColor("completed")).toBe("success");
  });

  it("returns attention for paused", () => {
    expect(statusColor("paused")).toBe("attention");
  });

  it("returns accent for downloading", () => {
    expect(statusColor("downloading")).toBe("accent");
  });

  it("returns default for other statuses", () => {
    expect(statusColor("queued")).toBe("default");
  });
});

describe("formatTimestamp", () => {
  it("returns dash for empty string", () => {
    expect(formatTimestamp("")).toBe("—");
  });

  it("formats valid unix timestamp", () => {
    // 2024-01-01 00:00:00 UTC = 1704067200
    const result = formatTimestamp("1704067200");
    expect(result).toContain("2024");
    expect(result).toContain("01");
  });

  it("returns original string for invalid timestamp", () => {
    expect(formatTimestamp("not-a-number")).toBe("not-a-number");
  });

  it("returns original string for zero timestamp", () => {
    expect(formatTimestamp("0")).toBe("0");
  });
});
