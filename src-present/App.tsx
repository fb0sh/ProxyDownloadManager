import { useState, useEffect } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider, BaseStyles, Text } from "@primer/react";
import {
  DownloadIcon, PlugIcon, ShieldIcon,
  BrowserIcon, PasteIcon, SyncIcon,
} from "@primer/octicons-react";
import { setLanguage } from "../src/i18n";
import {
  usePauseDownload, useResumeDownload, useDownloads, useSettings, useRedownloadDownload,
} from "../src/query/downloadQueries";
import Layout from "../src/components/Layout";
import DeleteDialog from "../src/components/dialogs/DeleteDialog";
import SettingsDialog from "../src/components/dialogs/SettingsDialog";
import AboutDialog from "../src/components/dialogs/AboutDialog";
import LogDialog from "../src/components/dialogs/LogDialog";
import ExtensionDialog from "../src/components/dialogs/ExtensionDialog";
import NewDownloadDialog from "../src/components/dialogs/NewDownloadDialog";
import PropertiesDialog from "../src/components/dialogs/PropertiesDialog";
import { AppProvider, useAppContext } from "../src/contexts/AppContext";
import type { DownloadItem } from "../src/types";

/* ─── React Query client ────────────────────────────────────────────── */
const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
});

/* ─── App shell ─────────────────────────────────────────────────────── */

type DemoDialog =
  | { type: "delete"; ids: number[] }
  | { type: "settings" }
  | { type: "about" }
  | { type: "extension" }
  | { type: "log" }
  | { type: "newDownload"; url?: string }
  | { type: "properties"; id: number }
  | null;

function AppInner() {
  const { dialog, setDialog, selectedIds, setSelectedIds } = useAppContext();
  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const redownloadDownload = useRedownloadDownload();
  const { data: downloads = [] } = useDownloads();
  const { settings: loadedSettings } = useSettings();

  useEffect(() => {
    if (loadedSettings) {
      setLanguage(loadedSettings.language || "zh");
    }
  }, [loadedSettings]);

  const selectedForRedownload = selectedIds.size === 1
    ? downloads.find(d => selectedIds.has(d.id) && (d.status === "completed" || d.status.startsWith("failed")))
    : undefined;

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh" }}>
      <Layout onRedownloadItem={selectedForRedownload} />

      {dialog?.type === "delete" && (
        <DeleteDialog ids={dialog.ids} onClose={() => { setDialog(null); setSelectedIds(new Set()); }} />
      )}
      {dialog?.type === "settings" && <SettingsDialog onClose={() => setDialog(null)} />}
      {dialog?.type === "about" && <AboutDialog onClose={() => setDialog(null)} onDownloadUpdate={() => {}} />}
      {dialog?.type === "extension" && <ExtensionDialog onClose={() => setDialog(null)} />}
      {dialog?.type === "log" && <LogDialog onClose={() => setDialog(null)} />}
      {dialog?.type === "newDownload" && (
        <NewDownloadDialog onClose={() => setDialog(null)} initialUrl={dialog.url || ""} />
      )}
      {dialog?.type === "properties" && (
        <PropertiesDialog id={dialog.id} onClose={() => setDialog(null)} />
      )}
    </div>
  );
}

function AppWithProvider() {
  const [dialog, setDialog] = useState<DemoDialog>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [filter, setFilter] = useState<"all" | "completed" | "incomplete">("all");

  // Demo actions — minimal, no quit/real operations
  const actions = {
    onNewDownload: () => setDialog({ type: "newDownload" }),
    onExtension: () => setDialog({ type: "extension" }),
    onLog: () => setDialog({ type: "log" }),
    onSettings: () => setDialog({ type: "settings" }),
    onAbout: () => setDialog({ type: "about" }),
    onQuit: () => {},
    onResumeSelected: async () => {},
    onPauseSelected: () => {},
    onDeleteSelected: () => {
      if (selectedIds.size > 0) setDialog({ type: "delete", ids: Array.from(selectedIds) });
    },
    onStop: () => {},
    onDelete: (ids: number[]) => setDialog({ type: "delete", ids }),
    onProperties: (id: number) => setDialog({ type: "properties", id }),
    onRedownload: async () => {},
  };

  return (
    <AppProvider
      dialog={dialog} setDialog={setDialog as any}
      selectedIds={selectedIds} setSelectedIds={setSelectedIds}
      filter={filter} setFilter={setFilter}
      actions={actions}
    >
      <AppInner />
    </AppProvider>
  );
}

/* ─── Page shell ────────────────────────────────────────────────────── */

const pageStyles = `
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
  html { scroll-behavior: smooth; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif;
    -webkit-font-smoothing: antialiased;
  }
  a { color: #0969da; text-decoration: none; }
  a:hover { text-decoration: underline; }
  .app-window {
    border: 1px solid #d0d7de;
    border-radius: 10px;
    overflow: hidden;
    box-shadow: 0 4px 24px rgba(0,0,0,0.08);
    background: #fff;
    width: 940px;
    height: 520px;
    flex-shrink: 0;
  }
  .app-window-wrap {
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    display: flex;
    justify-content: center;
  }
  @keyframes fadeUp { from { opacity: 0; transform: translateY(20px); } to { opacity: 1; transform: translateY(0); } }
  .anim-fade { animation: fadeUp 0.5s ease both; }
  .anim-fade-1 { animation: fadeUp 0.5s 0.1s ease both; }
  .anim-fade-2 { animation: fadeUp 0.5s 0.2s ease both; }
  .anim-fade-3 { animation: fadeUp 0.5s 0.3s ease both; }

  @media (max-width: 768px) {
    .hero-title { font-size: 28px !important; }
    .hero-sub { font-size: 15px !important; }
    .section-title { font-size: 22px !important; }
    .nav-links { gap: 4px !important; }
    .nav-links a span { padding: 4px 10px !important; font-size: 12px !important; }
    .app-window-wrap { justify-content: flex-start; }
  }
`;

type Feature = {
  icon: typeof DownloadIcon;
  title: string;
  desc: string;
};

function Page() {
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    const f = () => setScrolled(window.scrollY > 40);
    window.addEventListener("scroll", f, { passive: true });
    return () => window.removeEventListener("scroll", f);
  }, []);

  const features: Feature[] = [
    {
      icon: ShieldIcon,
      title: "每下载独立代理",
      desc: "每个下载任务可单独选择代理，支持 HTTP / SOCKS5 协议。同时维护多个代理候选列表，下载失败时自动切换候选代理，无需中断任务。",
    },
    {
      icon: DownloadIcon,
      title: "多代理并发",
      desc: "单任务最高 64 线程并行下载，自动根据文件大小和网络状况调整并发数，大幅提升下载速度。",
    },
    {
      icon: BrowserIcon,
      title: "浏览器扩展联动",
      desc: "Chrome / Edge / Firefox 扩展一键安装。右键菜单或自动拦截下载链接，URL 实时发送到桌面端，浏览器后台无需保持打开。",
    },
    {
      icon: SyncIcon,
      title: "断点续传 & 重试",
      desc: "支持 HTTP Range 请求，中断后自动恢复。多候选代理 + 多 User-Agent 轮询，下载失败自动切换重试，无需人工干预。",
    },
    {
      icon: PasteIcon,
      title: "剪贴板智能监测",
      desc: "自动识别剪贴板中的下载链接，弹出新建下载窗口。支持批量监测，无需手动粘贴。",
    },
    {
      icon: PlugIcon,
      title: "系统深度集成",
      desc: "托盘驻留后台下载、开机自启、系统通知推送、文件关联。macOS 菜单栏模式，Windows / Linux 系统托盘，体验原生。",
    },
  ];

  return (
    <>
      <style>{pageStyles}</style>

      {/* ── Nav ── */}
      <nav style={{
        position: "fixed", top: 0, left: 0, right: 0, zIndex: 100,
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "0 24px", height: 56,
        background: scrolled ? "#fffffff2" : "#fff",
        backdropFilter: scrolled ? "blur(8px)" : "none",
        borderBottom: "1px solid #d0d7de",
        transition: "all 0.2s",
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, fontWeight: 700, fontSize: 16, color: "#1f2328" }}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#1f883d" strokeWidth="2">
            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
          </svg>
          ProxyDownloadManager
        </div>
        <div className="nav-links" style={{ display: "flex", gap: 8 }}>
          <a href="https://github.com/fb0sh/ProxyDownloadManager/releases/latest" target="_blank" rel="noreferrer">
            <span style={{
              display: "inline-flex", alignItems: "center", gap: 6,
              padding: "6px 14px", borderRadius: 6, fontSize: 14, fontWeight: 600,
              background: "#1f883d", color: "#fff", border: "1px solid #1a7f37",
            }}><DownloadIcon size={16} /> 下载</span>
          </a>
          <a href="https://github.com/fb0sh/ProxyDownloadManager" target="_blank" rel="noreferrer">
            <span style={{
              display: "inline-flex", alignItems: "center", gap: 6,
              padding: "6px 14px", borderRadius: 6, fontSize: 14, fontWeight: 600,
              background: "#f6f8fa", color: "#1f2328", border: "1px solid #d0d7de",
            }}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/></svg>
              Source
            </span>
          </a>
        </div>
      </nav>

      {/* ── Hero ── */}
      <section style={{
        minHeight: "100vh", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
        padding: "80px 24px 40px", textAlign: "center",
        background: "linear-gradient(180deg, #f0f6ff 0%, #ffffff 60%)",
      }}>
        <div className="anim-fade" style={{ maxWidth: 860, width: "100%" }}>
          <div style={{
            display: "inline-flex", alignItems: "center", gap: 6,
            padding: "4px 12px", borderRadius: 20, fontSize: 12, fontWeight: 600,
            background: "#ddf4e4", color: "#1a7f37", border: "1px solid #acebbe",
            marginBottom: 20,
          }}>
            <span style={{ width: 6, height: 6, borderRadius: "50%", background: "#1f883d", display: "inline-block" }} />
            v0.5.0 — 多代理下载管理器
          </div>
          <h1 className="hero-title" style={{ fontSize: 44, fontWeight: 800, letterSpacing: "-0.03em", marginBottom: 16, color: "#1f2328", lineHeight: 1.2 }}>
            每个下载都选对代理
          </h1>
          <p className="hero-sub" style={{ fontSize: 18, color: "#656d76", maxWidth: 560, margin: "0 auto 32px", lineHeight: 1.6 }}>
            ProxyDownloadManager 是一款开源下载工具。
            每个任务可独立选择 HTTP / SOCKS5 代理，支持多候选代理自动切换。
            浏览器扩展一键拦截，多代理并发加速，断点续传不中断。
          </p>
          <div style={{ display: "flex", gap: 12, justifyContent: "center", flexWrap: "wrap" }}>
            <a href="https://github.com/fb0sh/ProxyDownloadManager/releases/latest" target="_blank" rel="noreferrer">
              <span style={{
                display: "inline-flex", alignItems: "center", gap: 8,
                padding: "12px 28px", borderRadius: 8, fontSize: 16, fontWeight: 600,
                background: "#1f883d", color: "#fff", border: "1px solid #1a7f37",
              }}><DownloadIcon size={18} /> 下载 ProxyDownloadManager</span>
            </a>
            <a href="https://github.com/fb0sh/ProxyDownloadManager" target="_blank" rel="noreferrer">
              <span style={{
                display: "inline-flex", alignItems: "center", gap: 8,
                padding: "12px 28px", borderRadius: 8, fontSize: 16, fontWeight: 600,
                background: "#f6f8fa", color: "#1f2328", border: "1px solid #d0d7de",
              }}>
                <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/></svg>
                Source
              </span>
            </a>
          </div>
        </div>
      </section>

      {/* ── Live Demo ── */}
      <section style={{ padding: "60px 24px", background: "#f6f8fa" }}>
        <div className="anim-fade" style={{ maxWidth: 940, margin: "0 auto" }}>
          <h2 style={{ fontSize: 28, fontWeight: 700, marginBottom: 8, color: "#1f2328", textAlign: "center" }}>
            实际体验
          </h2>
          <p style={{ textAlign: "center", color: "#656d76", marginBottom: 32, fontSize: 15 }}>
            完整的 ProxyDownloadManager 界面，直接操作试试
          </p>
          <div className="anim-fade-1 app-window-wrap">
            <div className="app-window">
              <QueryClientProvider client={queryClient}>
                <ThemeProvider colorMode="day">
                  <BaseStyles>
                    <AppWithProvider />
                  </BaseStyles>
                </ThemeProvider>
              </QueryClientProvider>
            </div>
          </div>
        </div>
      </section>

      {/* ── Features ── */}
      <section style={{ padding: "60px 24px", maxWidth: 940, margin: "0 auto" }}>
        <h2 className="anim-fade section-title" style={{ fontSize: 28, fontWeight: 700, marginBottom: 40, color: "#1f2328", textAlign: "center" }}>
          核心特性
        </h2>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))", gap: 16 }}>
          {features.map((f, i) => {
            const IconEl = f.icon;
            return (
              <div key={f.title} className={`anim-fade-${i % 4}`} style={{
                padding: 24, borderRadius: 8, border: "1px solid #d0d7de",
                background: "#fff", transition: "all 0.2s",
              }}>
                <div style={{ color: "#1f883d", marginBottom: 12 }}>
                  <IconEl size={24} />
                </div>
                <Text weight="semibold" style={{ display: "block", marginBottom: 6, fontSize: 15, color: "#1f2328" }}>
                  {f.title}
                </Text>
                <Text size="small" style={{ color: "#656d76", lineHeight: 1.6, display: "block" }}>
                  {f.desc}
                </Text>
              </div>
            );
          })}
        </div>
      </section>

      {/* ── Tech ── */}
      <section style={{ padding: "60px 24px", background: "#f6f8fa" }}>
        <div className="anim-fade" style={{ maxWidth: 700, margin: "0 auto", textAlign: "center" }}>
          <h2 style={{ fontSize: 28, fontWeight: 700, marginBottom: 32, color: "#1f2328" }}>
            技术栈
          </h2>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8, justifyContent: "center" }}>
            {[
              ["Tauri 2", "桌面框架"], ["React 19", "前端"], ["Primer React", "UI 组件"],
              ["Rust", "后端引擎"], ["Tokio", "异步运行时"], ["reqwest", "HTTP 客户端"],
              ["SQLite", "任务存储"], ["TypeScript", "类型安全"],
            ].map(([name, desc]) => (
              <span key={name} style={{
                display: "inline-flex", gap: 4, padding: "6px 12px", borderRadius: 6,
                background: "#fff", border: "1px solid #d0d7de", fontSize: 13,
              }}>
                <span style={{ fontWeight: 600 }}>{name}</span>
                <span style={{ color: "#656d76" }}>— {desc}</span>
              </span>
            ))}
          </div>
        </div>
      </section>

      {/* ── CTA ── */}
      <section style={{ padding: "60px 24px 80px", textAlign: "center" }}>
        <div className="anim-fade">
          <h2 style={{ fontSize: 28, fontWeight: 700, marginBottom: 16, color: "#1f2328" }}>立即开始使用</h2>
          <p style={{ color: "#656d76", marginBottom: 28, fontSize: 15 }}>
            免费、开源、跨平台。每下载独立选代理，多候选自动切换。
          </p>
          <a href="https://github.com/fb0sh/ProxyDownloadManager/releases/latest" target="_blank" rel="noreferrer">
            <span style={{
              display: "inline-flex", alignItems: "center", gap: 8,
              padding: "12px 28px", borderRadius: 8, fontSize: 16, fontWeight: 600,
              background: "#1f883d", color: "#fff", border: "1px solid #1a7f37",
            }}><DownloadIcon size={18} /> 下载 ProxyDownloadManager</span>
          </a>
        </div>
      </section>

      {/* ── Footer ── */}
      <footer style={{ textAlign: "center", padding: "24px", borderTop: "1px solid #d0d7de", color: "#656d76", fontSize: 13 }}>
        <p>ProxyDownloadManager — MIT 许可开源</p>
        <p style={{ marginTop: 4 }}>
          <a href="https://github.com/fb0sh/ProxyDownloadManager" target="_blank" rel="noreferrer">GitHub</a>
          {" · "}
          <a href="https://github.com/fb0sh/ProxyDownloadManager/releases" target="_blank" rel="noreferrer">Releases</a>
          {" · "}
          Made with ❤️ by fb0sh &amp; DohHoKun
        </p>
      </footer>
    </>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider colorMode="day">
        <BaseStyles>
          <Page />
        </BaseStyles>
      </ThemeProvider>
    </QueryClientProvider>
  );
}
