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

/* ---- 全局错误兜底：单个页面脚本异常不应让窗口静默失效 ---- */
let lastErrToast = 0;
function reportClientError(where, err) {
  const msg = (err && (err.message || err.reason || err)) || '未知错误';
  const now = Date.now();
  if (now - lastErrToast < 1500) return; // 避免同一错误循环刷屏
  lastErrToast = now;
  try { toast(where + '：' + msg, 'err'); } catch (e) { }
}
window.addEventListener('error', e => reportClientError('界面异常', e.error || e.message));
window.addEventListener('unhandledrejection', e => reportClientError('操作失败', e.reason));

/* ---- 粗粒度 token 估算（CJK 约 1 字 1 token，其余约 4 字符 1 token） ---- */
function estTokens(text) {
  const t = String(text || '');
  let cjk = 0;
  for (const ch of t) {
    if (/[\u2e80-\u9fff\u3000-\u303f\uff00-\uffef]/.test(ch)) cjk++;
  }
  return cjk + Math.ceil((t.length - cjk) / 4);
}

/* ---- 状态（主进程为权威数据源） ---- */
let S = null;
async function loadState() { S = await invoke('state_load'); return S; }
let saveTimer = null;
let savePending = false;
async function saveState() {
  clearTimeout(saveTimer);
  savePending = true;
  saveTimer = setTimeout(async () => {
    try { await invoke('state_save', { state: S, origin: MY_ORIGIN }); }
    catch (e) { toast('保存失败：' + e, 'err'); }
    finally { savePending = false; }
  }, 200);
}
// 需要与后端命令严格排序时使用（导入配置、批量应用设置）
async function saveStateNow() {
  clearTimeout(saveTimer);
  savePending = true;
  try { await invoke('state_save', { state: S, origin: MY_ORIGIN }); }
  catch (e) { toast('保存失败：' + e, 'err'); throw e; }
  finally { savePending = false; }
}
const getProv = id => S.providers.find(p => p.id === id);
const getPers = id => S.personas.find(p => p.id === id);
const defProvObj = () => S.providers.find(p => p.isDefault) || S.providers[0];
const curSession = () => S.sessions.find(s => s.id === S.currentSessionId);
function activeProv() { const s = curSession(); const id = (s && s.providerId) || S.settings.lastProviderId; return getProv(id) || defProvObj(); }
function activePers() { const s = curSession(); const id = (s && s.personaId) || S.settings.lastPersonaId; return getPers(id) || S.personas[0]; }

/* ---- 人格 / 皮肤联动（FR-PSN-005） ----
   切换人格：会话与全局默认同步，并自动切换到该人格绑定的供应商与皮肤；
   切换皮肤：若存在绑定该皮肤的人格，则一并切换人格（含供应商），否则只改皮肤。
   两者互不递归。 */
function switchPersona(id) {
  const ps = getPers(id);
  if (!ps) return null;
  const s = curSession();
  S.settings.lastPersonaId = ps.id;
  if (s) {
    s.personaId = ps.id;
    // FR-PSN-003：尚未开始对话时同步新人格的开场白
    if (ps.greeting && !s.msgs.some(m => m.role === 'user')) {
      s.msgs = [{ role: 'assistant', content: ps.greeting, ts: Date.now() }];
    }
  }
  if (ps.providerId && getProv(ps.providerId)) {
    S.settings.lastProviderId = ps.providerId;
    if (s) s.providerId = ps.providerId;
  }
  if (ps.skin && SKINS[ps.skin]) S.settings.skin = ps.skin;
  return ps;
}
function switchSkin(name) {
  if (!SKINS[name]) name = 'blue';
  const bound = S.personas.find(p => p.skin === name);
  if (bound) return switchPersona(bound.id);
  S.settings.skin = name;
  return null;
}

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
    return '<div class="codeblock"><div class="cb-bar"><span>' + esc(b.lang) + '</span><button class="cb-copy">复制</button></div><pre><code>' + highlightCode(b.code) + '</code></pre></div>';
  });
  return html;
}

/* ---- 轻量代码高亮（无外部依赖，仅用于代码块展示） ---- */
function highlightCode(code) {
  const source = esc(code);
  const re = /(&quot;[^\n]*?&quot;|'[^'\n]*'|`[^`\n]*`|\/\/[^\n]*|\/\*[\s\S]*?\*\/|#[^\n]*|\b\d+(?:\.\d+)?\b|\b(?:function|const|let|var|if|else|for|while|return|class|import|from|export|async|await|def|print|true|false|null|undefined|None|True|False|new|this|try|catch|throw|switch|case|break|continue|in|of|typeof|instanceof|public|private|static|void|int|float|string|bool|struct|impl|fn|use|mod|pub|match|mut|self|super)\b)/g;
  return source.replace(re, m => {
    if (/^&quot;|^'|^`/.test(m)) return '<span class="tok-str">' + m + '</span>';
    if (/^\/\/|^\/\*|^#/.test(m)) return '<span class="tok-com">' + m + '</span>';
    if (/^\d/.test(m)) return '<span class="tok-num">' + m + '</span>';
    return '<span class="tok-kw">' + m + '</span>';
  });
}

/* ---- 自然语言提醒解析 ---- */
function parseReminder(text) {
  const t = String(text || '').replace(/\s+/g, ' ').trim();
  const rel = t.match(/(\d+(?:\.\d+)?)\s*(秒钟?|分钟?|小时|时|天)\s*后\s*(.*)/);
  if (rel && !/每隔?/.test(t)) {
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
  if (at && (at[1] || /提醒/.test(t)) && !/每(?:周|星期|隔)/.test(t)) {
    const h = Math.min(23, +at[2]), m = Math.min(59, +(at[3] || 0));
    let d = new Date(); d.setHours(h, m, 0, 0);
    if (at[1] === '明天') d = new Date(d.getTime() + 86400000);
    else if (d.getTime() <= Date.now()) d = new Date(d.getTime() + 86400000);
    const content = at[4].replace(/^(?:提醒|通知|叫)\s*我?\s*/, '').trim() || '提醒';
    return { id: uid('rem'), content: cut(content, 40), due: d.getTime(), rule: 'once', enabled: true };
  }
  // FR-REM-003：每周几 HH:MM
  const weekly = t.match(/每(?:周|星期)([一二三四五六日天])\s*(\d{1,2})[:：点](\d{1,2})?分?\s*(.*)/);
  if (weekly) {
    const map = { 一: 1, 二: 2, 三: 3, 四: 4, 五: 5, 六: 6, 日: 0, 天: 0 };
    const target = map[weekly[1]];
    const h = Math.min(23, +weekly[2]), m = Math.min(59, +(weekly[3] || 0));
    const d = new Date(); d.setHours(h, m, 0, 0);
    let add = (target - d.getDay() + 7) % 7;
    if (add === 0 && d.getTime() <= Date.now()) add = 7;
    d.setDate(d.getDate() + add);
    const content = weekly[4].replace(/^(?:提醒|通知|叫)\s*我?\s*/, '').trim() || '提醒';
    return { id: uid('rem'), content: cut(content, 40), due: d.getTime(), rule: 'weekly', enabled: true };
  }
  // FR-REM-003：每隔 N 分钟 / N 小时
  const interval = t.match(/每(?:隔)?\s*(\d+(?:\.\d+)?)\s*(分钟?|小时|时)\s*(?:提醒|通知)?\s*(.*)/);
  if (interval) {
    const n = Math.max(1, Math.round(parseFloat(interval[1]) * (interval[2] === '分钟' || interval[2] === '分' ? 1 : 60)));
    const content = interval[3].replace(/^(?:提醒|通知|叫)\s*我?\s*/, '').trim() || '提醒';
    return { id: uid('rem'), content: cut(content, 40), due: Date.now() + n * 60000, rule: 'interval', intervalMin: n, enabled: true };
  }
  return null;
}

// 提醒重复规则的展示文案（每日 / 每周 / 间隔 / 一次性）
function remRuleText(r) {
  if (!r) return '';
  if (r.rule === 'daily') return '每天 ' + fmtTime(r.due);
  if (r.rule === 'weekly') return '每周' + '日一二三四五六'[new Date(r.due).getDay()] + ' ' + fmtTime(r.due);
  if (r.rule === 'interval') return '每 ' + (r.intervalMin || 30) + ' 分钟';
  return fmtDT(r.due);
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
