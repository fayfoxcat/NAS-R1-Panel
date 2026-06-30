/* ═══ NAS Panel — App Logic ═══ */
(function () {
  'use strict';

  const N = 2;  // overview + detail
  let cur = 0, sx = 0, sy = 0, cx = 0, swiping = false, horiz = false;
  let timer = null, idleTimer = null, cb = null;
  let lastTouch = Date.now();
  let screenOn = true;
  let swipeDir = 0;      // -1 = left, +1 = right
  let panelChanged = false;

  const $ = (s) => document.querySelector(s);
  const panels = document.querySelectorAll('.panel');
  const panelsEl = $('#panels');
  const dotsEl = $('#dots');

  for (let i = 0; i < N; i++) {
    const d = document.createElement('div');
    d.className = 'dot' + (i === 0 ? ' on' : '');
    dotsEl.appendChild(d);
  }
  const dots = dotsEl.querySelectorAll('.dot');

  // ── Screen idle / wake ─────────────────────────────────
  const IDLE_MS = 3 * 60 * 1000;

  function wakeScreen() {
    if (!screenOn) {
      fetch('/api/screen/on', { method: 'POST' }).catch(() => {});
      screenOn = true;
    }
  }

  function recordTouch() {
    lastTouch = Date.now();
    wakeScreen();
  }

  function startIdleTimer() {
    if (idleTimer) clearInterval(idleTimer);
    idleTimer = setInterval(() => {
      if (screenOn && (Date.now() - lastTouch) > IDLE_MS) {
        fetch('/api/screen/off', { method: 'POST' }).catch(() => {});
        screenOn = false;
      }
    }, 5000);
  }
  startIdleTimer();

  document.addEventListener('touchstart', recordTouch, { passive: true });

  // ── Positioning ───────────────────────────────────────
  function place(anim) {
    if (anim) {
      // Batch 1: enable transitions on all panels
      panels.forEach(el => { el.style.transition = ''; });
      void panelsEl.offsetHeight; // commit transition state for all at once
      // Batch 2: set target transforms — all panels animate together
      panels.forEach((el, i) => {
        let ofs = (i - cur) * 100;
        if (panelChanged) {
          // Wrap toward swipe direction so panels move in unison
          if (swipeDir < 0 && ofs >= 100) ofs -= 200;
          else if (swipeDir > 0 && ofs <= -100) ofs += 200;
        } else {
          // Snap-back: default wrap (prefer right side for positive offsets)
          if (ofs > 100) ofs -= 200;
          else if (ofs <= -100) ofs += 200;
        }
        el.style.transform = `translateX(${ofs}%)`;
      });
    } else {
      panels.forEach((el, i) => {
        el.style.transition = 'none';
        let ofs = (i - cur) * 100;
        if (ofs > 100) ofs -= 200;
        else if (ofs <= -100) ofs += 200;
        el.style.transform = `translateX(${ofs}%)`;
      });
    }
  }
  function go(i) {
    cur = ((i % N) + N) % N;
    panelChanged = true;
    place(true);
    panelChanged = false;
    dots.forEach((d, k) => d.classList.toggle('on', k === cur));
  }

  // ── Swipe ─────────────────────────────────────────────
  panelsEl.addEventListener('touchstart', (e) => {
    if (e.target.closest('.btn')) return;
    recordTouch();
    sx = cx = e.touches[0].clientX;
    sy = e.touches[0].clientY;
    swiping = true; horiz = false;
    panels.forEach(el => el.style.transition = 'none');
  }, { passive: true });

  panelsEl.addEventListener('touchmove', (e) => {
    if (!swiping) return;
    cx = e.touches[0].clientX;
    const dx = cx - sx, dy = e.touches[0].clientY - sy;
    if (!horiz && Math.abs(dx) > Math.abs(dy) && Math.abs(dx) > 8) horiz = true;
    if (horiz) {
      e.preventDefault();
      const pct = (dx / panelsEl.offsetWidth) * 100;
      panels.forEach((el, i) => {
        let ofs = (i - cur) * 100 + pct;
        if (ofs > 100) ofs -= 200;
        else if (ofs <= -100) ofs += 200;
        el.style.transform = `translateX(${ofs}%)`;
      });
    }
  }, { passive: false });

  panelsEl.addEventListener('touchend', () => {
    if (!swiping) return;
    swiping = false;
    if (!horiz) return;
    const dx = cx - sx, th = panelsEl.offsetWidth * 0.22;
    swipeDir = dx < 0 ? -1 : 1;
    if (dx < -th) go(cur + 1);
    else if (dx > th) go(cur - 1);
    else place(true);
  });

  // ── Modal ─────────────────────────────────────────────
  function confirm(msg, fn) { $('#modal-msg').textContent = msg; cb = fn; $('#modal-bg').classList.add('show'); }
  function hide() { $('#modal-bg').classList.remove('show'); cb = null; }
  $('#m-cancel').onclick = hide;
  $('#m-ok').onclick = () => { var fn = cb; hide(); if (fn) fn(); };
  $('#modal-bg').onclick = (e) => { if (e.target.id === 'modal-bg') hide(); };

  // ── Format ────────────────────────────────────────────
  function spd(n) {
    if (n == null || n < 1) return '0';
    if (n >= 1e9) return (n / 1e9).toFixed(1) + 'G';
    if (n >= 1e6) return (n / 1e6).toFixed(1) + 'M';
    if (n >= 1e3) return (n / 1e3).toFixed(0) + 'K';
    return n.toFixed(0) + 'B';
  }
  function gib(n) { return n != null ? n + 'G' : '--'; }
  function tib(n) { return n >= 1000 ? (n / 1000).toFixed(1) + 'T' : n.toFixed(1) + 'G'; }
  const RING_C = 314.16; // circumference of r=50 circle
  function setRing(el, pct) {
    const p = Math.min(100, Math.max(0, pct || 0));
    el.style.strokeDashoffset = RING_C * (1 - p / 100);
  }
  function barColor(p) {
    if (p >= 90) return 'var(--red)';
    if (p >= 75) return 'var(--org)';
    return 'var(--ylw)';
  }

  function pickNet(nets) {
    const cand = nets.filter(n => n.is_up && n.ipv4 && n.ipv4.length &&
      !n.name.startsWith('lo') && !n.name.startsWith('vnet') && !n.name.includes('ovs'));
    cand.sort((a, b) => (b.rx_bytes + b.tx_bytes) - (a.rx_bytes + a.tx_bytes));
    return cand[0] || null;
  }

  // ── Master Render ─────────────────────────────────────
  function render(d) {
    const upStr = (d.uptime || {}).uptime_str || '--';
    $('#up').textContent = upStr;
    const up2 = $('#up2');
    if (up2) up2.textContent = upStr;

    const cpu = d.cpu || {}, mem = d.memory || {};

    // Panel 0 — Overview
    const cpuPct = Math.round(cpu.percent ?? 0);
    $('#cpu-p').textContent = cpuPct;
    setRing($('#cpu-ring'), cpuPct);
    $('#cpu-sub').textContent = (cpu.temperature != null ? Math.round(cpu.temperature) + '℃' : '--') +
      (cpu.freq_current ? ' · ' + (cpu.freq_current / 1000).toFixed(1) + 'G' : '');

    const memPct = Math.round(mem.percent ?? 0);
    $('#mem-p').textContent = memPct;
    setRing($('#mem-ring'), memPct);
    $('#mem-sub').textContent = gib(mem.used_gb) + ' / ' + gib(mem.total_gb);

    const net = pickNet(d.network || []);
    if (net) {
      $('#net-name').textContent = net.name + (net.speed_mbps ? ' · ' + net.speed_mbps + 'M' : '');
      $('#rx').textContent = spd(net.rx_speed_bytes);
      $('#tx').textContent = spd(net.tx_speed_bytes);
      $('#ip4').textContent = net.ipv4.join(', ') || '--';
    }

    renderDisks(d.disk_health || []);

    // Panel 1 — Detail
    renderDocker(d.docker || []);
    renderVM(d.vms || []);
    renderSvc(d.services || []);
  }

  // ── Disks ─────────────────────────────────────────────
  function renderDisks(health) {
    const el = $('#disks');
    const phys = health.filter(d =>
      !d.name.includes('boot') && !d.name.includes('rpmb'));
    if (!phys.length) { el.innerHTML = '<div class="empty">无数据</div>'; return; }

    el.innerHTML = phys.map(d => {
      // Type badge
      let typeLabel, typeClass = 'b-type';
      if (d.type === 'nvme' || d.role === 'ssd') { typeClass += ' b-ssd'; typeLabel = 'SSD'; }
      else if (d.role === 'hdd') { typeClass += ' b-hdd'; typeLabel = 'HDD'; }
      else if (d.type === 'emmc') { typeClass += ' b-emmc'; typeLabel = 'eMMC'; }
      else { typeLabel = d.type || 'DISK'; }

      let healthBadge = '';
      if (d.health === 'PASSED') healthBadge = '<span class="disk-badge b-health-ok">正常</span>';
      else if (d.health === 'FAILED') healthBadge = '<span class="disk-badge b-health-bad">告警</span>';

      let roleBadge = '';
      if (d.role === 'system') roleBadge = '<span class="disk-badge b-role-sys">系统</span>';

      const meta = [];
      if (d.power_on_hours != null) meta.push(d.power_on_hours + 'h');
      if (d.temperature != null) meta.push(Math.round(d.temperature) + '℃');
      if (d.percent_used != null) meta.push('损耗' + d.percent_used.toFixed(1) + '%');

      let usage = '';
      if (d.mounts && d.mounts.length) {
        let tot = 0, usd = 0;
        for (const m of d.mounts) { tot += m.total_gb; usd += m.used_gb; }
        const p = tot > 0 ? (usd / tot * 100) : 0;
        usage = `<div class="disk-bar-wrap">
          <div class="disk-bar-fill" style="width:${Math.min(100,p)}%;background:${barColor(p)}"></div>
          <div class="disk-bar-text">
            <span class="r">${tib(usd)} / ${tib(tot - usd)} &nbsp; ${p.toFixed(1)}%</span>
          </div>
        </div>`;
      }

      return `<div class="disk">
        <div class="disk-top">
          <span class="disk-name">${d.model || d.name}</span>
          <div class="disk-badges">
            <span class="disk-badge ${typeClass}">${typeLabel} ${d.size}</span>
            ${healthBadge}
            ${roleBadge}
          </div>
        </div>
        <div class="disk-meta">${meta.join(' · ')}</div>
        ${usage}
      </div>`;
    }).join('');
  }

  function renderDocker(cs) {
    const el = $('#d-docker');
    if (!cs.length) { el.innerHTML = '<div class="empty">无容器</div>'; return; }
    cs.sort((a, b) => (b.State === 'running' ? 1 : 0) - (a.State === 'running' ? 1 : 0));
    el.innerHTML = cs.map(c => {
      const on = c.State === 'running';
      const nm = (c.Names || '').replace(/"/g, '');
      const st = (c.Status || '').replace(/"/g, '');
      return `<div class="item">
        <div class="item-l"><div class="item-n">${nm}</div><div class="item-s">${st}</div></div>
        <span class="tag ${on ? 'on' : 'off'}">${on ? '运行' : '停止'}</span>
      </div>`;
    }).join('');
  }

  function renderVM(vms) {
    const el = $('#d-vm');
    if (!vms.length) { el.innerHTML = '<div class="empty">无虚拟机</div>'; return; }
    el.innerHTML = vms.map(v => {
      const on = v.state === 'running';
      return `<div class="item">
        <div class="item-l"><div class="item-n">${v.name}</div><div class="item-s">ID ${v.id}</div></div>
        <span class="tag ${on ? 'on' : 'off'}">${on ? '运行' : v.state}</span>
      </div>`;
    }).join('');
  }

  function renderSvc(svcs) {
    const el = $('#d-svc');
    if (!svcs.length) { el.innerHTML = '<div class="empty">无</div>'; return; }
    el.innerHTML = svcs.map(s => `
      <div class="item">
        <div class="item-l"><div class="item-n">${s.name}</div></div>
        <span class="tag ${s.active ? 'on' : 'off'}">${s.active ? '活跃' : '停用'}</span>
      </div>`).join('');
  }

  // ── Fetch ─────────────────────────────────────────────
  async function fetchStatus() {
    try {
      const r = await fetch('/api/status');
      if (r.ok) render(await r.json());
    } catch (e) { /* ignore */ }
  }

  // ── Actions ───────────────────────────────────────────
  $('#b-reboot').onclick = () => confirm('确定重启 NAS？\n服务将暂时中断。', () => {
    $('#modal-msg').textContent = '重启中...';
    $('#m-cancel').style.display = 'none';
    $('#m-ok').style.display = 'none';
    fetch('/api/reboot', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"confirm":true}' }).catch(function(){});
  });
  $('#b-shutdown').onclick = () => confirm('确定关闭 NAS？\n需手动开机恢复。', () => {
    $('#modal-msg').textContent = '关机中...';
    $('#m-cancel').style.display = 'none';
    $('#m-ok').style.display = 'none';
    fetch('/api/shutdown', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"confirm":true}' }).catch(function(){});
  });

  // ── Init ──────────────────────────────────────────────
  place(false);
  fetchStatus();
  timer = setInterval(fetchStatus, 2000);
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      clearInterval(timer); timer = null;
      clearInterval(idleTimer); idleTimer = null;
    } else {
      fetchStatus();
      if (!timer) timer = setInterval(fetchStatus, 2000);
      if (!idleTimer) startIdleTimer();
    }
  });
})();
