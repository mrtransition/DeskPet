'use strict';
/* ============ 纯前端背景去除（纯色 / 简单背景） ============
   方案见 docs/方案：将极简图片背景去除工具合并到 DeskPet.md（路径 A：WebView + Canvas）。
   算法与 UI 解耦，只操作 RGBA 像素，便于后续迁移到 Rust（路径 B）。流程：

   1. 估计背景色：四条边缘带量化颜色直方图取众数；
   2. 构建背景掩码：从四边洪泛填充，RGB 与背景色距离 ≤ tolerance × 1.25 的连通区域；
   3. 应用 Alpha：距离 ≤ tolerance 置 0，tolerance ~ 1.25×tolerance 线性过渡，只降不升；
   4. 边缘羽化：对 Alpha 通道做可分离盒式模糊。

   注意：不做深度学习，不处理发丝 / 毛发 / 半透明物体。 */
(function (global) {
  const DEFAULTS = Object.freeze({ tolerance: 32, feather: 2, edgeSample: 12 });
  const MAX_PIXELS = 16 * 1024 * 1024; // 4096²，超出直接拒绝，避免前端冻结

  const clamp = (v, lo, hi, dft) => {
    const n = Number(v);
    if (!isFinite(n)) return dft;
    return Math.max(lo, Math.min(hi, n));
  };

  // 背景色估计：边缘带内出现次数最多的量化颜色（5 bit / 通道）
  function estimateBackgroundColor(data, width, height, band) {
    band = Math.max(1, Math.min(Math.round(band) || 1, Math.min(width, height) >> 1 || 1));
    const bins = new Map();
    const add = (x, y) => {
      const o = (y * width + x) * 4;
      if (data[o + 3] < 8) return; // 已透明像素不参与背景估计
      const r = data[o], g = data[o + 1], b = data[o + 2];
      const key = ((r >> 3) << 10) | ((g >> 3) << 5) | (b >> 3);
      let e = bins.get(key);
      if (!e) { e = [0, 0, 0, 0]; bins.set(key, e); }
      e[0]++; e[1] += r; e[2] += g; e[3] += b;
    };
    for (let y = 0; y < band; y++) {
      for (let x = 0; x < width; x++) { add(x, y); add(x, height - 1 - y); }
    }
    for (let x = 0; x < band; x++) {
      for (let y = band; y < height - band; y++) { add(x, y); add(width - 1 - x, y); }
    }
    let best = null;
    bins.forEach((e) => { if (!best || e[0] > best[0]) best = e; });
    if (!best || !best[0]) return [255, 255, 255];
    return [Math.round(best[1] / best[0]), Math.round(best[2] / best[0]), Math.round(best[3] / best[0])];
  }

  // 从四条边洪泛填充与背景色接近的连通区域
  function buildBackgroundMask(data, width, height, bg, tolerance) {
    const total = width * height;
    const mask = new Uint8Array(total);
    const queue = new Int32Array(total);
    let head = 0, tail = 0;
    const limit = tolerance * 1.25;
    const limit2 = limit * limit;
    const br = bg[0], bgc = bg[1], bb = bg[2];

    const push = (idx) => {
      if (mask[idx]) return;
      const o = idx * 4;
      if (data[o + 3] === 0) { mask[idx] = 1; queue[tail++] = idx; return; }
      const dr = data[o] - br, dg = data[o + 1] - bgc, db = data[o + 2] - bb;
      if (dr * dr + dg * dg + db * db <= limit2) { mask[idx] = 1; queue[tail++] = idx; }
    };

    for (let x = 0; x < width; x++) { push(x); push((height - 1) * width + x); }
    for (let y = 1; y < height - 1; y++) { push(y * width); push(y * width + width - 1); }

    while (head < tail) {
      const idx = queue[head++];
      const x = idx % width, y = (idx / width) | 0;
      if (x > 0) push(idx - 1);
      if (x + 1 < width) push(idx + 1);
      if (y > 0) push(idx - width);
      if (y + 1 < height) push(idx + width);
    }
    return { mask, queueSize: tail };
  }

  // 按颜色距离映射 Alpha；返回完全透明像素数。
  // 大面积背景距离为 0，用平方距离先分流，只有过渡带才算 sqrt。
  function applyAlpha(data, mask, width, height, bg, tolerance) {
    const t0 = tolerance;
    const t1 = tolerance * 1.25;
    const span = Math.max(1, t1 - t0);
    const t0sq = t0 * t0, t1sq = t1 * t1;
    const br = bg[0], bgc = bg[1], bb = bg[2];
    let removed = 0;
    for (let idx = 0; idx < mask.length; idx++) {
      if (!mask[idx]) continue;
      const o = idx * 4;
      const a0 = data[o + 3];
      if (a0 === 0) { removed++; continue; }
      const dr = data[o] - br, dg = data[o + 1] - bgc, db = data[o + 2] - bb;
      const d2 = dr * dr + dg * dg + db * db;
      let na;
      if (d2 <= t0sq) na = 0;
      else if (d2 >= t1sq) continue; // 洪泛阈值之外不会再进来，这里只兜底
      else na = Math.round(((Math.sqrt(d2) - t0) / span) * 255);
      if (na < a0) data[o + 3] = na;
      if (na === 0) removed++;
    }
    return removed;
  }

  // 可分离盒式模糊（水平 + 垂直），只改 Alpha 通道。
  // 先把 Alpha 抽成紧凑数组再模糊，避免每次访问都按 RGBA 步长计算，1600² 图可快数倍。
  function featherAlpha(data, width, height, radius) {
    if (radius <= 0) return;
    const total = width * height;
    const win = radius * 2 + 1;
    const alpha = new Uint8Array(total);
    const tmp = new Uint8Array(total);
    const row = width, col = height;

    for (let i = 0, o = 3; i < total; i++, o += 4) alpha[i] = data[o];

    for (let y = 0; y < height; y++) {
      const base = y * row;
      let sum = 0;
      for (let k = -radius; k <= radius; k++) {
        sum += alpha[base + (k < 0 ? 0 : (k >= row ? row - 1 : k))];
      }
      for (let x = 0; x < row; x++) {
        tmp[base + x] = (sum / win) | 0;
        sum += alpha[base + (x + radius + 1 >= row ? row - 1 : x + radius + 1)]
          - alpha[base + (x - radius < 0 ? 0 : x - radius)];
      }
    }
    for (let x = 0; x < width; x++) {
      let sum = 0;
      for (let k = -radius; k <= radius; k++) {
        sum += tmp[(k < 0 ? 0 : (k >= col ? col - 1 : k)) * row + x];
      }
      for (let y = 0; y < col; y++) {
        alpha[y * row + x] = (sum / win) | 0;
        sum += tmp[(y + radius + 1 >= col ? col - 1 : y + radius + 1) * row + x]
          - tmp[(y - radius < 0 ? 0 : y - radius) * row + x];
      }
    }
    for (let i = 0, o = 3; i < total; i++, o += 4) data[o] = alpha[i];
  }

  /* 主入口：原地修改 imageData.data，返回背景色与移除比例。
     imageData 需具备 { data: Uint8ClampedArray, width, height }。 */
  function removeBackground(imageData, options) {
    if (!imageData || !imageData.data || !imageData.width || !imageData.height) {
      throw new Error('缺少像素数据');
    }
    const width = imageData.width | 0, height = imageData.height | 0;
    if (width <= 0 || height <= 0) throw new Error('图片尺寸无效');
    if (width * height > MAX_PIXELS) throw new Error('图片像素过多，请先缩小到 4096 以内');

    const opts = options || {};
    const tolerance = clamp(opts.tolerance, 0, 255, DEFAULTS.tolerance);
    const feather = Math.round(clamp(opts.feather, 0, 20, DEFAULTS.feather));
    const edgeSample = Math.max(1, Math.round(clamp(opts.edgeSample, 1, 256, DEFAULTS.edgeSample)));
    const started = (global.performance && performance.now) ? performance.now() : Date.now();

    const data = imageData.data;
    const background = estimateBackgroundColor(data, width, height, edgeSample);
    const { mask, queueSize } = buildBackgroundMask(data, width, height, background, tolerance);
    const removed = applyAlpha(data, mask, width, height, background, tolerance);
    featherAlpha(data, width, height, feather);
    const elapsed = ((global.performance && performance.now) ? performance.now() : Date.now()) - started;

    return {
      width,
      height,
      backgroundColor: background,
      removedPixels: removed,
      removedRatio: removed / (width * height),
      maskedPixels: queueSize,
      elapsedMs: elapsed,
    };
  }

  global.BgRemover = Object.freeze({
    DEFAULTS,
    MAX_PIXELS,
    estimateBackgroundColor,
    buildBackgroundMask,
    applyAlpha,
    featherAlpha,
    removeBackground,
  });
})(window);
