import { describe, it, expect } from "vitest";
import { isActiveStatus, isFailed, statusString, formatRateLimit } from "../utils/format";

describe("status helpers", () => {
  it("treats connecting/retrying/merging as live", () => {
    expect(isActiveStatus("connecting")).toBe(true);
    expect(isActiveStatus("retrying")).toBe(true);
    expect(isActiveStatus("merging")).toBe(true);
    expect(isActiveStatus("paused")).toBe(false);
  });

  it("failed object stays failed", () => {
    expect(isFailed({ failed: "HTTP 403" })).toBe(true);
    expect(statusString({ failed: "HTTP 403" })).toBe("failed");
  });

  it("formats rate limits", () => {
    expect(formatRateLimit(0)).toBe("Unlimited");
    expect(formatRateLimit(1024 * 1024)).toBe("1 MB/s");
  });
});
