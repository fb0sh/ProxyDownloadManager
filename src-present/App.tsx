import { useEffect, useMemo, useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { setLanguage, t } from "../src/i18n";
import {
  usePauseDownload,
  useResumeDownload,
  useDownloads,
  useSettings,
  useRedownloadDownload,
} from "../src/query/downloadQueries";
import Layout from "../src/components/Layout";
import DialogRenderer from "../src/components/DialogRenderer";
import NewDownloadWindow from "../src/NewDownloadWindow";
import DownloadDetailsWindow from "../src/DownloadDetailsWindow";
import DemoWindow from "./DemoWindow";
import { emitToListeners } from "./tauri-mocks";
import { AppProvider, useAppContext, type AppActions } from "../src/contexts/AppContext";
import { useDialog } from "../src/hooks/useDialog";
import { useSelection } from "../src/hooks/useSelection";
import { isFailed } from "../src/utils/download";
import type { DownloadItem } from "../src/types";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
});

const RELEASE = "https://github.com/fb0sh/ProxyDownloadManager/releases/latest";
const SOURCE = "https://github.com/fb0sh/ProxyDownloadManager";
const VERSION = "0.13.2";

type ExtraDialog =
  | { type: "newDownload" }
  | { type: "properties"; id: number }
  | null;

function DemoInner({ extra, setExtra }: { extra: ExtraDialog; setExtra: (d: ExtraDialog) => void }) {
  const { dialog, dialogActions, selectedIds } = useAppContext();
  const { data: downloads = [] } = useDownloads();
  const { settings } = useSettings();

  useEffect(() => {
    if (settings) setLanguage(settings.language || "zh");
  }, [settings]);

  const selectedForRedownload = selectedIds.size === 1
    ? downloads.find((d) => selectedIds.has(d.id) && (d.status === "completed" || isFailed(d.status)))
    : undefined;

  return (
    <div className="demo-app h-full">
      <Layout className="h-full" onRedownloadItem={selectedForRedownload} />
      <DialogRenderer
        dialog={dialog}
        onClose={() => dialogActions.closeDialog()}
        onDownloadUpdate={() => {}}
      />
      {extra?.type === "newDownload" && (
        <DemoWindow
          title={t("newDownload.title")}
          width={640}
          height={560}
          onClose={() => setExtra(null)}
        >
          <NewDownloadWindow />
        </DemoWindow>
      )}
      {extra?.type === "properties" && (
        <DemoWindow
          title={t("properties.title")}
          width={460}
          height={520}
          onClose={() => setExtra(null)}
        >
          <DetailsWindowHost id={extra.id} />
        </DemoWindow>
      )}
    </div>
  );
}

/**
 * DownloadDetailsWindow resolves its id from the query string, or from the
 * `details-id` event the desktop app emits when it reuses an open window.
 * The demo has no query string, so replay that event once the window's own
 * listener has attached (a macrotask later — the listener registers after an
 * async module import).
 */
function DetailsWindowHost({ id }: { id: number }) {
  useEffect(() => {
    const handle = window.setTimeout(() => { emitToListeners("details-id", id); }, 0);
    return () => window.clearTimeout(handle);
  }, [id]);
  return <DownloadDetailsWindow />;
}

function DemoApp() {
  const dialog = useDialog();
  const selection = useSelection();
  const [filter, setFilter] = useState<"all" | "downloading" | "completed" | "incomplete">("all");
  const [extra, setExtra] = useState<ExtraDialog>(null);
  const pauseDownload = usePauseDownload();
  const resumeDownload = useResumeDownload();
  const redownloadDownload = useRedownloadDownload();
  const { data: downloads = [] } = useDownloads();

  const actions: AppActions = useMemo(() => ({
    onNewDownload: () => setExtra({ type: "newDownload" }),
    onExtension: () => dialog.openExtension(),
    onLog: () => dialog.openLog(),
    onSettings: () => dialog.openSettings(),
    onAbout: () => dialog.openAbout(),
    onQuit: () => {},
    onResumeSelected: () => {
      downloads.filter((d) => selection.selectedIds.has(d.id) && d.status === "paused")
        .forEach((d) => resumeDownload.mutate(d.id));
    },
    onPauseSelected: () => {
      downloads.filter((d) => selection.selectedIds.has(d.id) && d.status === "downloading")
        .forEach((d) => pauseDownload.mutate(d.id));
    },
    onDeleteSelected: () => {
      if (selection.selectedIds.size) dialog.openDelete(Array.from(selection.selectedIds));
    },
    onStop: (id: number) => pauseDownload.mutate(id),
    onDelete: (ids: number[]) => dialog.openDelete(ids),
    onProperties: (id: number) => setExtra({ type: "properties", id }),
    onRedownload: async (item: DownloadItem) => {
      try { await redownloadDownload.mutateAsync(item.id); } catch { /* demo */ }
    },
  }), [dialog, downloads, selection.selectedIds, pauseDownload, resumeDownload, redownloadDownload]);

  return (
    <AppProvider
      dialog={dialog.dialog}
      dialogActions={dialog}
      selectedIds={selection.selectedIds}
      selectionActions={selection}
      filter={filter}
      setFilter={setFilter}
      actions={actions}
    >
      <DemoInner extra={extra} setExtra={setExtra} />
    </AppProvider>
  );
}

function Spec({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-[#e5e5e5] py-3">
      <dt className="text-[13px] text-[#737373]">{k}</dt>
      <dd className="site-mono text-right text-[13px] font-medium">{v}</dd>
    </div>
  );
}

function Page() {
  return (
    <div className="site text-[#171717]">
      <header className="sticky top-0 z-20 flex h-14 items-center justify-between border-b border-[#e5e5e5] bg-[#fafafa]/95 px-5 backdrop-blur">
        <a href="#top" className="flex items-center gap-2 no-underline">
          <img src="./proxydm-icon.png" alt="" width={22} height={22} />
          <span className="site-display text-[14px] font-semibold tracking-tight">ProxyDM</span>
        </a>
        <nav className="flex items-center gap-2 text-[13px]">
          <a href="#try" className="px-3 py-1.5 text-[#737373] no-underline hover:text-[#171717]">试用</a>
          <a href={SOURCE} target="_blank" rel="noreferrer" className="px-3 py-1.5 text-[#737373] no-underline hover:text-[#171717]">源码</a>
          <a href={RELEASE} target="_blank" rel="noreferrer" className="bg-[#171717] px-3 py-1.5 text-[#fafafa] no-underline">
            下载 {VERSION}
          </a>
        </nav>
      </header>

      <section className="ledger px-5 pb-16 pt-16 md:pt-24" id="top">
        <div className="mx-auto max-w-[1100px]">
          <p className="site-mono mb-4 text-[12px] tracking-wide text-[#737373]">
            OPEN SOURCE · v{VERSION} · MAC / WIN / LINUX
          </p>
          <h1 className="site-display max-w-[18ch] text-[40px] font-semibold leading-[1.12] tracking-tight md:text-[56px]">
            每个下载走哪条代理，你说了算。
          </h1>
          <p className="mt-5 max-w-[42em] text-[16px] leading-7 text-[#525252]">
            ProxyDownloadManager 是桌面下载器。任务可各自选 HTTP / HTTPS CONNECT / SOCKS5，
            最高 64 连接，浏览器点击即拦截，中断后续上。不读剪贴板，不往云上送文件。
          </p>
          <div className="mt-8 flex flex-wrap gap-3">
            <a href={RELEASE} target="_blank" rel="noreferrer" className="bg-[#171717] px-4 py-2 text-[14px] text-[#fafafa] no-underline">
              下载桌面端
            </a>
            <a href="#try" className="border border-[#e5e5e5] bg-white px-4 py-2 text-[14px] text-[#171717] no-underline">
              先在浏览器里点一点
            </a>
          </div>
        </div>
      </section>

      <section id="try" className="border-t border-[#e5e5e5] bg-white px-5 py-14">
        <div className="mx-auto max-w-[1100px]">
          <div className="mb-6 flex flex-wrap items-end justify-between gap-3">
            <div>
              <h2 className="site-display text-[22px] font-semibold">真实界面</h2>
              <p className="mt-1 text-[14px] text-[#737373]">下面就是当前版本的主窗口。暂停、筛选、设置都可以点。</p>
            </div>
            <p className="site-mono text-[12px] text-[#737373]">演示数据 · 不会真的下载</p>
          </div>
          <div className="demo-frame shadow-[0_24px_60px_-28px_rgba(0,0,0,0.35)]">
            <div className="demo-chrome flex items-center gap-2 border-b border-[#e5e5e5] bg-[#f5f5f5] px-3">
              <span className="size-2.5 rounded-full bg-[#d4d4d4]" />
              <span className="size-2.5 rounded-full bg-[#d4d4d4]" />
              <span className="size-2.5 rounded-full bg-[#d4d4d4]" />
              <span className="site-mono mx-auto text-[11px] text-[#737373]">ProxyDownloadManager {VERSION}</span>
            </div>
            <div className="demo-body">
              <DemoApp />
            </div>
          </div>
        </div>
      </section>

      <section className="border-t border-[#e5e5e5] px-5 py-14">
        <div className="mx-auto grid max-w-[1100px] gap-12 md:grid-cols-[1fr_1fr]">
          <div>
            <h2 className="site-display text-[22px] font-semibold">怎么用</h2>
            <ol className="mt-6 space-y-5 text-[15px] leading-6">
              <li>
                <span className="site-mono text-[12px] text-[#737373]">01</span>
                <div className="font-medium">浏览器里点下载，或右键「用 ProxyDM 下载」。</div>
                <div className="text-[#737373]">扩展把 Cookie / Referer 交给桌面端；复制链接不会弹窗。</div>
              </li>
              <li>
                <span className="site-mono text-[12px] text-[#737373]">02</span>
                <div className="font-medium">选代理、连接数，开始下。</div>
                <div className="text-[#737373]">3 秒探不到文件信息，会改走默认代理再试。</div>
              </li>
              <li>
                <span className="site-mono text-[12px] text-[#737373]">03</span>
                <div className="font-medium">详情窗看每条连接。</div>
                <div className="text-[#737373]">#1 #2 黑色进度条，暂停、限速、刷新过期链接都在这里。</div>
              </li>
            </ol>
          </div>
          <dl>
            <Spec k="每任务代理" v="HTTP / HTTPS / SOCKS5" />
            <Spec k="并发连接" v="1–64，Auto 按体积" />
            <Spec k="断点续传" v="Range + 崩溃恢复" />
            <Spec k="浏览器" v="Chrome / Edge / Firefox" />
            <Spec k="限速" v="全局与单任务" />
            <Spec k="界面" v="中 / 英" />
          </dl>
        </div>
      </section>

      <section className="border-t border-[#e5e5e5] bg-white px-5 py-10">
        <div className="mx-auto flex max-w-[1100px] flex-wrap items-center justify-between gap-4">
          <p className="site-mono text-[12px] text-[#737373]">
            TAURI 2 · RUST · REACT 19 · SHADCN/UI
          </p>
          <a href={RELEASE} target="_blank" rel="noreferrer" className="bg-[#171717] px-4 py-2 text-[14px] text-[#fafafa] no-underline">
            去 Releases 下载
          </a>
        </div>
      </section>

      <footer className="border-t border-[#e5e5e5] px-5 py-6 text-[13px] text-[#737373]">
        <div className="mx-auto flex max-w-[1100px] flex-wrap justify-between gap-2">
          <span>ProxyDownloadManager · MIT</span>
          <span>
            <a href={SOURCE} className="text-[#171717] no-underline" target="_blank" rel="noreferrer">GitHub</a>
            {" · "}
            fb0sh
          </span>
        </div>
      </footer>
    </div>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <Page />
    </QueryClientProvider>
  );
}
