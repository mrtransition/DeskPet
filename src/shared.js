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
/* 日期时间：跟随界面语言。英文用 "Jan 5 14:30"，中文用 "1月5日 14:30" */
const fmtDT = ts => {
  const d = new Date(ts); const p = x => String(x).padStart(2, '0');
  const time = p(d.getHours()) + ':' + p(d.getMinutes());
  const en = typeof S !== 'undefined' && S && S.settings && S.settings.lang === 'en';
  if (en) {
    const mon = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'][d.getMonth()];
    return mon + ' ' + d.getDate() + ' ' + time;
  }
  return (d.getMonth() + 1) + '月' + d.getDate() + '日 ' + time;
};
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
/* NFR-PERF-004：皮肤图片 / 音效由主进程按 `$blob` 引用单独存放，前端拿到的仍是完整数据。
   回传时把已经落盘的资源换回引用（只带标记），否则每次保存都要序列化 + 传输十几 MB，
   设置页与宠物窗口都会明显卡顿。资源被改动时清掉 `$blob` 标记，让主进程重建文件。 */
const SKIN_BLOB_MARK = '$blob';
// 读取皮肤资源（图片 / 音效）；改动后调用 markPackDirty 让本次保存带上新资源
function packFiles(pack) { return (pack && pack.files) || {}; }
function markPackDirty(pack) {
  if (pack && typeof pack === 'object') delete pack[SKIN_BLOB_MARK];
  return pack;
}
function stripSkinAssets(pack) {
  if (!pack || typeof pack !== 'object' || !pack[SKIN_BLOB_MARK]) return pack;
  const out = { ...pack };
  if (out.files && Object.keys(out.files).length) out.files = {};
  if (out.sounds && Object.keys(out.sounds).length) out.sounds = {};
  return out;
}
function stateForSave() {
  const snap = { ...S, settings: { ...S.settings } };
  const skins = Array.isArray(S.settings.skins) ? S.settings.skins : [];
  snap.settings.skins = skins.map(item => (item.type === 'pack' && item.pack
    ? { ...item, pack: stripSkinAssets(item.pack) } : item));
  if (S.settings.skinPack) snap.settings.skinPack = stripSkinAssets(S.settings.skinPack);
  return snap;
}
async function saveState() {
  clearTimeout(saveTimer);
  savePending = true;
  saveTimer = setTimeout(async () => {
    try { await invoke('state_save', { state: stateForSave(), origin: MY_ORIGIN }); }
    catch (e) { toast('保存失败：' + e, 'err'); }
    finally { savePending = false; }
  }, 200);
}
// 需要与后端命令严格排序时使用（导入配置、批量应用设置）
async function saveStateNow() {
  clearTimeout(saveTimer);
  savePending = true;
  try { await invoke('state_save', { state: stateForSave(), origin: MY_ORIGIN }); }
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
      // 旧摘要基于上一个人格/开场白，重置会话时一并清空（FR-MEM-003）
      s.summary = '';
      s.summaryCount = 0;
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

/* ---- 皮肤库（FR-SKN-003） ----
   内置 3 套矢量皮肤 + 已导入皮肤（单图形象 / 皮肤包）统一成列表。
   同一时间只有一个「当前皮肤」：内置皮肤靠 settings.skin，导入皮肤靠
   settings.activeSkin（指向 skins[].id，决定运行时用 customSkin 还是 skinPack）。
   旧数据没有 activeSkin 字段，此时按「skinPack > customSkin > 内置」推断。 */
const CUSTOM_SKIN_KEY = 'custom';
function skinLib() {
  const list = Object.keys(SKINS).map(k => ({ key: k, type: 'builtin', label: SKINS[k].label, fur: SKINS[k].fur, id: k }));
  (S.settings.skins || []).forEach(s => list.push({ ...s, key: s.type === 'pack' ? 'pack:' + s.id : 'custom', type: s.type }));
  return list;
}
function findCustomSkin(id) {
  return (S.settings.skins || []).find(s => s.id === id) || null;
}
// 当前生效的导入皮肤 entry；返回 null 表示当前用内置皮肤
function activeCustomSkin() {
  const id = S.settings.activeSkin;
  if (!id) {
    if (S.settings.skinPack || (S.settings.customSkin && S.settings.customSkin.data)) {
      S.settings.skins = S.settings.skins || [];
      let entry = S.settings.skins.find(s => s.type === (S.settings.skinPack ? 'pack' : 'image'));
      if (!entry) {
        const name = S.settings.skinPack ? (S.settings.skinPack.name || '导入皮肤') : (S.settings.customSkin.name || '自定义形象');
        entry = S.settings.skinPack
          ? { id: uid('sk'), type: 'pack', name, label: name, pack: S.settings.skinPack, ts: Date.now() }
          : { id: uid('sk'), type: 'image', name, label: name.replace(/\.[^.]+$/, ''), data: S.settings.customSkin.data, ts: Date.now() };
        S.settings.skins.push(entry);
      }
      S.settings.activeSkin = entry.id;
      return entry;
    }
    return null;
  }
  return findCustomSkin(id);
}
// settings.skinPack / settings.customSkin 是运行时的「当前皮肤」快照，供 pet.html 直接使用
function syncActiveSkin() {
  const entry = S.settings.activeSkin ? findCustomSkin(S.settings.activeSkin) : null;
  if (!entry) { S.settings.activeSkin = null; S.settings.skinPack = null; S.settings.customSkin = null; return null; }
  if (entry.type === 'pack') {
    S.settings.skinPack = entry.pack || null;
    S.settings.customSkin = null;
  } else {
    S.settings.skinPack = null;
    S.settings.customSkin = entry.data ? { name: entry.name, data: entry.data, ts: entry.ts, alpha: entry.alpha } : null;
  }
  return entry;
}
function useCustomSkin(id) {
  const entry = findCustomSkin(id);
  if (!entry) return null;
  S.settings.activeSkin = entry.id;
  return syncActiveSkin();
}
function useBuiltinSkin(name) {
  if (!SKINS[name]) name = 'blue';
  S.settings.activeSkin = null;
  syncActiveSkin();
  return switchSkin(name);
}
// 删除一个导入皮肤（内置皮肤不可删）。当前正在使用该皮肤时回退到内置皮肤，
// 并清掉 skinPack / customSkin 快照，避免 pet.html 继续引用已删除的数据。
// 返回 { ok, fellBack, name }，由调用方决定 toast / 重绘时机。
function deleteCustomSkin(id) {
  if (!id) return { ok: false };
  S.settings.skins = S.settings.skins || [];
  const idx = S.settings.skins.findIndex(s => s.id === id);
  if (idx < 0) return { ok: false };
  const name = S.settings.skins[idx].name || S.settings.skins[idx].label || id;
  const wasActive = S.settings.activeSkin === id;
  S.settings.skins.splice(idx, 1);
  if (wasActive) {
    S.settings.activeSkin = null;
    S.settings.skinPack = null;
    S.settings.customSkin = null;
    if (!SKINS[S.settings.skin]) S.settings.skin = 'blue';
  }
  return { ok: true, fellBack: wasActive, name };
}

// 由单图条目临时生成一个皮肤包结构（不改动条目本身，供命名 / 导出预览使用）
function draftPackOf(entry) {
  if (!entry) return null;
  if (entry.type === 'pack') return entry.pack || null;
  if (!entry.pack && entry.data) {
    const m = /^data:image\/(gif|webp|png|apng)/i.exec(entry.data);
    const ext = m ? (m[1].toLowerCase() === 'apng' ? 'png' : m[1].toLowerCase()) : 'png';
    const stem = String(entry.name || 'custom-skin').replace(/\.[^.]+$/, '').replace(/[\/:*?"<>|]/g, '') || 'custom-skin';
    const fileName = stem + '.' + ext;
    const states = {};
    ['idle', 'drag', 'click', 'talk', 'sleep', 'work', 'happy', 'error'].forEach(st => { states[st] = fileName; });
    return { name: entry.name || '我的皮肤', version: '1.0', states, group: {}, files: { [fileName]: entry.data }, sounds: {}, ts: entry.ts || Date.now(), trim: entry.trim || null };
  }
  return entry.pack || null;
}
// 把当前「单图形象」就地升级为可维护的皮肤包条目；已有皮肤包则原样返回
function packOfEntry(entry) {
  if (!entry) return null;
  if (entry.type === 'pack') return entry.pack || null;
  // Codex 宠物不做「八态 → 单包」升级，直接返回空（维护工具走 Codex 分支）
  if (!entry.pack && entry.data) {
    const m = /^data:image\/(gif|webp|png|apng)/i.exec(entry.data);
    const ext = m ? (m[1].toLowerCase() === 'apng' ? 'png' : m[1].toLowerCase()) : 'png';
    const stem = String(entry.name || 'custom-skin').replace(/\.[^.]+$/, '').replace(/[\\/:*?"<>|]/g, '') || 'custom-skin';
    const fileName = stem + '.' + ext;
    const states = {};
    ['idle', 'drag', 'click', 'talk', 'sleep', 'work', 'happy', 'error'].forEach(st => { states[st] = fileName; });
    entry.pack = { name: entry.name || '我的皮肤', version: '1.0', states, group: {}, files: { [fileName]: entry.data }, sounds: {}, ts: entry.ts || Date.now(), trim: entry.trim || null };
    // 单图条目升级为皮肤包条目：运行时快照才能取到 contextSkin / 逐状态图
    entry.type = 'pack';
  }
  return entry.pack || null;
}
// 皮肤条目预览图：皮肤包取待机帧，单图取原图，内置为空（走矢量样式块）
function skinEntryPreview(entry) {
  if (entry.type === 'pack') {
    const p = entry.pack || {};
    if (p.format === 'codex' && (p.rows || []).length) {
      const idle = p.rows.find(r => r && r.name === 'idle') || p.rows[0];
      const frame = ((p.frames || []).find(f => f.row === idle.row && f.col === 0) || {}).src;
      return frame || (idle && p.files && p.files[idle.sheet]) || '';
    }
    const st = p.states || {};
    const f = st.idle || Object.values(st)[0];
    return (f && (p.files || {})[f]) || '';
  }
  return entry.data || '';
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

/* ---- HTML 上下文菜单（支持二级子菜单与滚动） ----
   items: [{ icon, label, check, danger, onClick, sub: [...] }, '-', ...]
   opts:  { bounds: DOMRect } —— 窗口内可用的矩形（宠物窗口很小，需要额外约束） */
function hideMenu() {
  const r = $id('menuRoot');
  // 兜底：清掉任何游离在 root 之外的子菜单，避免反复开关累积残留
  document.querySelectorAll('.ctxmenu.sub').forEach(n => { if (!r || !r.contains(n)) n.remove(); });
  if (!r) return;
  r.classList.remove('show');
  r.innerHTML = '';
  document.removeEventListener('mousedown', menuOutside);
}
function menuOutside(e) { if (!e.target.closest('.ctxmenu')) hideMenu(); }

// 在 bounds 内放置菜单。
// anchor.side 为 'left' / 'right' 时以 anchorRef 为参照贴在对应侧；
// anchor.x 为左边缘；anchor.y 为 'center' 时相对 anchorRef 竖直居中，否则为顶端坐标。
// 越界时先夹紧，夹不下再交给滚动容器（max-height）。
function menuPlace(box, scroll, x, y, bounds, anchor, anchorRef, gap) {
  const b = bounds || { left: 0, top: 0, right: innerWidth, bottom: innerHeight };
  const pad = 6;
  const maxW = Math.max(120, b.right - b.left - pad * 2);
  const maxH = Math.max(80, b.bottom - b.top - pad * 2);
  box.style.maxWidth = maxW + 'px';
  // 菜单总高受窗口限制；内部若含可滚动分组，则由分组吃掉剩余高度
  box.style.maxHeight = maxH + 'px';
  if (scroll) scroll.style.maxHeight = maxH + 'px';

  const w = box.offsetWidth;
  const h = box.offsetHeight;

  // 横向：优先贴 anchorRef 的指定侧；该侧放不下时自动翻到另一侧，最后才夹紧
  const g = typeof gap === 'number' ? gap : 6;
  let left;
  if (anchor && anchorRef && (anchor.side === 'left' || anchor.side === 'right')) {
    const rightX = anchorRef.right + g;
    const leftX = anchorRef.left - g - w;
    const fitsRight = rightX + w <= b.right - pad;
    const fitsLeft = leftX >= b.left + pad;
    if (anchor.side === 'right') {
      left = fitsRight ? rightX : (fitsLeft ? leftX : rightX);
    } else {
      left = fitsLeft ? leftX : (fitsRight ? rightX : leftX);
    }
  } else if (anchor && typeof anchor.x === 'number') {
    left = anchor.x;
  } else {
    left = x;
  }
  left = Math.max(b.left + pad, Math.min(left, b.right - pad - w));

  let top = y;
  if (anchor && anchor.y === 'center' && anchorRef) {
    top = anchorRef.top + (anchorRef.height - h) / 2;
  }
  top = Math.max(b.top + pad, Math.min(top, b.bottom - pad - h));

  box.style.left = left + 'px';
  box.style.top = top + 'px';
  return { left, top, width: w, height: h };
}

function showMenu(items, x, y, opts) {
  hideMenu();
  const b = (opts && opts.bounds) || { left: 0, top: 0, right: innerWidth, bottom: innerHeight };
  let root = $id('menuRoot');
  if (!root) { root = el('div'); root.id = 'menuRoot'; document.body.appendChild(root); }
  const box = el('div', 'ctxmenu');
  const scroll = el('div', 'cm-scroll');
  box.appendChild(scroll);

  const row = (it, parent) => {
    if (it === '-') { parent.appendChild(el('div', 'cm-div')); return; }
    // 分组块：{ header, items, scroll }。分组内的项目多时只让该块滚动，
    // 上下方的固定项（对话 / 设置）始终可见，不必整菜单一起滚。
    if (it && Array.isArray(it.items)) {
      const group = el('div', 'cm-group');
      if (it.header) group.appendChild(el('div', 'cm-group-head', esc(it.header)));
      const body = el('div', it.scroll ? 'cm-group-body cm-scroll' : 'cm-group-body');
      if (!it.items.length) body.appendChild(el('div', 'cm-empty', it.emptyText || '—'));
      it.items.forEach(sub => row(sub, body));
      group.appendChild(body);
      parent.appendChild(group);
      return;
    }
    const btn = el('button', 'cm-item' + (it.danger ? ' danger' : '') + (it.sub ? ' has-sub' : ''));
    btn.innerHTML = (it.icon ? '<span class="cm-ic">' + it.icon + '</span>' : '') +
      '<span>' + esc(it.label) + '</span>' +
      (it.sub ? '<span class="cm-arrow">▸</span>' : (it.check ? '<span class="cm-check">✓</span>' : ''));
    if (it.sub) {
      // hover 展开；点击也展开（触屏 / 不移动鼠标时可用）
      const open = () => openSub(it, btn, box);
      btn.addEventListener('mouseenter', open);
      btn.addEventListener('click', e => { e.stopPropagation(); open(); });
    } else {
      btn.addEventListener('mouseenter', () => closeSub());
      btn.onclick = () => { hideMenu(); it.onClick && it.onClick(); };
    }
    parent.appendChild(btn);
  };
  items.forEach(it => row(it, scroll));

  root.appendChild(box);
  root.classList.add('show');
  const anchor = opts && opts.anchor;
  const ref = opts && opts.anchorRef;   // 用于竖直居中（通常是宠物元素）
  const gap = (opts && opts.gap) || 6;
  const placed = menuPlace(box, scroll, x, y, b, anchor, ref, gap);
  // 主菜单右缘 → 子菜单左缘的落点，供子菜单定位复用
  const subAnchorX = placed.left + placed.width + gap;

  // ---- 子菜单 ----
  let subBox = null;
  let subOwner = null;
  function closeSub() {
    if (subBox) { subBox.remove(); subBox = null; }
    if (subOwner) { subOwner.classList.remove('open'); subOwner = null; }
  }
  function openSub(it, btn, parentBox) {
    if (subOwner === btn) return;
    closeSub();
    subBox = el('div', 'ctxmenu sub');
    const sc = el('div', 'cm-scroll');
    subBox.appendChild(sc);
    if (!it.sub.length) {
      sc.appendChild(el('div', 'cm-empty', it.emptyText || '—'));
    } else {
      it.sub.forEach(sub => {
        if (sub === '-') { sc.appendChild(el('div', 'cm-div')); return; }
        const b2 = el('button', 'cm-item' + (sub.danger ? ' danger' : ''));
        b2.innerHTML = (sub.icon ? '<span class="cm-ic">' + sub.icon + '</span>' : '') +
          '<span>' + esc(sub.label) + '</span>' + (sub.check ? '<span class="cm-check">✓</span>' : '');
        b2.onclick = () => { hideMenu(); sub.onClick && sub.onClick(); };
        sc.appendChild(b2);
      });
    }
    root.appendChild(subBox);
    subOwner = btn;
    btn.classList.add('open');

    // 定位：优先贴主菜单右缘展开；右侧空间确实不够时才翻到左侧，
    // 两侧都不够时靠滚动压缩高度，绝不允许压在父菜单上
    const pb = parentBox.getBoundingClientRect();
    const rb = btn.getBoundingClientRect();
    const pad = 6;
    subBox.style.maxHeight = Math.max(80, b.bottom - b.top - pad * 2) + 'px';
    const sw = subBox.offsetWidth, sh = subBox.offsetHeight;

    const roomRight = b.right - pad - (pb.right + gap);
    const roomLeft = (pb.left - gap) - (b.left + pad);
    // 子菜单刚插入时 offsetWidth 是未受约束的自然宽度（含最小宽度），用它判定空间；
    // 放不下时优先翻到左侧，两侧都不够才收窄，绝不压在父菜单上
    const needW = Math.ceil(sw);
    let left;
    if (roomRight >= needW) {
      left = pb.right + gap;                             // 正常：右侧展开
    } else if (roomLeft >= needW) {
      left = pb.left - gap - needW;                      // 右侧不够：翻到左侧
    } else if (roomRight >= roomLeft) {                   // 两侧都不够：选空间大的一侧并收窄
      left = pb.right + gap;
      subBox.style.maxWidth = Math.max(120, roomRight) + 'px';
    } else {
      left = Math.max(b.left + pad, pb.left - gap - needW);
      subBox.style.maxWidth = Math.max(120, roomLeft) + 'px';
    }
    left = Math.max(b.left + pad, Math.min(left, b.right - pad - Math.min(sw, subBox.offsetWidth)));

    // 竖直：与父项顶对齐，超出下边界则上移，但不越过上边界
    let top = rb.top - 4;
    if (top + sh > b.bottom - pad) top = b.bottom - pad - sh;
    top = Math.max(b.top + pad, Math.min(top, rb.top));
    subBox.style.left = left + 'px';
    subBox.style.top = top + 'px';
    void placed;
  }

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
  if (s.summary) lines.push('> **较早对话摘要（FR-MEM-003）**', '', s.summary, '', '---', '');
  s.msgs.forEach(m => {
    if (m.role === 'user') lines.push('## 🧑 用户', '', m.content, '');
    else if (m.role === 'assistant') lines.push('## 🐱 ' + (pers ? pers.name : 'AI'), '', m.content, '');
    else if (m.role === 'error') lines.push('> ⚠ ' + (m.title || '错误') + '：' + (m.content || ''), '');
    else lines.push('> ' + m.content, '');
  });
  return lines.join('\n');
}
