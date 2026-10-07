import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import NewDownloadWindow from "../NewDownloadWindow";
import type { PendingDownloadRequest } from "../types";

/**
 * The window that appears when a download starts — including one the browser
 * extension sent, which arrives as an event after the first render. That is the
 * same shape that blanked the details window once, so the render is pinned here
 * too, along with the headers being editable before the download starts.
 */
type Listener = (event: { payload: unknown }) => void;
let listener: Listener | null = null;
const startDownload = { mutateAsync: vi.fn(async () => 1), isPending: false };
const probeUrl = vi.fn(async () => ({
  url: "https://cdn.example/x.m4s",
  final_url: "https://cdn.example/x.m4s",
  file_name: "x.m4s",
  file_size: 5301228,
  content_type: "application/octet-stream",
  supports_range: true,
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ close: async () => {}, setTitle: async () => {} }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_name: string, cb: Listener) => {
    listener = cb;
    return () => {};
  },
}));
vi.mock("../query/downloadQueries", () => {
  // Stable reference: the window has effects keyed on settings.
  const settings = { language: "zh", proxies: {}, download_dir: "/tmp", default_proxy: "" };
  return {
    useSettings: () => ({ settings }),
    useStartDownload: () => startDownload,
  };
});
vi.mock("../tauriClient", () => ({
  tauriClient: {
    probeUrl: (...args: unknown[]) => probeUrl(...(args as [])),
    startDownload: vi.fn(),
  },
}));

describe("new download window", () => {
  beforeEach(() => {
    listener = null;
    window.history.replaceState({}, "", "/?view=new-download");
  });

  it("renders before any request arrives and again when one does", async () => {
    const { container } = render(<NewDownloadWindow />);
    expect(container.textContent?.trim()).toBeTruthy();
    await waitFor(() => expect(listener).not.toBeNull());
    act(() => {
      listener?.({ payload: { url: "https://cdn.example/x.m4s" } as PendingDownloadRequest });
    });
    // The filename and url live in inputs, so textContent cannot see them.
    expect(screen.getByDisplayValue("https://cdn.example/x.m4s")).toBeTruthy();
  });

  it("shows the headers a browser download brought, and lets you edit them", async () => {
    const view = render(<NewDownloadWindow />);
    await waitFor(() => expect(listener).not.toBeNull());
    act(() => {
      listener?.({
        payload: {
          url: "https://upos-sz-estgcos.bilivideo.com/x.m4s?sig=1",
          filename: "x.m4s",
          headers: { Referer: "https://www.bilibili.com/" },
        } as PendingDownloadRequest,
      });
    });
    // Closed by default, but it says how many headers will be sent.
    expect(view.container.textContent).toContain("1 个请求头");
    fireEvent.click(view.getByText("高级"));
    expect(screen.getByDisplayValue("Referer")).toBeTruthy();
    expect(screen.getByDisplayValue("https://www.bilibili.com/")).toBeTruthy();
  });

  it("probes with the edited headers", async () => {
    const view = render(<NewDownloadWindow />);
    await waitFor(() => expect(listener).not.toBeNull());
    act(() => {
      listener?.({
        payload: { url: "https://cdn.example/y.mp4", headers: { Referer: "https://site.example/" } } as PendingDownloadRequest,
      });
    });
    await waitFor(() => expect(probeUrl).toHaveBeenCalled());
    const lastCall = probeUrl.mock.calls[probeUrl.mock.calls.length - 1] as unknown as [string, Record<string, string>];
    const headers = lastCall[1];
    expect(headers.Referer).toBe("https://site.example/");
    expect(view.container.textContent).toContain("5.1 MB");
  });
});
