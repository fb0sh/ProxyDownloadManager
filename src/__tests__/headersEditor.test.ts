import { headersToRows, newHeaderRow, parseHeaderLines, rowsToHeaders } from "../components/HeadersEditor";

describe("headers editor", () => {
  it("round-trips a header map through rows", () => {
    const rows = headersToRows({ Referer: "https://www.bilibili.com/", Cookie: "sid=1" });
    expect(rows.map((r) => r.name)).toEqual(["Referer", "Cookie", ""]);
    expect(rowsToHeaders(rows)).toEqual({ Referer: "https://www.bilibili.com/", Cookie: "sid=1" });
  });

  it("drops half-filled rows", () => {
    const rows = [newHeaderRow("Referer", "https://x/"), newHeaderRow("Accept", ""), newHeaderRow("", "v")];
    expect(rowsToHeaders(rows)).toEqual({ Referer: "https://x/" });
  });

  it("trims the name but keeps the value as typed", () => {
    expect(rowsToHeaders([newHeaderRow("  Referer  ", " https://x/ ")])).toEqual({ Referer: " https://x/ " });
  });

  it("parses pasted request headers", () => {
    const pasted = [
      "accept: */*",
      "referer: https://www.bilibili.com/",
      "",
      "  origin: https://www.bilibili.com  ",
      "no-colon-here",
    ].join("\n");
    expect(parseHeaderLines(pasted)).toEqual({
      accept: "*/*",
      referer: "https://www.bilibili.com/",
      origin: "https://www.bilibili.com",
    });
  });

  it("parses curl -H lines, quotes, trailing backslashes and all", () => {
    const pasted = [
      "curl 'https://upos-sz-estgcos.bilivideo.com/x.m4s' \\",
      "  -H 'accept: */*' \\",
      '  -H "referer: https://www.bilibili.com/" \\',
      "  -H 'range: bytes=0-' \\",
      "  --compressed",
    ].join("\n");
    expect(parseHeaderLines(pasted)).toEqual({
      accept: "*/*",
      referer: "https://www.bilibili.com/",
      range: "bytes=0-",
    });
  });

  it("ignores comments and empty input", () => {
    expect(parseHeaderLines("# just a note\n\n")).toEqual({});
  });
});
