let bypass = false;

function setBypass(on) {
  bypass = on;
}

window.addEventListener(
  "keydown",
  (e) => {
    if (e.altKey || e.key === "Delete" || e.key === "Alt") setBypass(true);
  },
  true
);
window.addEventListener(
  "keyup",
  (e) => {
    if (!e.altKey && e.key !== "Delete") setBypass(false);
  },
  true
);
window.addEventListener("blur", () => setBypass(false));

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg.action === "proxydm-bypass-state") {
    sendResponse({ bypass });
    return true;
  }
});
