'use strict';
/* ============ Tauri 桥接与共享工具（桌面版） ============ */
const T = window.__TAURI__;
const invoke = T.core.invoke;
const tListen = T.event.listen;
const tEmit = T.event.emit;
const tEmitTo = T.event.emitTo;
const curWin = T.window.getCurrentWindow();

const MY_ORIGIN = document.documentElement.dataset.win || 'unknown';

const $id = id => document.getElementById(id);
const qs = (s, r) => (r || document).querySelector(s);
const el = (tag, cls, html) => { const e = document.createElement(tag); if (cls) e.className = cls; if (html !== undefined) e.innerHTML = html; return e; };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const clone = o => JSON.parse(JSON.stringify(o));
const esc = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const cut = (s, n) => (String(s).length > n ? String(s).slice(0, n) + '…' : String(s));
const fmtTime = ts => { const d = new Date(ts); const p = x => String(x).padStart(2, '0'); return p(d.getHours()) + ':' + p(d.getMinutes()); };
const fmtDT = ts => { const d = new Date(ts); const p = x => String(x).padStart(2, '0'); return (d.getMonth() + 1) + '月' + d.getDate() + '日 ' + p(d.getHours()) + ':' + p(d.getMinutes()); };
const uid = p => p + '-' + Date.now().toString(36) + Math.random().toString(36).slice(2, 7);
const clampNum = (v, lo, hi, dft) => { const n = parseFloat(v); if (isNaN(n)) return dft; return Math.max(lo, Math.min(hi, n)); };

/* ---- 状态（主进程为权威数据源） ---- */
let S = null;
async function loadState() { S = await invoke('state_load'); return S; }
let saveTimer = null;
async function saveState() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try { await invoke('state_save', { state: S, origin: MY_ORIGIN }); } catch (e) { toast('保存失败：' + e, 'err'); }
  }, 200);
}
const getProv = id => S.providers.find(p => p.id === id);
const getPers = id => S.personas.find(p => p.id === id);
const defProvObj = () => S.providers.find(p => p.isDefault) || S.providers[0];
const curSession = () => S.sessions.find(s => s.id === S.currentSessionId);
function activeProv() { const s = curSession(); const id = (s && s.providerId) || S.settings.lastProviderId; return getProv(id) || defProvObj(); }
function activePers() { const s = curSession(); const id = (s && s.personaId) || S.settings.lastPersonaId; return getPers(id) || S.personas[0]; }

/* ---- Toast ---- */
function ensureToasts() { if (!$id('toasts')) { const d = el('div'); d.id = 'toasts'; document.body.appendChild(d); } }
function toast(msg, type) {
  ensureToasts();
  const box = $id('toasts');
  while (box.children.length >= 3) box.removeChild(box.firstChild);
  const t = el('div', 'toast' + (type ? ' ' + type : ''), esc(msg));
  box.appendChild(t);
  setTimeout(() => { t.style.transition = 'opacity .3s'; t.style.opacity = '0'; setTimeout(() => t.remove(), 320); }, 2600);
}

/* ---- HTML 上下文菜单 ---- */
function hideMenu() { const r = $id('menuRoot'); if (!r) return; r.classList.remove('show'); r.innerHTML = ''; document.removeEventListener('mousedown', menuOutside); }
function menuOutside(e) { if (!e.target.closest('.ctxmenu')) hideMenu(); }
function showMenu(items, x, y) {
  hideMenu();
  let root = $id('menuRoot');
  if (!root) { root = el('div'); root.id = 'menuRoot'; document.body.appendChild(root); }
  const box = el('div', 'ctxmenu');
  items.forEach(it => {
    if (it === '-') { box.appendChild(el('div', 'cm-div')); return; }
    const b = el('button', 'cm-item' + (it.danger ? ' danger' : ''));
    b.innerHTML = (it.icon ? '<span class="cm-ic">' + it.icon + '</span>' : '') + '<span>' + esc(it.label) + '</span>' + (it.check ? '<span class="cm-check">✓</span>' : '');
    b.onclick = () => { hideMenu(); it.onClick && it.onClick(); };
    box.appendChild(b);
  });
  root.appendChild(box);
  root.classList.add('show');
  const r = box.getBoundingClientRect();
  box.style.left = Math.max(6, Math.min(x, innerWidth - r.width - 8)) + 'px';
  box.style.top = Math.max(6, Math.min(y, innerHeight - r.height - 8)) + 'px';
  setTimeout(() => document.addEventListener('mousedown', menuOutside), 0);
}

/* ---- Markdown 渲染（无外部依赖） ---- */
function md(src) {
  const blocks = [];
  src = String(src).replace(/```(\w*)\n?([\s\S]*?)```/g, (m, lang, code) => {
    blocks.push({ lang: lang || 'text', code: code.replace(/\n$/, '') });
    return '\x00B' + (blocks.length - 1) + '\x00';
  });
  const lines = esc(src).split('\n');
  const out = [];
  let para = [], list = null, quote = false;
  const flushP = () => { if (para.length) { out.push('<p>' + para.join('<br>') + '</p>'); para = []; } };
  const flushL = () => { if (list) { out.push('</' + list + '>'); list = null; } };
  const flushQ = () => { if (quote) { out.push('</blockquote>'); quote = false; } };
  const flushAll = () => { flushP(); flushL(); flushQ(); };
  const inline = s => s
    .replace(/`([^`\n]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>')
    .replace(/\*([^*\n]+)\*/g, '<i>$1</i>')
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
  for (let raw of lines) {
    const line = raw.trimEnd();
    const bm = line.match(/^\x00B(\d+)\x00$/);
    if (bm) { flushAll(); out.push('\x00B' + bm[1] + '\x00'); continue; }
    if (!line.trim()) { flushAll(); continue; }
    if (/^---+$/.test(line.trim())) { flushAll(); out.push('<hr>'); continue; }
    const h = line.match(/^(#{1,3})\s+(.*)/);
    if (h) { flushAll(); out.push('<h' + (h[1].length + 1) + '>' + inline(h[2]) + '</h' + (h[1].length + 1) + '>'); continue; }
    if (/^&gt;\s?/.test(line)) { flushP(); flushL(); if (!quote) { out.push('<blockquote>'); quote = true; } out.push('<p>' + inline(line.replace(/^&gt;\s?/, '')) + '</p>'); continue; }
    if (quote) flushQ();
    const ul = line.match(/^[-*]\s+(.*)/);
    const ol = line.match(/^\d+[.、]\s+(.*)/);
    if (ul) { flushP(); if (list !== 'ul') { flushL(); out.push('<ul>'); list = 'ul'; } out.push('<li>' + inline(ul[1]) + '</li>'); continue; }
    if (ol) { flushP(); if (list !== 'ol') { flushL(); out.push('<ol>'); list = 'ol'; } out.push('<li>' + inline(ol[1]) + '</li>'); continue; }
    if (list) flushL();
    para.push(inline(line));
  }
  flushAll();
  let html = out.join('\n');
  html = html.replace(/\x00B(\d+)\x00/g, (m, i) => {
    const b = blocks[+i];
    return '<div class="codeblock"><div class="cb-bar"><span>' + esc(b.lang) + '</span><button class="cb-copy">复制</button></div><pre><code>' + esc(b.code) + '</code></pre></div>';
  });
  return html;
}

/* ---- 自然语言提醒解析 ---- */
function parseReminder(text) {
  const t = String(text || '').replace(/\s+/g, ' ').trim();
  const rel = t.match(/(\d+(?:\.\d+)?)\s*(秒钟?|分钟?|小时|时|天)\s*后\s*(.*)/);
  if (rel) {
    const n = parseFloat(rel[1]);
    const unit = { '秒': 1000, '秒钟': 1000, '分': 60000, '分钟': 60000, '小时': 3600000, '时': 3600000, '天': 86400000 }[rel[2]];
    let content = rel[3].replace(/^(?:提醒|通知|叫)\s*我?\s*/, '').replace(/^(?:去|来)\s*/, '').trim();
    if (!content) content = '提醒';
    return { id: uid('rem'), content: cut(content, 40), due: Date.now() + Math.round(n * unit), rule: 'once', enabled: true };
  }
  const daily = t.match(/每天\s*(\d{1,2})[:：点](\d{1,2})?分?\s*(.*)/);
  if (daily) {
    const h = Math.min(23, +daily[1]), m = Math.min(59, +(daily[2] || 0));
    let d = new Date(); d.setHours(h, m, 0, 0);
    if (d.getTime() <= Date.now()) d = new Date(d.getTime() + 86400000);
    const content = daily[3].replace(/^(?:提醒|通知|叫)\s*我?\s*/, '').trim() || '提醒';
    return { id: uid('rem'), content: cut(content, 40), due: d.getTime(), rule: 'daily', enabled: true };
  }
  const at = t.match(/(今天|明天|今晚)?\s*(\d{1,2})[:：点](\d{1,2})?分?\s*(.*)/);
  if (at && (at[1] || /提醒/.test(t))) {
    const h = Math.min(23, +at[2]), m = Math.min(59, +(at[3] || 0));
    let d = new Date(); d.setHours(h, m, 0, 0);
    if (at[1] === '明天') d = new Date(d.getTime() + 86400000);
    else if (d.getTime() <= Date.now()) d = new Date(d.getTime() + 86400000);
    const content = at[4].replace(/^(?:提醒|通知|叫)\s*我?\s*/, '').trim() || '提醒';
    return { id: uid('rem'), content: cut(content, 40), due: d.getTime(), rule: 'once', enabled: true };
  }
  return null;
}

/* ---- 常量 ---- */
const SKINS = {
  blue: { label: '团子蓝', fur: '#9CC3F7', belly: '#F3F8FF' },
  orange: { label: '橘猫', fur: '#F5B06E', belly: '#FDEEDD' },
  mint: { label: '薄荷', fur: '#93D6BD', belly: '#EDF9F4' },
};
const PROVIDER_TEMPLATES = [
  { key: 'mock', label: '演示模式（内置，无需 Key）', type: 'mock', baseUrl: '', model: 'demo-pet-v1' },
  { key: 'openai', label: 'OpenAI', type: 'openai', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  { key: 'deepseek', label: 'DeepSeek', type: 'openai', baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  { key: 'moonshot', label: 'Moonshot Kimi', type: 'openai', baseUrl: 'https://api.moonshot.cn/v1', model: 'moonshot-v1-8k' },
  { key: 'zhipu', label: '智谱 GLM', type: 'openai', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4-flash' },
  { key: 'qwen', label: '通义千问', type: 'openai', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-plus' },
  { key: 'azure', label: 'Azure OpenAI', type: 'openai', baseUrl: 'https://你的资源名.openai.azure.com/openai/deployments/部署名', model: 'gpt-4o' },
  { key: 'anthropic', label: 'Anthropic Claude', type: 'anthropic', baseUrl: 'https://api.anthropic.com/v1', model: 'claude-sonnet-4-5' },
  { key: 'gemini', label: 'Google Gemini', type: 'gemini', baseUrl: 'https://generativelanguage.googleapis.com/v1beta', model: 'gemini-2.0-flash' },
  { key: 'ollama', label: 'Ollama（本地）', type: 'openai', baseUrl: 'http://localhost:11434/v1', model: 'qwen2.5:7b' },
  { key: 'lmstudio', label: 'LM Studio（本地）', type: 'openai', baseUrl: 'http://localhost:1234/v1', model: 'local-model' },
  { key: 'vllm', label: 'vLLM（自建）', type: 'openai', baseUrl: 'http://127.0.0.1:8000/v1', model: '你的模型名' },
  { key: 'custom', label: '自定义 OpenAI 兼容端点', type: 'openai', baseUrl: '', model: '' },
];

/* ---- 复制 ---- */
function copyText(t) {
  const done = () => toast('已复制到剪贴板', 'ok');
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(t).then(done).catch(() => fallbackCopy(t, done));
  } else fallbackCopy(t, done);
}
function fallbackCopy(t, done) {
  const ta = document.createElement('textarea');
  ta.value = t; ta.style.position = 'fixed'; ta.style.opacity = '0';
  document.body.appendChild(ta); ta.select();
  try { document.execCommand('copy'); done(); } catch (e) { toast('复制失败', 'err'); }
  ta.remove();
}

/* ---- 记忆相关度选取（注入不超过 5 条，FR-MEM-002） ---- */
function pickMemories(text) {
  if (!S.memories || !S.memories.length) return [];
  const terms = String(text || '').split(/[\s，。、！？!?.,]+/).filter(w => w.length > 1);
  const now = Date.now();
  const scored = S.memories.map((m) => {
    let sc = 0;
    for (const w of terms) if (m.content.includes(w)) sc += 2;
    sc += Math.max(0, 1 - (now - m.ts) / (1000 * 60 * 60 * 24 * 30));
    return { m, sc };
  }).sort((a, b) => b.sc - a.sc);
  return scored.slice(0, 5).map(x => x.m);
}

/* ---- 会话导出为 Markdown ---- */
function sessionToMd(s) {
  const pers = getPers(s.personaId);
  const prov = getProv(s.providerId);
  const lines = ['# ' + (s.title || '未命名会话'), '', '- 人格：' + (pers ? pers.name : '（人格已删除）'), '- 模型：' + (prov ? prov.name + ' / ' + prov.model : '-'), '- 时间：' + new Date(s.createdAt).toLocaleString('zh-CN'), '', '---', ''];
  s.msgs.forEach(m => {
    if (m.role === 'user') lines.push('## 🧑 用户', '', m.content, '');
    else if (m.role === 'assistant') lines.push('## 🐱 ' + (pers ? pers.name : 'AI'), '', m.content, '');
    else if (m.role === 'error') lines.push('> ⚠ ' + (m.title || '错误') + '：' + (m.content || ''), '');
    else lines.push('> ' + m.content, '');
  });
  return lines.join('\n');
}
