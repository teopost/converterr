/* Converterr web UI — dependency-free single page app. */
'use strict';

const BOOT = window.Converterr || {};
const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

// ---------------------------------------------------------------------------
// Icons (inline SVG, Material-style paths)
// ---------------------------------------------------------------------------
const ICONS = {
  library: 'M10 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z',
  activity: 'M3.5 18.5l6-6 4 4L22 6.9 20.6 5.5l-7.1 8-4-4L2 17z',
  settings: 'M19.4 13a7.5 7.5 0 0 0 0-2l2.1-1.6-2-3.5-2.5 1a7.3 7.3 0 0 0-1.7-1L15 3.3h-4l-.4 2.6c-.6.3-1.2.6-1.7 1l-2.5-1-2 3.5L6.5 11a7.5 7.5 0 0 0 0 2l-2.1 1.6 2 3.5 2.5-1c.5.4 1.1.7 1.7 1l.4 2.6h4l.4-2.6c.6-.3 1.2-.6 1.7-1l2.5 1 2-3.5zM13 15.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7z',
  system: 'M21 2H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h7v2H8v2h8v-2h-2v-2h7c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 14H3V4h18v12z',
  add: 'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z',
  refresh: 'M17.65 6.35A7.96 7.96 0 0 0 12 4a8 8 0 1 0 7.73 10h-2.08A6 6 0 1 1 12 6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z',
  search: 'M15.5 14h-.8l-.3-.3A6.5 6.5 0 1 0 14 15.5l.3.3v.8l5 5 1.5-1.5-5-5zm-6 0a4.5 4.5 0 1 1 0-9 4.5 4.5 0 0 1 0 9z',
  play: 'M8 5v14l11-7z',
  pause: 'M6 19h4V5H6v14zm8-14v14h4V5h-4z',
  trash: 'M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z',
  edit: 'M3 17.25V21h3.75L17.8 9.94l-3.75-3.75L3 17.25zM20.7 7.04a1 1 0 0 0 0-1.41l-2.34-2.34a1 1 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z',
  up: 'M4 12l1.4 1.4L11 7.8V20h2V7.8l5.6 5.6L20 12l-8-8z',
  log: 'M14 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z',
  retry: 'M12 5V1L7 6l5 5V7c3.3 0 6 2.7 6 6s-2.7 6-6 6-6-2.7-6-6H4c0 4.4 3.6 8 8 8s8-3.6 8-8-3.6-8-8-8z',
  convert: 'M6.99 11L3 15l3.99 4v-3H14v-2H6.99v-3zM21 9l-3.99-4v3H10v2h7.01v3L21 9z',
  folder: 'M10 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z',
  filter: 'M10 18h4v-2h-4v2zM3 6v2h18V6H3zm3 7h12v-2H6v2z',
  clear: 'M5 13h14v-2H5v2zm-2 4h14v-2H3v2zM7 7v2h14V7H7z',
  bolt: 'M7 2v11h3v9l7-12h-4l4-8z',
};
const icon = (name) => `<svg viewBox="0 0 24 24" fill="currentColor"><path d="${ICONS[name] || ''}"/></svg>`;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

function bytes(n) {
  if (n == null) return '';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  let v = Number(n);
  while (Math.abs(v) >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(i >= 3 ? 2 : i ? 1 : 0)} ${u[i]}`;
}

function duration(sec) {
  if (sec == null) return '';
  sec = Math.max(0, Math.round(sec));
  const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
  if (h) return `${h}h ${String(m).padStart(2, '0')}m`;
  if (m) return `${m}m ${String(s).padStart(2, '0')}s`;
  return `${s}s`;
}

function ago(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  const s = (Date.now() - d) / 1000;
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.floor(s / 60)} minutes ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} hours ago`;
  if (s < 86400 * 7) return `${Math.floor(s / 86400)} days ago`;
  return d.toLocaleDateString();
}

const fname = (p) => String(p).split('/').pop();
const dirname = (p) => String(p).split('/').slice(0, -1).join('/');

const CODEC_COLORS = { hevc: '#27c24c', h264: '#5d9cec', av1: '#f05050', vc1: '#ffa500', mpeg2video: '#e67e22', msmpeg4v3: '#c0392b', mpeg4: '#9b59b6', vp9: '#1abc9c', wmv3: '#d35400' };
const codecColor = (c) => CODEC_COLORS[c] || '#888';

const STATUS_LABEL = {
  queued: 'inverse', running: 'primary', completed: 'success', skipped: 'info', failed: 'danger', cancelled: 'inverse',
};
const statusLabel = (s) => `<span class="label ${STATUS_LABEL[s] || ''}">${esc(s)}</span>`;

async function api(method, path, body) {
  const res = await fetch(`/api/v1${path}`, {
    method,
    headers: { 'X-Api-Key': BOOT.apiKey || '', 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (res.status === 204) return null;
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.message || `${res.status} ${res.statusText}`);
  return data;
}
const GET = (p) => api('GET', p);
const POST = (p, b) => api('POST', p, b ?? {});
const PUT = (p, b) => api('PUT', p, b);
const DEL = (p) => api('DELETE', p);

function toast(msg, kind = 'info') {
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.textContent = msg;
  $('#toasts').appendChild(el);
  setTimeout(() => el.remove(), kind === 'error' ? 7000 : 3500);
}

async function attempt(fn, okMsg) {
  try {
    const r = await fn();
    if (okMsg) toast(okMsg, 'success');
    return r;
  } catch (e) {
    toast(e.message, 'error');
    throw e;
  }
}

// ---------------------------------------------------------------------------
// Toolbar / modal
// ---------------------------------------------------------------------------
function setToolbar(buttons) {
  const tb = $('#toolbar');
  tb.innerHTML = '';
  for (const b of buttons) {
    if (b === '|') { tb.insertAdjacentHTML('beforeend', '<div class="sep"></div>'); continue; }
    if (b === '->') { tb.insertAdjacentHTML('beforeend', '<div class="spacer"></div>'); continue; }
    const btn = document.createElement('button');
    btn.className = `tb-btn${b.on ? ' on' : ''}`;
    btn.innerHTML = `${icon(b.icon)}<span>${esc(b.label)}</span>`;
    btn.onclick = async () => { btn.disabled = true; try { await b.onClick(); } catch (_) { /* toast shown */ } finally { btn.disabled = false; } };
    tb.appendChild(btn);
  }
}

function modal({ title, body, footer = [], wide = false, onOpen }) {
  const bd = $('#modal');
  bd.innerHTML = `<div class="modal" style="${wide ? 'max-width:1100px' : ''}">
      <div class="modal-header">${esc(title)}<button class="close" aria-label="Close">×</button></div>
      <div class="modal-body">${body}</div>
      <div class="modal-footer"></div></div>`;
  const close = () => { bd.hidden = true; bd.innerHTML = ''; };
  $('.close', bd).onclick = close;
  bd.onclick = (e) => { if (e.target === bd) close(); };
  const ft = $('.modal-footer', bd);
  if (!footer.length) ft.remove();
  for (const f of footer) {
    if (f === '->') { ft.insertAdjacentHTML('beforeend', '<div class="grow"></div>'); continue; }
    const b = document.createElement('button');
    b.className = `btn ${f.kind || ''}`;
    b.textContent = f.label;
    b.onclick = async () => {
      b.disabled = true;
      try { if ((await f.onClick(bd)) !== false) close(); } catch (_) { /* toast shown */ } finally { b.disabled = false; }
    };
    ft.appendChild(b);
  }
  bd.hidden = false;
  if (onOpen) onOpen(bd);
  return { close, el: bd };
}

function confirmModal(title, text, label = 'Delete') {
  return new Promise((resolve) => {
    modal({
      title,
      body: `<p>${text}</p>`,
      footer: ['->', { label: 'Cancel', onClick: () => resolve(false) }, { label, kind: 'danger', onClick: () => resolve(true) }],
    });
  });
}

/* Form builder: fields = [{key, label, type, help, options, step}] */
function formHtml(fields, values) {
  return fields.map((f) => {
    if (f.section) return `<h2 class="section-title">${esc(f.section)}</h2>`;
    const v = values[f.key];
    let input;
    switch (f.type) {
      case 'checkbox':
        input = `<input type="checkbox" name="${f.key}" ${v ? 'checked' : ''}>`;
        break;
      case 'select':
        input = `<select name="${f.key}">${f.options.map((o) => {
          const [val, lab] = Array.isArray(o) ? o : [o, o];
          return `<option value="${esc(val)}" ${String(v) === String(val) ? 'selected' : ''}>${esc(lab)}</option>`;
        }).join('')}</select>`;
        break;
      case 'textarea':
        input = `<textarea name="${f.key}">${esc(v)}</textarea>`;
        break;
      case 'path':
        input = `<div class="input-row"><input type="text" name="${f.key}" value="${esc(v)}"><button type="button" class="btn" data-browse="${f.key}">${icon('folder')}</button></div>`;
        break;
      default:
        input = `<input type="${f.type || 'text'}" name="${f.key}" value="${esc(v)}" ${f.step ? `step="${f.step}"` : ''}>`;
    }
    return `<div class="form-group"><label>${esc(f.label)}</label><div class="field">${input}${f.help ? `<div class="help">${f.help}</div>` : ''}</div></div>`;
  }).join('');
}

function readForm(root, fields, base) {
  const out = { ...base };
  for (const f of fields) {
    const el = root.querySelector(`[name="${f.key}"]`);
    if (!el) continue;
    if (f.type === 'checkbox') out[f.key] = el.checked;
    else if (f.type === 'number' || f.int) out[f.key] = Number(el.value || 0);
    else out[f.key] = el.value;
  }
  return out;
}

function bindBrowse(root) {
  $$('[data-browse]', root).forEach((b) => {
    b.onclick = () => {
      const input = root.querySelector(`[name="${b.dataset.browse}"]`);
      folderPicker(input.value || '/', (p) => { input.value = p; });
    };
  });
}

/* Inline folder browser shown under the path field. */
function folderPicker(start, onPick) {
  const host = document.createElement('div');
  host.className = 'dir-list';
  const field = $('#modal [data-browse]').closest('.field');
  $$('.dir-list', field).forEach((d) => d.remove());
  field.appendChild(host);
  const load = async (path) => {
    try {
      const d = await GET(`/filesystem?path=${encodeURIComponent(path)}`);
      onPick(d.path);
      host.innerHTML = (d.parent ? `<div data-p="${esc(d.parent)}">⬑ ..</div>` : '')
        + d.directories.map((x) => `<div data-p="${esc(x.path)}">📁 ${esc(x.name)}</div>`).join('');
      $$('[data-p]', host).forEach((el) => { el.onclick = () => load(el.dataset.p); });
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  load(start);
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------
let pollTimer = null;
const poll = (fn, ms) => { clearInterval(pollTimer); pollTimer = setInterval(fn, ms); };

let profilesCache = [];
async function loadProfiles() { profilesCache = await GET('/profile'); return profilesCache; }

/* ---- Libraries ---- */
const LIBRARY_FIELDS = () => [
  { key: 'name', label: 'Name' },
  { key: 'path', label: 'Path', type: 'path', help: 'Folder as seen by Converterr (inside the container).' },
  { key: 'profileId', label: 'Profile', type: 'select', int: true, options: profilesCache.map((p) => [p.id, p.name]) },
  { key: 'enabled', label: 'Enabled', type: 'checkbox', help: 'Disabled libraries are skipped by scans and webhooks.' },
  { key: 'extensions', label: 'File extensions', help: 'Comma separated.' },
  { key: 'notify', label: 'Notify after conversion', type: 'select', options: [['none', 'None'], ['sonarr', 'Sonarr (rescan series)'], ['radarr', 'Radarr (rescan movie)']] },
  { key: 'arrPath', label: 'Sonarr/Radarr path', help: 'The same folder as seen by Sonarr/Radarr, e.g. <code>/tv</code>. Used to map webhook and rescan paths. Leave empty if identical.' },
];

function editLibrary(lib) {
  const isNew = !lib;
  lib = lib || { name: '', path: '/', profileId: profilesCache[0]?.id || 1, enabled: true, extensions: 'mkv,mp4,avi,m4v,mov,wmv,ts,m2ts,mpg,mpeg', notify: 'none', arrPath: '' };
  const fields = LIBRARY_FIELDS();
  const footer = [];
  if (!isNew) {
    footer.push({
      label: 'Delete', kind: 'danger', onClick: async () => {
        if (!(await confirmModal('Delete library', `Remove library <b>${esc(lib.name)}</b>? Files on disk are not touched.`))) return false;
        await attempt(() => DEL(`/library/${lib.id}`), 'Library deleted');
        render();
        return true;
      },
    });
  }
  footer.push('->', { label: 'Cancel', onClick: () => true }, {
    label: 'Save', kind: 'primary', onClick: async (root) => {
      const body = readForm(root, fields, lib);
      await attempt(() => (isNew ? POST('/library', body) : PUT(`/library/${lib.id}`, body)), 'Library saved');
      render();
    },
  });
  modal({ title: isNew ? 'Add Library' : `Edit Library - ${lib.name}`, body: formHtml(fields, lib), footer, onOpen: bindBrowse });
}

async function pageLibraries() {
  setToolbar([
    { icon: 'add', label: 'Add Library', onClick: async () => { await loadProfiles(); editLibrary(null); } },
    '|',
    { icon: 'refresh', label: 'Scan All', onClick: () => attempt(() => POST('/library/scan'), 'Scan started') },
  ]);
  const draw = async () => {
    const [libs] = await Promise.all([GET('/library'), loadProfiles()]);
    const el = $('#content');
    if (!libs.length) {
      el.innerHTML = `<div class="empty"><p>No libraries yet.</p><button class="btn primary" id="addFirst">${icon('add')} Add your first library</button></div>`;
      $('#addFirst').onclick = () => editLibrary(null);
      return;
    }
    el.innerHTML = `<div class="lib-grid">${libs.map((l) => {
      const s = l.stats;
      const prof = profilesCache.find((p) => p.id === l.profileId);
      const bar = s.codecs.map(([c, n]) => `<div title="${esc(c || 'unknown')}: ${n}" style="width:${(n / Math.max(1, s.files)) * 100}%;background:${codecColor(c)}"></div>`).join('');
      return `<div class="card ${l.enabled ? '' : 'disabled'}">
        <h3><a href="#/library/${l.id}">${esc(l.name)}</a>
          ${l.scanning ? '<span class="label info">scanning</span>' : ''}
          ${l.exists ? '' : '<span class="label danger">missing</span>'}
          ${l.enabled ? '' : '<span class="label inverse">disabled</span>'}</h3>
        <div class="path">${esc(l.path)}</div>
        <div class="stats">
          <div class="stat"><b>${s.files}</b><small>files</small></div>
          <div class="stat"><b>${bytes(s.size)}</b><small>on disk</small></div>
          <div class="stat"><b style="color:${s.toConvert ? 'var(--warning)' : 'var(--success)'}">${s.toConvert}</b><small>to convert (${bytes(s.toConvertSize)})</small></div>
        </div>
        <div class="codec-bar">${bar}</div>
        <div>${s.codecs.slice(0, 6).map(([c, n]) => `<span class="label" style="background:${codecColor(c)}">${esc(c || '?')} ${n}</span>`).join('')}</div>
        <div class="footer">
          <span class="label primary">${esc(prof ? prof.name : 'no profile')}</span>
          ${l.notify !== 'none' ? `<span class="label inverse">${esc(l.notify)}</span>` : ''}
          <span class="grow"></span>
          <button class="icon-btn" title="Scan" data-scan="${l.id}">${icon('refresh')}</button>
          <button class="icon-btn" title="Edit" data-edit="${l.id}">${icon('edit')}</button>
        </div></div>`;
    }).join('')}</div>`;
    $$('[data-scan]').forEach((b) => { b.onclick = () => attempt(() => POST(`/library/${b.dataset.scan}/scan`), 'Scan started').then(draw); });
    $$('[data-edit]').forEach((b) => { b.onclick = () => editLibrary(libs.find((l) => l.id === Number(b.dataset.edit))); });
  };
  await draw();
  poll(draw, 5000);
}

/* ---- Library detail (file list) ---- */
async function pageLibrary(id, params) {
  const state = { page: 1, toConvert: params.get('toConvert') === '1', search: params.get('search') || '', selected: new Set() };
  const libs = await GET('/library');
  const lib = libs.find((l) => l.id === Number(id));
  if (!lib) { $('#content').innerHTML = '<div class="empty">Library not found</div>'; return; }
  await loadProfiles();

  const draw = async () => {
    const q = new URLSearchParams({ page: state.page, pageSize: 50, toConvert: state.toConvert, search: state.search });
    const d = await GET(`/library/${id}/files?${q}`);
    const queueSel = async (force) => {
      if (!state.selected.size) { toast('Select one or more files first'); return; }
      const r = await attempt(() => POST('/queue', { paths: [...state.selected], force }));
      toast(force ? `${r.forced} file(s) started now` : `${r.queued} file(s) queued`, 'success');
      state.selected.clear();
      draw();
    };
    setToolbar([
      { icon: 'refresh', label: 'Scan', onClick: () => attempt(() => POST(`/library/${id}/scan`), 'Scan started') },
      { icon: 'edit', label: 'Edit', onClick: () => editLibrary(lib) },
      '|',
      { icon: 'filter', label: state.toConvert ? 'To convert' : 'All files', on: state.toConvert, onClick: () => { state.toConvert = !state.toConvert; state.page = 1; return draw(); } },
      '|',
      { icon: 'convert', label: `Queue selected${state.selected.size ? ` (${state.selected.size})` : ''}`, onClick: () => queueSel(false) },
      { icon: 'bolt', label: 'Convert now', onClick: () => queueSel(true) },
    ]);
    const pages = Math.max(1, Math.ceil(d.totalRecords / d.pageSize));
    $('#content').innerHTML = `
      <h1 class="page-title">${esc(lib.name)} <span class="dim" style="font-size:14px;color:var(--text-dim)">${esc(lib.path)}</span></h1>
      <div class="stat-tiles">
        <div class="tile"><b>${lib.stats.files}</b><small>files</small></div>
        <div class="tile"><b>${bytes(lib.stats.size)}</b><small>on disk</small></div>
        <div class="tile"><b>${lib.stats.toConvert}</b><small>to convert</small></div>
        <div class="tile"><b>${bytes(lib.stats.toConvertSize)}</b><small>to convert (size)</small></div>
      </div>
      <div style="max-width:420px;margin-bottom:12px"><input type="search" id="fileSearch" placeholder="Filter by path" value="${esc(state.search)}"></div>
      ${d.records.length ? `<table class="table"><thead><tr>
        <th style="width:1%"><input type="checkbox" id="selAll"></th><th>File</th><th>Codec</th><th class="hide-sm">Resolution</th><th class="num">Size</th><th class="hide-sm num">Duration</th><th>Status</th><th></th></tr></thead><tbody>
        ${d.records.map((f) => `<tr>
          <td><input type="checkbox" data-sel="${esc(f.path)}" ${state.selected.has(f.path) ? 'checked' : ''}></td>
          <td><div class="path">${esc(fname(f.path))}</div><div class="dim path">${esc(dirname(f.path).replace(lib.path, '') || '/')}</div></td>
          <td><span class="label" style="background:${codecColor(f.codec)}">${esc(f.codec || '?')}</span>${f.bitDepth > 8 ? `<span class="label inverse">${f.bitDepth}bit</span>` : ''}</td>
          <td class="hide-sm">${f.width ? `${f.width}×${f.height}` : ''}</td>
          <td class="num">${bytes(f.size)}</td>
          <td class="hide-sm num">${duration(f.duration)}</td>
          <td>${f.needsConvert ? `<span class="label warning" title="${esc(f.reason)}">convert</span>` : (f.reason ? `<span class="label inverse" title="${esc(f.reason)}">${esc(f.reason.slice(0, 30))}</span>` : '<span class="label success">ok</span>')}</td>
          <td class="actions"><button class="icon-btn" title="Add to queue" data-q="${esc(f.path)}">${icon('convert')}</button><button class="icon-btn" title="Convert now (ignore time window)" data-force="${esc(f.path)}">${icon('bolt')}</button></td>
        </tr>`).join('')}</tbody></table>
        <div class="pager"><button class="btn" id="prev" ${state.page <= 1 ? 'disabled' : ''}>‹</button>
          Page ${state.page} of ${pages} · ${d.totalRecords} files
          <button class="btn" id="next" ${state.page >= pages ? 'disabled' : ''}>›</button></div>`
        : `<div class="empty">${lib.stats.files ? 'No matching files' : 'No files scanned yet — click Scan.'}</div>`}`;
    const s = $('#fileSearch');
    let t;
    s.oninput = () => { clearTimeout(t); t = setTimeout(() => { state.search = s.value; state.page = 1; draw().then(() => { const n = $('#fileSearch'); n.focus(); n.setSelectionRange(n.value.length, n.value.length); }); }, 350); };
    const prev = $('#prev'), next = $('#next');
    if (prev) prev.onclick = () => { state.page--; draw(); };
    if (next) next.onclick = () => { state.page++; draw(); };
    $$('[data-q]').forEach((b) => { b.onclick = () => attempt(() => POST('/queue', { path: b.dataset.q }), 'Queued'); });
    $$('[data-force]').forEach((b) => { b.onclick = () => attempt(() => POST('/queue', { path: b.dataset.force, force: true }), 'Conversion started'); });
    const updateSelLabel = () => draw();
    $$('[data-sel]').forEach((c) => { c.onchange = () => { if (c.checked) state.selected.add(c.dataset.sel); else state.selected.delete(c.dataset.sel); updateSelLabel(); }; });
    const all = $('#selAll');
    if (all) {
      all.checked = d.records.length > 0 && d.records.every((f) => state.selected.has(f.path));
      all.onchange = () => { d.records.forEach((f) => (all.checked ? state.selected.add(f.path) : state.selected.delete(f.path))); updateSelLabel(); };
    }
  };
  await draw();
}

/* ---- Queue ---- */
async function pageQueue() {
  const selected = new Set();
  const draw = async () => {
    const [tasks, w, libs] = await Promise.all([GET('/queue'), GET('/workers'), GET('/library')]);
    const libName = (id) => (libs.find((l) => l.id === id) || {}).name || '';
    setToolbar([
      w.paused
        ? { icon: 'play', label: 'Resume', onClick: () => attempt(() => POST('/workers/resume'), 'Workers resumed').then(draw) }
        : { icon: 'pause', label: 'Pause', onClick: () => attempt(() => POST('/workers/pause'), 'Workers paused').then(draw) },
      { icon: 'add', label: 'Add Path', onClick: addPathModal },
      { icon: 'bolt', label: `Convert now${selected.size ? ` (${selected.size})` : ''}`, onClick: async () => {
        if (!selected.size) { toast('Select one or more queued files first'); return; }
        const r = await attempt(() => POST('/queue/force', { ids: [...selected] }));
        toast(`${r.forced} file(s) started now`, 'success');
        selected.clear();
        draw();
      } },
      '|',
      { icon: 'refresh', label: 'Refresh', onClick: draw },
    ]);
    const state = w.paused ? '<div class="alert warning">Workers are paused. Running conversions are suspended and resume where they stopped.</div>'
      : (!w.running ? '<div class="alert">Outside the scheduled time window — conversions are suspended until the window opens.</div>' : '');
    $('#content').innerHTML = `${state}
      <div class="stat-tiles">${w.workers.map((x) => `<div class="tile"><b>${x.taskId ? (x.suspended ? 'Suspended' : 'Working') : (x.enabled ? 'Idle' : 'Stopping')}</b><small>${x.forced ? 'Forced' : `Worker ${x.id}`}${x.taskId ? ` · task #${x.taskId}` : ''}</small></div>`).join('') || '<div class="tile"><b>0</b><small>workers configured</small></div>'}</div>
      ${tasks.length ? `<table class="table"><thead><tr><th style="width:1%"></th><th>File</th><th class="hide-sm">Library</th><th>Codec</th><th class="num">Size</th><th>Progress</th><th class="num hide-sm">Speed</th><th class="num">ETA</th><th></th></tr></thead><tbody>
      ${tasks.map((t) => `<tr>
        <td>${t.status === 'queued' && !t.force ? `<input type="checkbox" data-sel="${t.id}" ${selected.has(t.id) ? 'checked' : ''}>` : ''}</td>
        <td><div class="path">${esc(fname(t.path))}${t.force ? ' <span class="label warning">forced</span>' : ''}</div><div class="dim path">${esc(dirname(t.path))}</div></td>
        <td class="hide-sm">${esc(libName(t.libraryId))}</td>
        <td>${t.codecBefore ? `<span class="label" style="background:${codecColor(t.codecBefore)}">${esc(t.codecBefore)}</span>` : ''}</td>
        <td class="num">${bytes(t.sizeBefore)}</td>
        <td>${t.status === 'running'
          ? `<div class="progress ${t.suspended ? 'paused' : ''}"><div class="bar" style="width:${t.progress.toFixed(1)}%"></div><span>${t.suspended ? 'suspended · ' : ''}${t.progress.toFixed(1)}%</span></div>`
          : statusLabel(t.status)}</td>
        <td class="num hide-sm">${esc(t.speed)}</td>
        <td class="num">${t.status === 'running' && !t.suspended ? duration(t.etaSeconds) : ''}</td>
        <td class="actions">
          ${t.status === 'queued' && !t.force ? `<button class="icon-btn" title="Convert now (ignore time window)" data-force="${t.id}">${icon('bolt')}</button>` : ''}
          ${t.status === 'queued' ? `<button class="icon-btn" title="Move to top" data-top="${t.id}">${icon('up')}</button>` : ''}
          <button class="icon-btn" title="${t.status === 'running' ? 'Cancel' : 'Remove'}" data-rm="${t.id}">${icon('trash')}</button></td>
      </tr>`).join('')}</tbody></table>` : '<div class="empty">Queue is empty</div>'}`;
    $$('[data-top]').forEach((b) => { b.onclick = () => attempt(() => POST(`/queue/${b.dataset.top}/top`)).then(draw); });
    $$('[data-force]').forEach((b) => { b.onclick = () => attempt(() => POST('/queue/force', { ids: [Number(b.dataset.force)] }), 'Conversion started').then(draw); });
    $$('[data-sel]').forEach((c) => { c.onchange = () => { const id = Number(c.dataset.sel); if (c.checked) selected.add(id); else selected.delete(id); draw(); }; });
    $$('[data-rm]').forEach((b) => {
      b.onclick = async () => {
        if (await confirmModal('Remove from queue', 'Remove this file from the queue? A running conversion is stopped and the original file is kept.', 'Remove')) {
          await attempt(() => DEL(`/queue/${b.dataset.rm}`), 'Removed');
          draw();
        }
      };
    });
  };
  await draw();
  poll(draw, 3000);
}

function addPathModal() {
  const fields = [
    { key: 'path', label: 'File or folder', type: 'path', help: 'Every video file in a folder is queued, ignoring the profile rules. The path must be inside a library.' },
    { key: 'force', label: 'Convert now', type: 'checkbox', help: 'Start right away, ignoring the time window.' },
  ];
  modal({
    title: 'Queue file or folder',
    body: formHtml(fields, { path: '/' }),
    onOpen: bindBrowse,
    footer: ['->', { label: 'Cancel', onClick: () => true }, {
      label: 'Queue', kind: 'primary', onClick: async (root) => {
        const r = await attempt(() => POST('/queue', readForm(root, fields, {})));
        toast(`${r.queued} file(s) queued${r.forced ? `, ${r.forced} started now` : ''}`, 'success');
        route();
      },
    }],
  });
}

/* ---- History ---- */
async function pageHistory(params) {
  const state = { page: 1, status: params.get('status') || '' };
  const draw = async () => {
    const d = await GET(`/history?${new URLSearchParams({ page: state.page, pageSize: 50, status: state.status })}`);
    setToolbar([
      { icon: 'refresh', label: 'Refresh', onClick: draw },
      '|',
      ...['', 'completed', 'skipped', 'failed', 'cancelled'].map((s) => ({ icon: 'filter', label: s || 'All', on: state.status === s, onClick: () => { state.status = s; state.page = 1; return draw(); } })),
      '->',
      { icon: 'clear', label: 'Clear', onClick: async () => { if (await confirmModal('Clear history', 'Delete every history entry?', 'Clear')) { await attempt(() => DEL('/history'), 'History cleared'); draw(); } } },
    ]);
    const pages = Math.max(1, Math.ceil(d.totalRecords / d.pageSize));
    $('#content').innerHTML = `
      <div class="stat-tiles">
        <div class="tile"><b>${d.stats.completed}</b><small>converted</small></div>
        <div class="tile"><b>${d.stats.failed}</b><small>failed</small></div>
        <div class="tile"><b style="color:var(--success)">${bytes(d.stats.savedBytes)}</b><small>space saved</small></div>
      </div>
      ${d.records.length ? `<table class="table"><thead><tr><th>File</th><th>Status</th><th>Codec</th><th class="num">Before</th><th class="num">After</th><th class="num hide-sm">Saved</th><th class="hide-sm">Finished</th><th></th></tr></thead><tbody>
      ${d.records.map((t) => {
        const saved = t.sizeAfter != null && t.status === 'completed' ? (1 - t.sizeAfter / t.sizeBefore) * 100 : null;
        return `<tr>
          <td><div class="path">${esc(fname(t.outputPath || t.path))}</div><div class="dim path" title="${esc(t.error)}">${esc(t.error || dirname(t.path))}</div></td>
          <td>${statusLabel(t.status)}</td>
          <td>${t.codecBefore ? `<span class="label" style="background:${codecColor(t.codecBefore)}">${esc(t.codecBefore)}</span>` : ''}${t.codecAfter ? ` → <span class="label" style="background:${codecColor(t.codecAfter)}">${esc(t.codecAfter)}</span>` : ''}</td>
          <td class="num">${bytes(t.sizeBefore)}</td>
          <td class="num">${t.sizeAfter != null ? bytes(t.sizeAfter) : ''}</td>
          <td class="num hide-sm">${saved != null ? `${saved.toFixed(0)}%` : ''}</td>
          <td class="hide-sm" title="${esc(t.finishedAt)}">${ago(t.finishedAt)}</td>
          <td class="actions">
            <button class="icon-btn" title="ffmpeg log" data-log="${t.id}">${icon('log')}</button>
            ${t.status !== 'completed' ? `<button class="icon-btn" title="Retry" data-retry="${t.id}">${icon('retry')}</button>` : ''}</td>
        </tr>`;
      }).join('')}</tbody></table>
      <div class="pager"><button class="btn" id="prev" ${state.page <= 1 ? 'disabled' : ''}>‹</button>
        Page ${state.page} of ${pages} · ${d.totalRecords} entries
        <button class="btn" id="next" ${state.page >= pages ? 'disabled' : ''}>›</button></div>` : '<div class="empty">No history yet</div>'}`;
    const prev = $('#prev'), next = $('#next');
    if (prev) prev.onclick = () => { state.page--; draw(); };
    if (next) next.onclick = () => { state.page++; draw(); };
    $$('[data-log]').forEach((b) => {
      b.onclick = async () => {
        const r = await attempt(() => GET(`/task/${b.dataset.log}/log`));
        modal({ title: `Task #${b.dataset.log} log`, body: `<pre>${esc(r.log || '(empty)')}</pre>`, wide: true });
      };
    });
    $$('[data-retry]').forEach((b) => { b.onclick = () => attempt(() => POST(`/history/${b.dataset.retry}/retry`), 'Queued again').then(draw); });
  };
  await draw();
}

/* ---- Profiles ---- */
const PROFILE_FIELDS = [
  { key: 'name', label: 'Name' },
  { key: 'sourceCodecs', label: 'Convert these codecs', help: 'Comma separated ffprobe codec names (e.g. <code>av1,vc1,mpeg2video</code>). Empty = convert every file whose codec differs from the target.' },
  { key: 'convertOversized', label: 'Convert oversized files', type: 'checkbox', help: 'Also convert files larger than the max resolution below, whatever their codec.' },
  { key: 'maxWidth', label: 'Max width', type: 'number', help: 'Downscale to fit (aspect ratio kept). 0 = no limit.' },
  { key: 'maxHeight', label: 'Max height', type: 'number' },
  { key: 'encoder', label: 'Video encoder', type: 'select', options: [['libx265', 'HEVC (libx265)'], ['libx264', 'H.264 (libx264)'], ['libsvtav1', 'AV1 (libsvtav1)']] },
  { key: 'preset', label: 'Preset', help: 'x264/x265: ultrafast … veryslow. SVT-AV1: 0-13.' },
  { key: 'crf', label: 'Quality (CRF)', type: 'number', help: 'Lower = better quality and bigger files. x265: 22-28 is typical.' },
  { key: 'pixFmt', label: 'Pixel format', type: 'select', options: [['yuv420p', '8-bit (yuv420p)'], ['yuv420p10le', '10-bit (yuv420p10le)'], ['', 'Keep source']], help: '8-bit output from an HDR source is tone-mapped to SDR.' },
  { key: 'container', label: 'Container', type: 'select', options: [['mkv', 'MKV'], ['mp4', 'MP4 (drops subtitles)'], ['keep', 'Keep (mp4 stays mp4, everything else mkv)']] },
  { key: 'audio', label: 'Audio', type: 'select', options: [['copy', 'Copy'], ['aac', 'AAC 192k'], ['ac3', 'AC3 640k']] },
  { key: 'skipIfLarger', label: 'Keep original if not smaller', type: 'checkbox' },
  { key: 'extraArgs', label: 'Extra ffmpeg arguments', help: 'Added before the output file, e.g. <code>-threads 3</code>.' },
];

function editProfile(p) {
  const isNew = !p;
  p = p || { name: '', sourceCodecs: '', convertOversized: false, maxWidth: 1920, maxHeight: 1080, encoder: 'libx265', preset: 'veryfast', crf: 26, pixFmt: 'yuv420p', container: 'mkv', audio: 'copy', skipIfLarger: true, extraArgs: '' };
  const footer = [];
  if (!isNew) {
    footer.push({
      label: 'Delete', kind: 'danger', onClick: async () => {
        if (!(await confirmModal('Delete profile', `Delete profile <b>${esc(p.name)}</b>?`))) return false;
        await attempt(() => DEL(`/profile/${p.id}`), 'Profile deleted');
        render();
        return true;
      },
    });
  }
  footer.push('->', { label: 'Cancel', onClick: () => true }, {
    label: 'Save', kind: 'primary', onClick: async (root) => {
      const body = readForm(root, PROFILE_FIELDS, p);
      await attempt(() => (isNew ? POST('/profile', body) : PUT(`/profile/${p.id}`, body)), 'Profile saved');
      render();
    },
  });
  modal({ title: isNew ? 'Add Profile' : `Edit Profile - ${p.name}`, body: formHtml(PROFILE_FIELDS, p), footer });
}

async function pageProfiles() {
  setToolbar([{ icon: 'add', label: 'Add Profile', onClick: () => editProfile(null) }]);
  const profiles = await loadProfiles();
  $('#content').innerHTML = `
    <div class="alert">A profile decides <b>which</b> files are converted (codec / resolution rules) and <b>how</b> (encoder, quality, size). Every library uses one profile.</div>
    <div class="lib-grid">${profiles.map((p) => `<div class="card">
      <h3>${esc(p.name)}</h3>
      <div style="margin:8px 0">${(p.sourceCodecs ? p.sourceCodecs.split(',') : ['any ≠ target']).map((c) => `<span class="label" style="background:${codecColor(c.trim())}">${esc(c.trim())}</span>`).join('')}
        ${p.convertOversized ? `<span class="label inverse">> ${p.maxWidth}×${p.maxHeight}</span>` : ''}</div>
      <div class="dim" style="color:var(--text-dim)">→ ${esc(p.encoder)} · ${esc(p.preset)} · CRF ${p.crf} · max ${p.maxWidth || '∞'}×${p.maxHeight || '∞'} · ${esc(p.pixFmt || 'source')} · ${esc(p.container)} · audio ${esc(p.audio)}</div>
      <div class="footer"><span class="grow"></span><button class="icon-btn" data-edit="${p.id}">${icon('edit')}</button></div>
    </div>`).join('')}</div>`;
  $$('[data-edit]').forEach((b) => { b.onclick = () => editProfile(profiles.find((p) => p.id === Number(b.dataset.edit))); });
}

/* ---- Settings: General / Connect ---- */
const GENERAL_FIELDS = [
  { key: 'workers', label: 'Workers', type: 'number', help: 'Conversions running at the same time. On a Raspberry Pi keep 1.' },
  { key: 'scheduleEnabled', label: 'Use time window', type: 'checkbox', help: 'Outside the window running conversions are suspended (not restarted) and new ones are not started.' },
  { key: 'scheduleStart', label: 'Window start', type: 'time' },
  { key: 'scheduleEnd', label: 'Window end', type: 'time', help: 'Windows across midnight (e.g. 22:00 → 06:00) are supported.' },
  { key: 'scanIntervalMinutes', label: 'Library scan interval', type: 'number', help: 'Minutes between automatic scans of every library. 0 = only manual scans and webhooks.' },
  { key: 'probeConcurrency', label: 'Parallel probes', type: 'number', help: 'ffprobe calls running at the same time during a scan.' },
  { key: 'cacheDir', label: 'Cache folder', help: 'ffmpeg writes here first; the original is replaced only after the output is verified.' },
  { key: 'apiKey', label: 'API key', help: 'Send as <code>X-Api-Key</code> header or <code>?apikey=</code>. Empty = generate a new one.' },
  { key: 'authUsername', label: 'UI username' },
  { key: 'authPassword', label: 'UI password', type: 'password', help: 'Optional HTTP Basic authentication for the web UI. Empty = no login.' },
];

const CONNECT_FIELDS = [
  { section: 'Sonarr / Radarr' },
  { key: 'sonarrUrl', label: 'Sonarr URL', help: 'e.g. <code>http://sonarr:8989</code>' },
  { key: 'sonarrApiKey', label: 'Sonarr API key' },
  { key: 'radarrUrl', label: 'Radarr URL', help: 'e.g. <code>http://radarr:7878</code>' },
  { key: 'radarrApiKey', label: 'Radarr API key' },
  { section: 'Telegram' },
  { key: 'telegramBotToken', label: 'Telegram bot token', type: 'password', help: 'From @BotFather. You can reuse the bot already configured in Sonarr/Radarr.' },
  { key: 'telegramChatId', label: 'Telegram chat ID' },
  { key: 'telegramTopicId', label: 'Telegram topic ID', help: 'Optional, for group topics.' },
  { key: 'telegramSilent', label: 'Send silently', type: 'checkbox' },
  { key: 'telegramOnQueued', label: 'On file queued', type: 'checkbox', help: 'A file needing conversion was found (one summary message per scan).' },
  { key: 'telegramOnCompleted', label: 'On conversion done', type: 'checkbox', help: 'Includes conversions skipped because the output was not smaller.' },
  { key: 'telegramOnFailed', label: 'On conversion failed', type: 'checkbox' },
];

async function pageSettings(fields, extra = '') {
  const s = await GET('/settings');
  const save = async () => {
    const body = readForm($('#content'), fields, s);
    const saved = await attempt(() => PUT('/settings', body), 'Settings saved');
    if (saved.apiKey !== BOOT.apiKey) { BOOT.apiKey = saved.apiKey; }
    render();
  };
  setToolbar([{ icon: 'edit', label: 'Save', onClick: save }]);
  $('#content').innerHTML = `<form onsubmit="return false">${formHtml(fields, s)}</form>${extra}`;
  return s;
}

async function pageGeneral() { await pageSettings(GENERAL_FIELDS); }

async function pageConnect() {
  const origin = location.origin;
  const extra = `
    <div class="form-group"><label></label><div class="field">
      <button class="btn" id="testSonarr">Test Sonarr</button> <button class="btn" id="testRadarr">Test Radarr</button>
      <button class="btn" id="testTelegram">Test Telegram</button></div></div>
    <h2 class="section-title">Webhook (convert right after import)</h2>
    <p>In Sonarr/Radarr: <b>Settings → Connect → + → Webhook</b>, trigger <b>On Import</b> (and On Upgrade), method POST, URL:</p>
    <pre style="background:#1a1a1a;padding:10px;border-radius:4px">${esc(origin)}/api/v1/webhook?apikey=${esc(BOOT.apiKey)}</pre>
    <p class="dim" style="color:var(--text-dim)">Use the address Sonarr/Radarr can reach (e.g. <code>http://converterr:8787</code> on a shared Docker network). Imported files are probed and queued only if the library profile says so; set each library's <i>Sonarr/Radarr path</i> if the folders are mounted at different paths.</p>`;
  await pageSettings(CONNECT_FIELDS, extra);
  const test = (k) => async () => {
    const root = $('#content');
    const body = { url: root.querySelector(`[name="${k}Url"]`).value, apiKey: root.querySelector(`[name="${k}ApiKey"]`).value };
    const r = await attempt(() => POST('/test/arr', body));
    toast(`Connected: ${r.message}`, 'success');
  };
  $('#testTelegram').onclick = async () => {
    const root = $('#content');
    const v = (k) => root.querySelector(`[name="${k}"]`).value;
    await attempt(() => POST('/test/telegram', { botToken: v('telegramBotToken'), chatId: v('telegramChatId'), topicId: v('telegramTopicId') }), 'Telegram test message sent');
  };
  $('#testSonarr').onclick = test('sonarr');
  $('#testRadarr').onclick = test('radarr');
}

/* ---- System ---- */
async function pageStatus() {
  setToolbar([{ icon: 'refresh', label: 'Refresh', onClick: pageStatus }]);
  const s = await GET('/system/status');
  const row = (k, v) => `<div class="form-group"><label>${k}</label><div class="field" style="padding-top:6px">${v}</div></div>`;
  $('#content').innerHTML = `<h2 class="section-title" style="margin-top:0">About</h2>
    ${row('Version', esc(s.version))}
    ${row('ffmpeg', esc(s.ffmpeg))}
    ${row('Uptime', duration(s.uptimeSeconds))}
    ${row('Data folder', esc(s.dataDir))}
    ${row('Workers', s.running ? '<span class="label success">running</span>' : (s.paused ? '<span class="label warning">paused</span>' : '<span class="label info">waiting for time window</span>'))}
    ${row('Queue', s.queueSize)}
    ${row('Converted', `${s.history.completed} files, ${bytes(s.history.savedBytes)} saved`)}
    <h2 class="section-title">More info</h2>
    ${row('Source', '<a href="https://github.com/teopost/converterr" target="_blank" rel="noopener">github.com/teopost/converterr</a>')}
    ${row('License', 'GPL-3.0-or-later')}`;
}

async function pageLogs() {
  const draw = async () => {
    const logs = await GET('/log');
    $('#content').innerHTML = logs.length ? `<table class="table"><thead><tr><th style="width:1%"></th><th style="width:180px">Time</th><th>Message</th></tr></thead><tbody>
      ${logs.map((l) => `<tr><td><span class="label ${l.level === 'error' ? 'danger' : l.level === 'warn' ? 'warning' : 'info'}">${esc(l.level)}</span></td>
        <td class="dim" title="${esc(l.time)}">${new Date(l.time).toLocaleString()}</td><td class="path log-${esc(l.level)}">${esc(l.message)}</td></tr>`).join('')}
      </tbody></table>` : '<div class="empty">No log entries</div>';
  };
  setToolbar([{ icon: 'refresh', label: 'Refresh', onClick: draw }]);
  await draw();
  poll(draw, 5000);
}

// ---------------------------------------------------------------------------
// Router & chrome
// ---------------------------------------------------------------------------
const ROUTES = [
  [/^\/?$/, 'libraries', () => pageLibraries()],
  [/^\/library\/(\d+)$/, 'libraries', (m, p) => pageLibrary(m[1], p)],
  [/^\/activity\/queue$/, 'queue', () => pageQueue()],
  [/^\/activity\/history$/, 'history', (m, p) => pageHistory(p)],
  [/^\/settings\/profiles$/, 'profiles', () => pageProfiles()],
  [/^\/settings\/general$/, 'general', () => pageGeneral()],
  [/^\/settings\/connect$/, 'connect', () => pageConnect()],
  [/^\/system\/status$/, 'status', () => pageStatus()],
  [/^\/system\/logs$/, 'logs', () => pageLogs()],
];
const PARENT = { queue: 'activity', history: 'activity', profiles: 'settings', general: 'settings', connect: 'settings', status: 'system', logs: 'system' };

let current = null;
async function render() {
  if (current) await current();
}

async function route() {
  clearInterval(pollTimer);
  const [path, qs] = (location.hash.slice(1) || '/').split('?');
  const params = new URLSearchParams(qs || '');
  for (const [re, name, fn] of ROUTES) {
    const m = path.match(re);
    if (!m) continue;
    const parent = PARENT[name] || name;
    $$('.nav-item').forEach((a) => a.classList.toggle('active', a.dataset.route === parent));
    $$('.nav-sub').forEach((d) => d.classList.toggle('open', d.dataset.parent === parent));
    $$('.nav-sub a').forEach((a) => a.classList.toggle('active', a.dataset.route === name));
    $('#sidebar').classList.remove('open');
    $('#toolbar').innerHTML = '';
    $('#content').innerHTML = '';
    current = () => fn(m, params).catch((e) => { $('#content').innerHTML = `<div class="alert warning">${esc(e.message)}</div>`; });
    await current();
    return;
  }
  location.hash = '#/';
}

async function refreshHeader() {
  try {
    const [q, w] = await Promise.all([GET('/queue'), GET('/workers')]);
    $('#queueBadge').textContent = q.length || '';
    const running = q.find((t) => t.status === 'running');
    $('#headerStatus').innerHTML = (w.paused ? '<span class="label warning">paused</span>' : (!w.running ? '<span class="label info">scheduled</span>' : ''))
      + (running ? ` <span title="${esc(running.path)}">${esc(fname(running.path)).slice(0, 40)} · ${running.progress.toFixed(1)}%</span>` : '');
  } catch (_) { /* ignore */ }
}

function init() {
  $$('[data-icon]').forEach((el) => { el.innerHTML = icon(el.dataset.icon); });
  $('#sidebarToggle').onclick = () => $('#sidebar').classList.toggle('open');
  $('#globalSearch').onkeydown = async (e) => {
    if (e.key !== 'Enter' || !e.target.value.trim()) return;
    const libs = await GET('/library');
    if (libs.length) location.hash = `#/library/${libs[0].id}?search=${encodeURIComponent(e.target.value.trim())}`;
  };
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') { const bd = $('#modal'); bd.hidden = true; bd.innerHTML = ''; } });
  window.addEventListener('hashchange', route);
  route();
  refreshHeader();
  setInterval(refreshHeader, 5000);
}

init();
