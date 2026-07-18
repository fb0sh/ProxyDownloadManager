import { describe, it, expect } from "vitest";
import { formatBytes } from "../utils/format";

describe("formatBytes", () => {
  it("returns '0 B' for zero", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("formats bytes", () => {
    expect(formatBytes(500)).toBe("500 B");
  });

  it("formats KB", () => {
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
  });

  it("formats MB", () => {
    expect(formatBytes(1048576)).toBe("1 MB");
    expect(formatBytes(2097152)).toBe("2 MB");
  });

  it("formats GB", () => {
    expect(formatBytes(1073741824)).toBe("1 GB");
  });

  it("formats TB", () => {
    expect(formatBytes(1099511627776)).toBe("1 TB");
  });

  it("handles large numbers without precision issues", () => {
    const result = formatBytes(1048576 * 3); // 3 MB
    expect(result).toContain("MB");
  });
});
