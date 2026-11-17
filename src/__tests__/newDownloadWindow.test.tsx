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
  etag: "",
  last_modified: "",
  suggested_connections: 4,
  is_hls: false,
  hls_variants: [],
}));
const takePendingDownload = vi.fn(async (_id: string): Promise<PendingDownloadRequest | null> => null);

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
  const settings = { language: "zh", proxies: { clash: { protocol: "http", host: "127.0.0.1", port: 7890 } }, download_dir: "/tmp", default_proxy: "" };
  return {
    useSettings: () => ({ settings }),
    useStartDownload: () => startDownload,
  };
});
vi.mock("../tauriClient", () => ({
  tauriClient: {
    probeUrl: (...args: unknown[]) => probeUrl(...(args as [])),
    takePendingDownload: (id: string) => takePendingDownload(id),
    startDownload: vi.fn(),
  },
}));

describe("new download window", () => {
  beforeEach(() => {
    listener = null;
    probeUrl.mockClear();
    takePendingDownload.mockReset();
    takePendingDownload.mockResolvedValue(null);
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

  it("recovers full headers and cookies on cold start via ?ticket= without event loss", async () => {
    window.history.replaceState({}, "", "/?view=new-download&ticket=req-cold-1");
    takePendingDownload.mockResolvedValueOnce({
      request_id: "req-cold-1",
      url: "https://site.example/dl/1",
      final_url: "https://cdn.example/secret.zip?sig=9",
      filename: "secret.zip",
      cookies: "sid=xyz",
      referrer: "https://site.example/files",
      headers: { Authorization: "Bearer tok" },
      proxy_name: "clash",
    });

    const view = render(<NewDownloadWindow />);
    await waitFor(() => expect(takePendingDownload).toHaveBeenCalledWith("req-cold-1"));
    expect(screen.getByDisplayValue("https://cdn.example/secret.zip?sig=9")).toBeTruthy();
    expect(screen.getByDisplayValue("secret.zip")).toBeTruthy();
    expect(view.container.textContent).toContain("3 个请求头");

    await waitFor(() => expect(probeUrl).toHaveBeenCalled());
    const [probedUrl, probedHeaders, probedProxy] = probeUrl.mock.calls[0] as unknown as [
      string,
      Record<string, string>,
      string,
    ];
    expect(probedUrl).toBe("https://cdn.example/secret.zip?sig=9");
    expect(probedHeaders.Cookie).toBe("sid=xyz");
    expect(probedHeaders.Referer).toBe("https://site.example/files");
    expect(probedHeaders.Authorization).toBe("Bearer tok");
    expect(probedProxy).toBe("clash");
  });

  it("never touches takePendingDownload on manual blank New Download window", async () => {
    window.history.replaceState({}, "", "/?view=new-download");
    render(<NewDownloadWindow />);
    await waitFor(() => expect(listener).not.toBeNull());
    expect(takePendingDownload).not.toHaveBeenCalled();
  });

  it("warns when probing a binary filename returns text/html login/hotlink trap", async () => {
    probeUrl.mockResolvedValueOnce({
      url: "https://cdn.example/archive.zip",
      final_url: "https://cdn.example/login",
      file_name: "archive.zip",
      file_size: 14200,
      content_type: "text/html; charset=utf-8",
      supports_range: false,
      etag: "",
      last_modified: "",
      suggested_connections: 1,
      is_hls: false,
      hls_variants: [],
    });
    const view = render(<NewDownloadWindow />);
    await waitFor(() => expect(listener).not.toBeNull());
    act(() => {
      listener?.({ payload: { url: "https://cdn.example/archive.zip", filename: "archive.zip" } });
    });
    await waitFor(() => expect(view.container.textContent).toContain("text/html"));
    expect(view.container.textContent).toContain("防盗链");
  });
});
