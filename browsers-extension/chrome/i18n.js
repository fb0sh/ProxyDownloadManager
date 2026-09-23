const en = {
  title: "ProxyDM",
  connected: "Connected",
  disconnected: "Disconnected",
  capture: "Capture downloads",
  on: "On",
  off: "Off",
  skipNext: "Skip next download",
  once: "Once",
  armed: "Armed",
  skipHint: "Hold Alt or Delete while clicking a download to use the browser once.",
  media: "Media on this tab",
  noMedia: "No media captured on this tab.",
  download: "Download",
  downloadFailed: "ProxyDM did not accept this download. Is the app running?",
  filters: "Filters",
  minSize: "Minimum size (bytes)",
  ignoredDomains: "Ignored domains",
  ignoredExtensions: "Ignored extensions",
  save: "Save filters",
  saved: "Saved",
  menuLink: "Download with ProxyDM",
  menuPage: "Download page with ProxyDM",
  menuSel: "Download selected link with ProxyDM",
  notifyOfflineTitle: "ProxyDM is not running",
  notifyOfflineBody: "Using the browser download instead. Start ProxyDM to capture downloads.",
  actionConnected: "ProxyDM connected",
  actionOffline: "ProxyDM enabled — desktop offline",
  actionDisabled: "ProxyDM disabled",
};

const zh = {
  title: "ProxyDM",
  connected: "已连接",
  disconnected: "未连接",
  capture: "拦截下载",
  on: "开",
  off: "关",
  skipNext: "跳过下一次下载",
  once: "一次",
  armed: "已就绪",
  skipHint: "按住 Alt 或 Delete 再点下载，可改用浏览器下载一次。",
  media: "当前标签页媒体",
  noMedia: "这个标签页还没有捕获到媒体。",
  download: "下载",
  downloadFailed: "ProxyDM 没有接手这次下载。请确认桌面端已启动。",
  filters: "过滤",
  minSize: "最小文件大小（字节）",
  ignoredDomains: "忽略的域名",
  ignoredExtensions: "忽略的扩展名",
  save: "保存过滤",
  saved: "已保存",
  menuLink: "使用 ProxyDM 下载",
  menuPage: "使用 ProxyDM 下载此页面",
  menuSel: "使用 ProxyDM 下载选中链接",
  notifyOfflineTitle: "ProxyDM 未运行",
  notifyOfflineBody: "已改用浏览器下载。启动 ProxyDM 后即可接管。",
  actionConnected: "ProxyDM 已连接",
  actionOffline: "ProxyDM 已开启 — 桌面端离线",
  actionDisabled: "ProxyDM 已关闭",
};

function uiLang() {
  const raw = (typeof chrome !== "undefined" && chrome.i18n?.getUILanguage?.()) ||
    (typeof navigator !== "undefined" && navigator.language) ||
    "en";
  return String(raw).toLowerCase().startsWith("zh") ? "zh" : "en";
}

const dict = uiLang() === "zh" ? zh : en;

export function t(key) {
  return dict[key] || en[key] || key;
}
