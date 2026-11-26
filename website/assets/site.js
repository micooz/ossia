import coverEn from './cover-en.png';
import coverZh from './cover-zh.png';
import libraryEn from './library-en.webp';
import libraryZh from './library-zh.webp';
import translations from './translations.json';

const OSSIA_FEED_ORIGIN = 'https://ossia-releases.maisow.com';
const GITHUB_RELEASES_PAGE = 'https://github.com/micooz/ossia/releases';
const INSTALLERS = {
  'darwin-arm64': 'ossia-darwin-arm64.dmg',
  'windows-amd64': 'ossia-windows-amd64.exe',
};
const localeImages = {
  cover: { en: coverEn, zh: coverZh },
  library: { en: libraryEn, zh: libraryZh },
};
const languageButton = document.querySelector('.language');
const meta = document.querySelector('meta[name="description"]');
const dialog = document.querySelector('.lightbox');
const dialogImage = dialog.querySelector('img');
const sourceButtons = [...document.querySelectorAll('[data-download-source]')];
const downloadLinks = [...document.querySelectorAll('[data-download-target]')];
const sourceHints = [...document.querySelectorAll('[data-source-hint]')];

let downloadSource = 'github';

function renderDownloads() {
  sourceButtons.forEach((button) => {
    const active = button.dataset.downloadSource === downloadSource;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  sourceHints.forEach((hint) => {
    hint.hidden = hint.dataset.sourceHint !== downloadSource;
  });
  downloadLinks.forEach((link) => {
    const target = link.dataset.downloadTarget;
    const file = INSTALLERS[target];
    link.href =
      downloadSource === 'ossia'
        ? `${OSSIA_FEED_ORIGIN}/${target}/latest/${file}`
        : `${GITHUB_RELEASES_PAGE}/latest/download/${file}`;
  });
}

function setLanguage(lang, save = false) {
  document.documentElement.lang = lang;
  document.querySelector('main').lang = lang;
  document.querySelectorAll('[data-i18n]').forEach((element) => {
    element.textContent = translations[element.dataset.i18n][lang];
  });
  for (const attribute of ['alt', 'aria-label']) {
    document.querySelectorAll(`[data-i18n-${attribute}]`).forEach((element) => {
      element.setAttribute(
        attribute,
        translations[element.getAttribute(`data-i18n-${attribute}`)][lang],
      );
    });
  }
  document.querySelectorAll('[data-locale-image]').forEach((image) => {
    image.src = localeImages[image.dataset.localeImage][lang];
    const link = image.closest('[data-lightbox]');
    if (link) link.href = image.src;
  });
  delete document.documentElement.dataset.translating;
  languageButton.textContent = lang === 'zh' ? 'English' : '简体中文';
  languageButton.setAttribute(
    'aria-label',
    lang === 'zh' ? 'Switch language to English' : '切换语言为简体中文',
  );
  document
    .querySelector('.nav')
    .setAttribute('aria-label', lang === 'zh' ? '主导航' : 'Main navigation');
  document.title =
    lang === 'zh' ? 'Ossia — AI 原生乐谱制作软件' : 'Ossia — AI-native music notation';
  meta.content =
    lang === 'zh'
      ? 'Ossia 是一款开源、本地优先的 AI 原生乐谱制作软件。用自然语言与 AI Agent 协作创作、改编、标注与打磨出版级乐谱。'
      : 'Ossia is open-source, local-first, AI-native music notation software. Compose, arrange, annotate, and refine engraved scores in conversation with your AI agent.';
  dialog.setAttribute('aria-label', lang === 'zh' ? '查看大图' : 'View image');
  dialog
    .querySelector('button')
    .setAttribute('aria-label', lang === 'zh' ? '关闭大图' : 'Close image');
  if (save) {
    try {
      localStorage.setItem('ossia-site-language', lang);
    } catch {}
  }
}
setLanguage(document.documentElement.lang);
renderDownloads();
sourceButtons.forEach((button) => {
  button.addEventListener('click', () => {
    downloadSource = button.dataset.downloadSource;
    renderDownloads();
  });
});
const oldAnchors = {
  download: 'download',
  mac: 'guide',
  macos: 'mac-install',
  windows: 'win',
  agent: 'agent',
  updates: 'updates',
  omr: 'omr',
  faq: 'faq',
  main: 'main',
};
function resolveAnchor() {
  const rawAnchor = location.hash.slice(1);
  const base = rawAnchor.replace(/-(en|zh)$/, '');
  const target = document.getElementById(oldAnchors[base] || base);
  if (target && target.id !== rawAnchor) {
    history.replaceState(null, '', '#' + target.id);
    requestAnimationFrame(() => target.scrollIntoView());
  }
}
resolveAnchor();
window.addEventListener('hashchange', resolveAnchor);
languageButton.addEventListener('click', () => {
  setLanguage(document.documentElement.lang === 'zh' ? 'en' : 'zh', true);
});
document.querySelectorAll('[data-lightbox]').forEach((link) =>
  link.addEventListener('click', (event) => {
    if (typeof dialog.showModal !== 'function') return;
    event.preventDefault();
    dialogImage.src = link.href;
    dialogImage.alt = link.querySelector('img')?.alt || '';
    dialog.showModal();
  }),
);
dialog.querySelector('button').addEventListener('click', () => dialog.close());
dialog.addEventListener('click', (event) => {
  if (event.target === dialog) dialog.close();
});
document.querySelectorAll('[data-copy]').forEach((button) =>
  button.addEventListener('click', async () => {
    const text = document.getElementById(button.dataset.copy).textContent.trim();
    const label = button.querySelector('[data-i18n]') ?? button;
    const original = label.textContent;
    try {
      if (navigator.clipboard && window.isSecureContext) await navigator.clipboard.writeText(text);
      else {
        const area = document.createElement('textarea');
        area.value = text;
        area.setAttribute('readonly', '');
        area.style.position = 'fixed';
        area.style.opacity = '0';
        document.body.append(area);
        area.select();
        const copied = document.execCommand('copy');
        area.remove();
        if (!copied) throw new Error('copy failed');
      }
      label.textContent = button.closest('main').lang === 'zh' ? '已复制' : 'Copied';
    } catch {
      label.textContent = button.closest('main').lang === 'zh' ? '复制失败' : 'Copy failed';
    }
    setTimeout(() => {
      label.textContent = label.dataset.i18n
        ? translations[label.dataset.i18n][document.documentElement.lang]
        : original;
    }, 1400);
  }),
);
const manualLinks = [...document.querySelectorAll('.manual-nav a[href^="#"]')];
const manualSections = manualLinks
  .map((link) => document.getElementById(link.getAttribute('href').slice(1)))
  .filter(Boolean);
if ('IntersectionObserver' in window && manualSections.length) {
  // Highlight the current guide section in the sticky sidebar while scrolling.
  const setActive = (id) => {
    manualLinks.forEach((link) => {
      const active = link.getAttribute('href') === `#${id}`;
      link.classList.toggle('is-active', active);
      if (active) link.setAttribute('aria-current', 'location');
      else link.removeAttribute('aria-current');
    });
  };
  const observer = new IntersectionObserver(
    (entries) => {
      const visible = entries
        .filter((entry) => entry.isIntersecting)
        .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
      if (visible[0]) setActive(visible[0].target.id);
    },
    { rootMargin: '-18% 0px -65% 0px' },
  );
  manualSections.forEach((section) => observer.observe(section));
}
