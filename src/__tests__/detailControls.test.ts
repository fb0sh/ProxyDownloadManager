import { describe, it, expect } from "vitest";
import { detailControls } from "../hooks/useDownloadDetail";

describe("detailControls", () => {
  it("shows pause while downloading, resume while paused/queued, open when completed", () => {
    expect(detailControls("downloading", null)).toEqual({
      busy: false, showPause: true, showResume: false, showOpen: false,
    });
    expect(detailControls("paused", null)).toEqual({
      busy: false, showPause: false, showResume: true, showOpen: false,
    });
    expect(detailControls("queued", null)).toEqual({
      busy: false, showPause: false, showResume: true, showOpen: false,
    });
    expect(detailControls("completed", null)).toEqual({
      busy: false, showPause: false, showResume: false, showOpen: true,
    });
  });

  it("keeps the clicked control visible while status lags the action (5a22aab class)", () => {
    // Clicked pause; backend still says downloading→paused transition pending.
    const pausing = detailControls("downloading", "pause");
    expect(pausing.busy).toBe(true);
    expect(pausing.showPause).toBe(true);

    // Status already flipped to paused but the await hasn't resolved:
    // the pause button must not vanish mid-click.
    const lagging = detailControls("paused", "pause");
    expect(lagging.showPause).toBe(true);
    expect(lagging.busy).toBe(true);

    // Clicked resume from paused; button stays until success.
    const resuming = detailControls("downloading", "resume");
    expect(resuming.showResume).toBe(true);
  });

  it("keeps open buttons during openFile/openFolder and disables everything while busy", () => {
    const opening = detailControls("completed", "openFile");
    expect(opening.showOpen).toBe(true);
    expect(opening.busy).toBe(true);
    expect(detailControls("completed", "copyUrl").busy).toBe(true);
  });

  it("failed status offers no controls (redownload lives in the main window)", () => {
    const failed = detailControls({ failed: "boom" }, null);
    expect(failed).toEqual({
      busy: false, showPause: false, showResume: false, showOpen: false,
    });
  });
});
