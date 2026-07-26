import { describe, it, expect, vi, beforeEach } from "vitest";
import type { DownloadItem } from "../types";

// Capture listeners registered through the seam so tests can fire events.
const listeners = new Map<string, (event: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, cb: (event: { payload: unknown }) => void) => {
    listeners.set(name, cb);
    return Promise.resolve(() => listeners.delete(name));
  }),
}));

import { subscribeDownloadEvents } from "../downloadEvents";
import { EVENTS } from "../constants/events";

function fakeQueryClient() {
  return {
    setQueryData: vi.fn(),
    invalidateQueries: vi.fn(),
  };
}

function fire(name: string, payload: unknown) {
  const cb = listeners.get(name);
  if (!cb) throw new Error(`no listener for ${name}`);
  cb({ payload });
}

function sampleItem(id: number): DownloadItem {
  return {
    id,
    url: "https://example.com/a.zip",
    file_name: "a.zip",
    save_path: "/tmp/a.zip",
    total_size: 1000,
    downloaded: 0,
    status: "downloading",
    parts: [],
    proxy_name: "",
    connections: 4,
    resumable: true,
    created_at: "0",
    last_try: "",
  };
}

describe("subscribeDownloadEvents", () => {
  beforeEach(() => {
    listeners.clear();
    vi.clearAllMocks();
  });

  it("patches the downloads cache on progress events", () => {
    const qc = fakeQueryClient();
    subscribeDownloadEvents(qc as never);

    fire(EVENTS.DOWNLOAD_PROGRESS, { id: 1, downloaded: 500 });

    expect(qc.setQueryData).toHaveBeenCalledTimes(1);
    const [key, updater] = qc.setQueryData.mock.calls[0];
    expect(key).toEqual(["downloads"]);
    const updated = (updater as (old: DownloadItem[]) => DownloadItem[])([sampleItem(1), sampleItem(2)]);
    expect(updated[0].downloaded).toBe(500);
    expect(updated[1].downloaded).toBe(0);
    expect(qc.invalidateQueries).not.toHaveBeenCalled();
  });

  it("invalidates on every lifecycle event", () => {
    const qc = fakeQueryClient();
    subscribeDownloadEvents(qc as never);

    fire(EVENTS.DOWNLOAD_PAUSED, { id: 1 });
    fire(EVENTS.DOWNLOAD_RESUMED, { id: 1 });
    fire(EVENTS.DOWNLOAD_CANCELLED, { id: 1 });
    fire(EVENTS.DOWNLOAD_CREATED, {});
    fire(EVENTS.DOWNLOAD_COMPLETED, { id: 1, file_name: "a.zip" });
    fire(EVENTS.DOWNLOAD_ERROR, { id: 1, url: "u", message: "boom" });

    expect(qc.invalidateQueries).toHaveBeenCalledTimes(6);
    expect(qc.invalidateQueries).toHaveBeenCalledWith({ queryKey: ["downloads"] });
  });

  it("forwards domain handlers with typed payloads", () => {
    const qc = fakeQueryClient();
    const onStarted = vi.fn();
    const onCompleted = vi.fn();
    const onError = vi.fn();
    const onBrowserDownloadUrl = vi.fn();
    const onCreated = vi.fn();
    subscribeDownloadEvents(qc as never, {
      onStarted,
      onCompleted,
      onError,
      onBrowserDownloadUrl,
      onCreated,
    });

    fire(EVENTS.DOWNLOAD_STARTED, 7);
    fire(EVENTS.DOWNLOAD_COMPLETED, { id: 7, file_name: "a.zip" });
    fire(EVENTS.DOWNLOAD_ERROR, { id: 7, url: "u", message: "boom" });
    fire(EVENTS.BROWSER_DOWNLOAD_URL, "https://example.com/b.zip");
    fire(EVENTS.DOWNLOAD_CREATED, {});

    expect(onStarted).toHaveBeenCalledWith(7);
    expect(onCompleted).toHaveBeenCalledWith({ id: 7, file_name: "a.zip" });
    expect(onError).toHaveBeenCalledWith({ id: 7, url: "u", message: "boom" });
    expect(onBrowserDownloadUrl).toHaveBeenCalledWith("https://example.com/b.zip");
    expect(onCreated).toHaveBeenCalled();
  });

  it("drops events after unsubscribe", () => {
    const qc = fakeQueryClient();
    const onStarted = vi.fn();
    const unsubscribe = subscribeDownloadEvents(qc as never, { onStarted });

    // Grab handlers before unsubscribe removes them from the registry.
    const progress = listeners.get(EVENTS.DOWNLOAD_PROGRESS)!;
    const started = listeners.get(EVENTS.DOWNLOAD_STARTED)!;
    unsubscribe();

    progress({ payload: { id: 1, downloaded: 500 } });
    started({ payload: 1 });

    expect(qc.setQueryData).not.toHaveBeenCalled();
    expect(onStarted).not.toHaveBeenCalled();
  });

  it("actually removes the backend listeners on unsubscribe", async () => {
    const qc = fakeQueryClient();
    const unsubscribe = subscribeDownloadEvents(qc as never);
    expect(listeners.size).toBeGreaterThan(0);

    unsubscribe();
    // The unlisten fns run when the listen promises resolve.
    await Promise.resolve();
    await Promise.resolve();
    expect(listeners.size).toBe(0);
  });

  it("respects the progressId filter (details window)", () => {
    const qc = fakeQueryClient();
    subscribeDownloadEvents(qc as never, {}, { progressId: 7 });

    fire(EVENTS.DOWNLOAD_PROGRESS, { id: 1, downloaded: 500 });
    expect(qc.setQueryData).not.toHaveBeenCalled();

    fire(EVENTS.DOWNLOAD_PROGRESS, { id: 7, downloaded: 500 });
    expect(qc.setQueryData).toHaveBeenCalledTimes(1);
  });
});
