import { t } from "./i18n.js";

const $ = (id) => document.getElementById(id);

$("lbl-capture").textContent = t("capture");
$("lbl-skip").textContent = t("skipNext");
$("skip-hint").textContent = t("skipHint");
$("lbl-media").textContent = t("media");
$("lbl-filters").textContent = t("filters");
$("lbl-minsize").textContent = t("minSize");
$("lbl-domains").textContent = t("ignoredDomains");
$("lbl-exts").textContent = t("ignoredExtensions");
$("bypass").textContent = t("once");
$("save").textContent = t("save");
$("ignoredDomains").placeholder = "cdn.example, static.foo";
$("ignoredExtensions").placeholder = "ico, svg, json";

async function downloadMedia(item) {
  $("error").hidden = true;
  try {
    const resp = await chrome.runtime.sendMessage({
      action: "download-media",
      url: item.url,
      contentType: item.contentType,
      tabId: item.tabId,
      referrer: item.referrer,
      size: item.size,
    });
    if (!resp?.ok) {
      $("error").hidden = false;
      $("error").textContent = t("downloadFailed");
    }
  } catch {
    $("error").hidden = false;
    $("error").textContent = t("downloadFailed");
  }
}

async function refresh() {
  const status = await chrome.runtime.sendMessage({ action: "popup-status" });
  const conn = $("conn");
  conn.innerHTML = status.connected
    ? `<span class="dot on"></span><span>${t("connected")}</span>`
    : `<span class="dot off"></span><span>${t("disconnected")}</span>`;
  $("toggle").textContent = status.enabled ? t("on") : t("off");
  $("count").textContent = String(status.mediaCount || 0);
  const list = $("media");
  list.innerHTML = "";
  (status.media || []).forEach((m) => {
    const li = document.createElement("li");
    const span = document.createElement("span");
    span.textContent = m.url;
    span.title = m.contentType || m.url;
    const btn = document.createElement("button");
    btn.textContent = t("download");
    const go = (e) => {
      e.preventDefault();
      e.stopPropagation();
      downloadMedia(m);
    };
    btn.addEventListener("click", go);
    li.addEventListener("click", go);
    li.append(span, btn);
    list.append(li);
  });
  if ((status.media || []).length === 0) {
    const li = document.createElement("li");
    li.style.cursor = "default";
    li.innerHTML = `<span class="muted">${t("noMedia")}</span>`;
    list.append(li);
  }
  const s = status.settings || {};
  $("minSize").value = s.minSize || 0;
  $("ignoredDomains").value = s.ignoredDomains || "";
  $("ignoredExtensions").value = s.ignoredExtensions || "";
}

$("toggle").onclick = async () => {
  const status = await chrome.runtime.sendMessage({ action: "popup-status" });
  await chrome.runtime.sendMessage({ action: "set-enabled", enabled: !status.enabled });
  refresh();
};

$("bypass").onclick = async () => {
  await chrome.runtime.sendMessage({ action: "bypass-next" });
  $("bypass").textContent = t("armed");
};

$("save").onclick = async () => {
  await chrome.runtime.sendMessage({
    action: "save-settings",
    settings: {
      minSize: Number($("minSize").value || 0),
      ignoredDomains: $("ignoredDomains").value,
      ignoredExtensions: $("ignoredExtensions").value,
    },
  });
  $("save").textContent = t("saved");
  setTimeout(() => { $("save").textContent = t("save"); }, 1200);
};

chrome.runtime.onMessage.addListener((msg) => {
  if (msg.action === "status-changed") refresh();
});

refresh();
