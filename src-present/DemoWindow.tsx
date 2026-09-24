import { useEffect, type ReactNode } from "react";

interface DemoWindowProps {
  title: string;
  /** Real window size from useWindowManager; clamped to the demo frame. */
  width: number;
  height: number;
  onClose: () => void;
  children: ReactNode;
}

/**
 * A second OS window drawn inside the demo frame. The desktop app opens
 * 新建下载 / 详情 as separate 640x560 and 460x520 windows, so the showcase
 * renders the same components behind the same chrome instead of inventing
 * a dialog the product never shows.
 */
export default function DemoWindow({ title, width, height, onClose, children }: DemoWindowProps) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    // The real window closes itself (WebviewWindow.close()); the mock turns
    // that into this event so 下载 / 稍后下载 dismiss the panel too.
    const onCloseRequest = () => onClose();
    window.addEventListener("keydown", onKey);
    window.addEventListener("demo-window-close", onCloseRequest);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("demo-window-close", onCloseRequest);
    };
  }, [onClose]);

  return (
    <div className="demo-window-layer" onMouseDown={onClose}>
      <div
        className="demo-window"
        style={{ width: `min(${width}px, 100%)`, height: `min(${height}px, 100%)` }}
        onMouseDown={(e) => e.stopPropagation()}
        role="dialog"
        aria-label={title}
      >
        <div className="demo-chrome flex items-center gap-2 border-b border-[#e5e5e5] bg-[#f5f5f5] px-3">
          <button
            type="button"
            onClick={onClose}
            aria-label="close"
            title="close"
            className="size-2.5 rounded-full bg-[#d4d4d4] hover:bg-[#a3a3a3]"
          />
          <span className="size-2.5 rounded-full bg-[#d4d4d4]" />
          <span className="size-2.5 rounded-full bg-[#d4d4d4]" />
          <span className="site-mono mx-auto text-[11px] text-[#737373]">{title}</span>
        </div>
        <div className="demo-window-body">{children}</div>
      </div>
    </div>
  );
}
