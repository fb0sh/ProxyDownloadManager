const $ = (id) => document.getElementById(id);

async function refresh() {
  const status = await chrome.runtime.sendMessage({ action: "popup-status" });
  const conn = $("conn");
  conn.innerHTML = status.connected
    ? '<span class="dot on"></span><span>Connected</span>'
    : '<span class="dot off"></span><span>Disconnected</span>';
  $("toggle").textContent = status.enabled ? "On" : "Off";
  $("count").textContent = String(status.mediaCount || 0);
  const list = $("media");
  list.innerHTML = "";
  (status.media || []).forEach((m) => {
    const li = document.createElement("li");
    const span = document.createElement("span");
    span.textContent = m.url;
    span.title = m.contentType || "";
    const btn = document.createElement("button");
    btn.textContent = "Download";
    btn.onclick = async () => {
      await chrome.runtime.sendMessage({ action: "download-media", url: m.url, contentType: m.contentType });
    };
    li.append(span, btn);
    list.append(li);
  });
  if ((status.media || []).length === 0) {
    const li = document.createElement("li");
    li.innerHTML = '<span class="muted">No media captured on this tab.</span>';
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
  $("bypass").textContent = "Armed";
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
  $("save").textContent = "Saved";
  setTimeout(() => { $("save").textContent = "Save filters"; }, 1200);
};

chrome.runtime.onMessage.addListener((msg) => {
  if (msg.action === "status-changed") refresh();
});

refresh();
