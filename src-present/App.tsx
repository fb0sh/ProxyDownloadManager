import { useState, useEffect, useRef } from "react";

/* =========================================================================
 * Styles
 * ========================================================================= */
const S = {
  // ── Reset ──────────────────────────────────────────────────────────────
  global: `
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    html { scroll-behavior: smooth; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif;
      background: #0d1117; color: #e6edf3; line-height: 1.6;
      -webkit-font-smoothing: antialiased;
    }
    a { color: #58a6ff; text-decoration: none; }
    a:hover { text-decoration: underline; }
    ::selection { background: #1f6feb33; }

    /* Animations */
    @keyframes fadeUp { from { opacity: 0; transform: translateY(24px); } to { opacity: 1; transform: translateY(0); } }
    @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
    @keyframes pulse { 0%, 100% { opacity: 0.4; } 50% { opacity: 1; } }
    @keyframes slideBar { from { width: 0; } to { width: var(--w); } }
    @keyframes float {
      0%, 100% { transform: translateY(0); }
      50% { transform: translateY(-8px); }
    }

    .anim-fade-up { animation: fadeUp 0.6s ease both; }
    .anim-fade-up-1 { animation: fadeUp 0.6s 0.1s ease both; }
    .anim-fade-up-2 { animation: fadeUp 0.6s 0.2s ease both; }
    .anim-fade-up-3 { animation: fadeUp 0.6s 0.3s ease both; }
    .anim-fade-in { animation: fadeIn 0.8s ease both; }
  `,

  // ── Nav ────────────────────────────────────────────────────────────────
  nav: {
    position: "fixed" as const, top: 0, left: 0, right: 0, zIndex: 100,
    display: "flex", alignItems: "center", justifyContent: "space-between",
    padding: "0 24px", height: 56,
    background: "#0d1117e6", backdropFilter: "blur(12px)",
    borderBottom: "1px solid #30363d",
  },
  navLogo: { display: "flex", alignItems: "center", gap: 8, fontWeight: 700, fontSize: 16, color: "#e6edf3" },
  navLinks: { display: "flex", alignItems: "center", gap: 12 },
  navBtn: (primary = false): React.CSSProperties => ({
    display: "inline-flex", alignItems: "center", gap: 6,
    padding: "6px 16px", borderRadius: 6, fontSize: 14, fontWeight: 600,
    background: primary ? "#238636" : "#21262d",
    color: primary ? "#fff" : "#e6edf3",
    border: primary ? "1px solid #2ea043" : "1px solid #30363d",
    cursor: "pointer", transition: "all 0.2s",
  }),

  // ── Hero ───────────────────────────────────────────────────────────────
  hero: {
    minHeight: "100vh", display: "flex", flexDirection: "column" as const,
    alignItems: "center", justifyContent: "center",
    padding: "80px 24px 60px", textAlign: "center" as const,
    background: "radial-gradient(ellipse at 50% 0%, #161b22 0%, #0d1117 70%)",
  },
  heroTitle: { fontSize: 48, fontWeight: 800, letterSpacing: "-0.03em", marginBottom: 16, lineHeight: 1.2 },
  heroSub: { fontSize: 18, color: "#8b949e", maxWidth: 560, marginBottom: 32, lineHeight: 1.6 },
  heroCta: {
    display: "inline-flex", alignItems: "center", gap: 8,
    padding: "12px 28px", borderRadius: 8, fontSize: 16, fontWeight: 600,
    background: "#238636", color: "#fff",
    border: "1px solid #2ea043",
    cursor: "pointer", transition: "all 0.2s",
  },

  // ── Section ────────────────────────────────────────────────────────────
  section: { padding: "80px 24px", maxWidth: 1100, margin: "0 auto" },
  sectionTitle: { fontSize: 32, fontWeight: 700, marginBottom: 8, letterSpacing: "-0.02em" },
  sectionSub: { fontSize: 16, color: "#8b949e", marginBottom: 48, maxWidth: 560 },

  // ── Mock App Window ────────────────────────────────────────────────────
  mockWindow: {
    width: "100%", maxWidth: 820, borderRadius: 12, overflow: "hidden",
    boxShadow: "0 8px 32px rgba(0,0,0,0.4)",
    border: "1px solid #30363d",
    background: "#0d1117",
    margin: "0 auto",
  } as React.CSSProperties,
  mockTitlebar: {
    display: "flex", alignItems: "center", gap: 8,
    padding: "10px 16px",
    background: "#161b22",
    borderBottom: "1px solid #30363d",
  } as React.CSSProperties,
  mockDot: (color: string): React.CSSProperties => ({
    width: 12, height: 12, borderRadius: "50%", background: color,
  }),
  mockBody: { padding: 0 } as React.CSSProperties,

  // ── Feature Grid ───────────────────────────────────────────────────────
  featureGrid: {
    display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
    gap: 16,
  } as React.CSSProperties,
  featureCard: {
    padding: 24, borderRadius: 10,
    border: "1px solid #30363d",
    background: "#161b22",
    transition: "all 0.3s",
    cursor: "default",
  } as React.CSSProperties,
  featureIcon: { fontSize: 28, marginBottom: 12, display: "block" },
  featureTitle: { fontSize: 16, fontWeight: 600, marginBottom: 8 },
  featureDesc: { fontSize: 14, color: "#8b949e", lineHeight: 1.6 },

  // ── Interactive Demo ────────────────────────────────────────────────────
  demoTable: {
    width: "100%", borderCollapse: "collapse" as const,
    fontSize: 13,
  },
  demoTh: {
    textAlign: "left" as const, padding: "8px 12px",
    borderBottom: "1px solid #30363d",
    color: "#8b949e", fontWeight: 600, fontSize: 12,
    textTransform: "uppercase" as const, letterSpacing: "0.05em",
  },
  demoTd: { padding: "8px 12px", borderBottom: "1px solid #21262d", verticalAlign: "middle" as const },
  progressBar: {
    height: 6, borderRadius: 3, background: "#21262d", overflow: "hidden",
  } as React.CSSProperties,
  progressFill: {
    height: "100%", borderRadius: 3, background: "#238636",
    transition: "width 0.5s ease",
  } as React.CSSProperties,
  statusBadge: (status: string): React.CSSProperties => {
    const colors: Record<string, string> = {
      downloading: "#1f6feb", completed: "#238636", paused: "#d29922", failed: "#da3633",
    };
    return {
      display: "inline-block", padding: "2px 8px", borderRadius: 12, fontSize: 11, fontWeight: 600,
      background: `${colors[status] || "#21262d"}33`,
      color: colors[status] || "#8b949e",
    };
  },

  // ── Tech Stack ─────────────────────────────────────────────────────────
  techGrid: {
    display: "flex", flexWrap: "wrap" as const, gap: 8,
  } as React.CSSProperties,
  techBadge: {
    display: "inline-flex", alignItems: "center", gap: 4,
    padding: "6px 12px", borderRadius: 6,
    background: "#161b22", border: "1px solid #30363d",
    fontSize: 13, fontWeight: 500,
  } as React.CSSProperties,

  // ── Footer ──────────────────────────────────────────────────────────────
  footer: {
    textAlign: "center" as const, padding: "32px 24px",
    borderTop: "1px solid #21262d", color: "#8b949e", fontSize: 13,
  } as React.CSSProperties,
};

/* =========================================================================
 * Mock data
 * ========================================================================= */
type MockStatus = "downloading" | "completed" | "paused" | "failed";
interface MockItem {
  id: number; name: string; url: string; size: string; status: MockStatus;
  progress: number; speed: string; downloaded: string;
}

const mockItems: MockItem[] = [
  { id: 1, name: "ubuntu-24.04-desktop-amd64.iso", url: "https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso", size: "5.7 GB", status: "downloading", progress: 67, speed: "12.4 MB/s", downloaded: "3.8 GB" },
  { id: 2, name: "node-v22.0.0.pkg", url: "https://nodejs.org/dist/v22.0.0/node-v22.0.0.pkg", size: "84 MB", status: "completed", progress: 100, speed: "—", downloaded: "84 MB" },
  { id: 3, name: "vscode_amd64.deb", url: "https://code.visualstudio.com/sha/download?build=stable&os=linux-deb-x64", size: "112 MB", status: "downloading", progress: 43, speed: "5.2 MB/s", downloaded: "48 MB" },
  { id: 4, name: "docker-desktop-mac.dmg", url: "https://desktop.docker.com/mac/main/amd64/Docker.dmg", size: "280 MB", status: "paused", progress: 78, speed: "—", downloaded: "218 MB" },
  { id: 5, name: "proxydm-0.5.0-x86_64.AppImage", url: "https://github.com/fb0sh/ProxyDownloadManager/releases/download/v0.5.0/proxydm-0.5.0.AppImage", size: "12 MB", status: "completed", progress: 100, speed: "—", downloaded: "12 MB" },
];

/* =========================================================================
 * Components
 * ========================================================================= */

function MockAppWindow() {
  return (
    <div style={S.mockWindow}>
      {/* Title bar */}
      <div style={S.mockTitlebar}>
        <div style={S.mockDot("#ff5f57")} />
        <div style={S.mockDot("#febc2e")} />
        <div style={S.mockDot("#28c840")} />
        <span style={{ marginLeft: 12, fontSize: 12, color: "#8b949e" }}>
          ProxyDownloadManager 0.5.0
        </span>
      </div>
      {/* Toolbar */}
      <div style={{
        display: "flex", alignItems: "center", gap: 4,
        padding: "8px 12px", borderBottom: "1px solid #21262d",
        background: "#161b22",
      }}>
        {["New Download", "Extensions", "Log", "Settings", "About"].map(text => (
          <span key={text} style={{
            padding: "4px 12px", borderRadius: 6, fontSize: 12, fontWeight: 500,
            color: "#e6edf3", cursor: "default",
            background: text === "New Download" ? "#238636" : "transparent",
            transition: "background 0.2s",
          }}>{text}</span>
        ))}
        <span style={{ flex: 1 }} />
        <span style={{ padding: "4px 12px", borderRadius: 6, fontSize: 12, color: "#f78166", cursor: "default" }}>
          Quit
        </span>
      </div>
      {/* Download table */}
      <table style={S.demoTable}>
        <thead>
          <tr>
            {["Name", "Size", "Status", "Progress", "Speed"].map(h => (
              <th key={h} style={S.demoTh}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {mockItems.map((item, idx) => (
            <tr key={item.id} style={{
              animation: `fadeUp 0.5s ${0.1 * idx}s ease both`,
              opacity: 0,
            }}>
              <td style={S.demoTd}>
                <div style={{ fontWeight: 500, fontSize: 13, maxWidth: 280, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {item.name}
                </div>
                <div style={{ fontSize: 11, color: "#8b949e", marginTop: 2, maxWidth: 280, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {item.url}
                </div>
              </td>
              <td style={S.demoTd}>{item.size}</td>
              <td style={S.demoTd}>
                <span style={S.statusBadge(item.status)}>{item.status}</span>
              </td>
              <td style={{ ...S.demoTd, minWidth: 160 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <div style={{ ...S.progressBar, flex: 1 }}>
                    <div style={{ ...S.progressFill, width: `${item.progress}%` }} />
                  </div>
                  <span style={{ fontSize: 11, color: "#8b949e", minWidth: 40 }}>
                    {item.progress}%
                  </span>
                </div>
              </td>
              <td style={S.demoTd}>
                <span style={{ fontSize: 12, color: "#8b949e" }}>{item.speed}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "center",
        padding: "8px 12px", borderTop: "1px solid #21262d",
        fontSize: 12, color: "#8b949e",
      }}>
        <span>5 downloads — 2 active</span>
        <span>⬇ 17.6 MB/s</span>
      </div>
    </div>
  );
}

function FeatureCard({ icon, title, desc, idx }: { icon: string; title: string; desc: string; idx: number }) {
  const [hover, setHover] = useState(false);
  return (
    <div
      className={`anim-fade-up-${idx % 4}`}
      style={{
        ...S.featureCard,
        transform: hover ? "translateY(-4px)" : "none",
        borderColor: hover ? "#58a6ff" : "#30363d",
      }}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <span style={S.featureIcon}>{icon}</span>
      <div style={S.featureTitle}>{title}</div>
      <div style={S.featureDesc}>{desc}</div>
    </div>
  );
}

/* Progress bar that animates on scroll into view */
function AnimatedBar({ target, color = "#238636" }: { target: number; color?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const obs = new IntersectionObserver(([e]) => {
      if (e.isIntersecting) {
        setTimeout(() => setW(target), 200);
        obs.disconnect();
      }
    }, { threshold: 0.3 });
    obs.observe(el);
    return () => obs.disconnect();
  }, [target]);
  return (
    <div ref={ref} style={{ height: 6, borderRadius: 3, background: "#21262d", overflow: "hidden" }}>
      <div style={{ height: "100%", borderRadius: 3, background: color, width: `${w}%`, transition: "width 1s ease" }} />
    </div>
  );
}

/* =========================================================================
 * App
 * ========================================================================= */

function App() {
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 40);
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  const features = [
    { icon: "⬇️", title: "多线程下载", desc: "单任务最高 32 线程并行下载，自动根据文件大小调整连接数，充分利用带宽。" },
    { icon: "🔁", title: "断点续传", desc: "支持 HTTP Range 请求，下载中断后自动恢复，无需从头开始。" },
    { icon: "🔒", title: "代理支持", desc: "HTTP 和 SOCKS5 代理，支持全局代理和每下载独立代理配置。" },
    { icon: "🧩", title: "浏览器扩展", desc: "Chrome / Edge / Firefox 扩展，一键拦截下载并交由 ProxyDM 接管。" },
    { icon: "📋", title: "剪贴板监测", desc: "自动检测剪贴板中的下载链接，弹出新建下载窗口，无需手动粘贴。" },
    { icon: "⚡", title: "系统集成", desc: "系统托盘驻留、开机自启、通知推送、文件关联，无缝融入桌面体验。" },
    { icon: "🌐", title: "国际化", desc: "中英文界面，设置持久化，语言切换即时生效。" },
    { icon: "📊", title: "实时进度", desc: "每线程独立进度条，实时下载速度，预估完成时间，状态一目了然。" },
  ];

  const techs = [
    { name: "Tauri 2", desc: "桌面框架" },
    { name: "React 19", desc: "前端" },
    { name: "Primer React", desc: "UI 组件" },
    { name: "Rust", desc: "后端" },
    { name: "Tokio", desc: "异步运行时" },
    { name: "reqwest", desc: "HTTP 客户端" },
    { name: "SQLite", desc: "存储引擎" },
    { name: "TypeScript", desc: "类型安全" },
  ];

  return (
    <>
      <style>{S.global}</style>

      {/* ── Nav ── */}
      <nav style={S.nav}>
        <div style={S.navLogo}>
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#238636" strokeWidth="2">
            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
          </svg>
          ProxyDM
        </div>
        <div style={S.navLinks}>
          <a href="https://github.com/fb0sh/ProxyDownloadManager/releases/latest" target="_blank" rel="noreferrer">
            <span style={S.navBtn(true)}>⬇️ 下载</span>
          </a>
          <a href="https://github.com/fb0sh/ProxyDownloadManager" target="_blank" rel="noreferrer">
            <span style={S.navBtn(false)}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/></svg>
              Source
            </span>
          </a>
        </div>
      </nav>

      {/* ── Hero ── */}
      <section style={S.hero}>
        <div className="anim-fade-up" style={{ maxWidth: 820, width: "100%" }}>
          <div style={{
            display: "inline-flex", alignItems: "center", gap: 6,
            padding: "4px 12px", borderRadius: 20, fontSize: 12, fontWeight: 600,
            background: "#23863622", color: "#3fb950", border: "1px solid #23863644",
            marginBottom: 20,
          }}>
            <span style={{ width: 6, height: 6, borderRadius: "50%", background: "#3fb950", display: "inline-block" }} />
            v0.5.0 — 最新版本
          </div>
          <h1 style={S.heroTitle}>
            多线程下载管理器
          </h1>
          <p style={S.heroSub}>
            ProxyDM 是一款开源的多线程下载工具，支持代理、断点续传、浏览器集成。
            基于 Rust + Tauri 构建，兼具性能与美观的桌面体验。
          </p>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: 12, flexWrap: "wrap" }}>
            <a href="https://github.com/fb0sh/ProxyDownloadManager/releases/latest" target="_blank" rel="noreferrer">
              <span style={S.heroCta}>
                ⬇️ 下载 ProxyDM
              </span>
            </a>
            <a href="https://github.com/fb0sh/ProxyDownloadManager" target="_blank" rel="noreferrer">
              <span style={{
                display: "inline-flex", alignItems: "center", gap: 8,
                padding: "12px 28px", borderRadius: 8, fontSize: 16, fontWeight: 600,
                background: "#21262d", color: "#e6edf3",
                border: "1px solid #30363d", cursor: "pointer", transition: "all 0.2s",
              }}>
                <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/></svg>
                查看源码
              </span>
            </a>
          </div>
        </div>

        {/* App mockup */}
        <div className="anim-fade-up-2" style={{ width: "100%", maxWidth: 860, marginTop: 48 }}>
          <MockAppWindow />
        </div>
      </section>

      {/* ── Features ── */}
      <section style={S.section} id="features">
        <div className="anim-fade-up" style={{ textAlign: "center" as const, marginBottom: 48 }}>
          <h2 style={S.sectionTitle}>功能特性</h2>
          <p style={{ ...S.sectionSub, margin: "8px auto 0" }}>
            从下载到管理，覆盖完整链路
          </p>
        </div>
        <div style={S.featureGrid}>
          {features.map((f, i) => (
            <FeatureCard key={f.title} {...f} idx={i} />
          ))}
        </div>
      </section>

      {/* ── Interactive Demo ── */}
      <section style={{ ...S.section, paddingTop: 40 }} id="demo">
        <div className="anim-fade-up" style={{ textAlign: "center" as const, marginBottom: 48 }}>
          <h2 style={S.sectionTitle}>实时体验</h2>
          <p style={{ ...S.sectionSub, margin: "8px auto 0" }}>
            模拟下载管理界面，体验真实操作流程
          </p>
        </div>
        <div className="anim-fade-up-2">
          <MockAppWindow />
        </div>
        {/* Speed stats */}
        <div className="anim-fade-up-3" style={{
          display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
          gap: 16, marginTop: 32,
        }}>
          {[
            { label: "最大线程", value: "32", unit: "per task" },
            { label: "代理协议", value: "HTTP / SOCKS5", unit: "" },
            { label: "支持平台", value: "macOS / Windows / Linux", unit: "" },
            { label: "开源许可", value: "MIT", unit: "" },
          ].map((s, i) => (
            <div key={i} className={`anim-fade-up-${i % 4}`} style={{
              padding: 20, borderRadius: 10,
              border: "1px solid #30363d", background: "#161b22",
              textAlign: "center" as const,
            }}>
              <div style={{ fontSize: 24, fontWeight: 700, color: "#58a6ff", marginBottom: 4 }}>{s.value}</div>
              <div style={{ fontSize: 13, color: "#8b949e" }}>{s.label}</div>
            </div>
          ))}
        </div>
      </section>

      {/* ── Tech Stack ── */}
      <section style={{ ...S.section, paddingTop: 40 }} id="tech">
        <div className="anim-fade-up" style={{ textAlign: "center" as const, marginBottom: 48 }}>
          <h2 style={S.sectionTitle}>技术栈</h2>
          <p style={{ ...S.sectionSub, margin: "8px auto 0" }}>
            现代技术栈，兼顾性能与开发效率
          </p>
        </div>
        <div className="anim-fade-up-1" style={{ ...S.techGrid, justifyContent: "center" }}>
          {techs.map(t => (
            <span key={t.name} style={S.techBadge}>
              <span style={{ fontWeight: 700 }}>{t.name}</span>
              <span style={{ color: "#8b949e" }}>— {t.desc}</span>
            </span>
          ))}
        </div>
        {/* Progress bars */}
        <div className="anim-fade-up-2" style={{ marginTop: 40, display: "flex", flexDirection: "column", gap: 12 }}>
          {[
            { label: "Rust 代码", pct: 65, color: "#f74c00" },
            { label: "TypeScript", pct: 25, color: "#3178c6" },
            { label: "HTML / CSS", pct: 10, color: "#e34c26" },
          ].map(s => (
            <div key={s.label}>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: 13, marginBottom: 4 }}>
                <span>{s.label}</span>
                <span style={{ color: "#8b949e" }}>{s.pct}%</span>
              </div>
              <AnimatedBar target={s.pct} color={s.color} />
            </div>
          ))}
        </div>
      </section>

      {/* ── CTA ── */}
      <section style={{ ...S.section, textAlign: "center" as const, paddingTop: 40, paddingBottom: 80 }}>
        <div className="anim-fade-up">
          <h2 style={{ ...S.sectionTitle, marginBottom: 16 }}>立即开始使用</h2>
          <p style={{ ...S.sectionSub, margin: "0 auto 32px" }}>
            免费、开源、跨平台。下载 ProxyDM，体验更高效的下载方式。
          </p>
          <a href="https://github.com/fb0sh/ProxyDownloadManager/releases/latest" target="_blank" rel="noreferrer">
            <span style={S.heroCta}>
              ⬇️ 下载 ProxyDM
            </span>
          </a>
        </div>
      </section>

      {/* ── Footer ── */}
      <footer style={S.footer}>
        <p>ProxyDM — 基于 MIT 许可开源</p>
        <p style={{ marginTop: 4 }}>
          <a href="https://github.com/fb0sh/ProxyDownloadManager" target="_blank" rel="noreferrer">GitHub</a>
          {" · "}
          <a href="https://github.com/fb0sh/ProxyDownloadManager/releases" target="_blank" rel="noreferrer">Releases</a>
          {" · "}
          Made with ❤️ by fb0sh & DohHoKun
        </p>
      </footer>
    </>
  );
}

export default App;
