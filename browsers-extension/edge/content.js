// =============================================================================
// content.js — Detects download links, adds ⬇ badge
// =============================================================================

const EXTS = ['.zip','.rar','.7z','.tar','.gz','.xz','.bz2','.zst',
  '.exe','.msi','.dmg','.pkg','.apk','.deb','.rpm',
  '.iso','.img','.pdf','.epub',
  '.mp4','.mkv','.avi','.mov','.webm','.m4v',
  '.mp3','.flac','.wav','.ogg','.opus','.m4a',
  '.jar','.war','.nupkg',
  '.tar.gz','.tar.xz','.tar.bz2','.tgz','.txz'];

function isDownload(href) {
  if (!href) return false;
  return EXTS.some(e => href.split(/[?#]/)[0].toLowerCase().endsWith(e));
}

function addBadge(link) {
  if (link.dataset.pd) return;
  link.dataset.pd = '1';
  const badge = document.createElement('span');
  badge.textContent = '⬇';
  badge.title = 'Download with ProxyDM';
  badge.style.cssText = 'display:inline-block;margin-left:4px;font-size:12px;cursor:pointer;opacity:0.7';
  badge.onclick = (e) => {
    e.preventDefault(); e.stopPropagation();
    const url = link.href || link.getAttribute('href');
    if (url) chrome.runtime.sendMessage({ action: 'sendUrl', url });
  };
  link.after(badge);
}

function scan() {
  document.querySelectorAll('a[href]').forEach(a => {
    if (isDownload(a.href)) addBadge(a);
  });
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', scan);
} else { scan(); }
new MutationObserver(() => scan()).observe(document.body, { childList: true, subtree: true });
