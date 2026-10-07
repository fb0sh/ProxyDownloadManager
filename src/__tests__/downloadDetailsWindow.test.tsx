import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DownloadDetailsWindow from "../DownloadDetailsWindow";
import type { DownloadItem } from "../types";

/**
 * The details window renders `item` only after the query resolves, so a hook
 * placed below its early returns runs on some renders and not others: React
 * throws "Rendered more hooks than during the previous render" and the window
 * goes blank, which is what 0.20.0 shipped. Nothing else in the toolchain
 * catches that — there is no eslint config, and tsc cannot see it.
 */
let mockItem: DownloadItem | undefined;
const handles = {
  handleCopyUrl: vi.fn(),
  handleOpenFile: vi.fn(async () => true),
  handleOpenFolder: vi.fn(async () => true),
  handlePause: vi.fn(),
  handleResume: vi.fn(async () => true),
};

vi.mock("@tauri-apps/api/dpi", () => ({
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    setSize: async () => {},
    setTitle: async () => {},
    close: async () => {},
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("../query/downloadQueries", () => {
  // One stable object: the window has an effect keyed on `settings`, so a fresh
  // literal per call would re-run it on every render and spin forever.
  const settings = { language: "zh", proxies: {} };
  return {
    useDeleteDownload: () => ({ mutateAsync: vi.fn() }),
    useRedownloadDownload: () => ({ mutateAsync: vi.fn() }),
    useSettings: () => ({ settings }),
  };
});
vi.mock("../hooks/useDownloadDetail", () => ({
  useDownloadIdFromUrl: () => 1,
  useDownloadDetail: () => ({
    item: mockItem,
    urlCopied: false,
    controls: { busy: false, showPause: false, showResume: false, showOpen: false },
    pendingAction: null,
    ...handles,
  }),
}));
vi.mock("../hooks/useDownloadSpeed", () => ({
  useDownloadSpeed: () => new Map(),
  computeETA: () => "—",
}));
vi.mock("../tauriClient", () => ({
  tauriClient: { refreshDownloadUrl: vi.fn(async () => {}) },
}));

const item: DownloadItem = {
  id: 1,
  url: "https://upos-sz-estgcos.bilivideo.com/x.m4s?sig=1",
  file_name: "x.m4s",
  save_path: "/tmp/x.m4s",
  total_size: 5301228,
  downloaded: 1024,
  status: "failed",
  parts: [],
  proxy_name: "",
  connections: 0,
  resumable: true,
  created_at: "2026-10-07T08:00:00Z",
  last_try: "2026-10-07T08:01:00Z",
  headers: { Referer: "https://www.bilibili.com/" },
  content_type: "video/mp4",
  error_code: "http",
  error_message: "HTTP 403",
  http_status: 403,
};

describe("download details window", () => {
  beforeEach(() => {
    mockItem = undefined;
    window.history.replaceState({}, "", "/?id=1");
  });

  it("keeps rendering while the item has not arrived", () => {
    const view = render(<DownloadDetailsWindow />);
    expect(view.container.textContent?.trim()).toBeTruthy();
  });

  it("survives the item arriving after that first render", () => {
    const view = render(<DownloadDetailsWindow />);
    mockItem = item;
    // Before the fix this second render hit the seeding hook for the first
    // time and React threw "Rendered more hooks than during the previous
    // render", leaving the whole window blank.
    view.rerender(<DownloadDetailsWindow />);
    expect(view.container.textContent).toContain("x.m4s");
  });

  it("shows the headers that will be replayed on the Advanced tab", () => {
    mockItem = item;
    const view = render(<DownloadDetailsWindow />);
    // fireEvent, not .click(): the raw call runs outside act() and the tab's
    // state update would not have been flushed by the next line.
    fireEvent.click(view.getByText("高级"));
    expect(screen.getByDisplayValue("Referer")).toBeTruthy();
    expect(screen.getByDisplayValue("https://www.bilibili.com/")).toBeTruthy();
    expect(screen.getByText("保存并重试")).toBeTruthy();
  });
});
