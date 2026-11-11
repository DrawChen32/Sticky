// 便利贴图标：一套几何、九种尺寸。无第三方依赖，自己栅格化 + 手写 PNG/ICO/ICNS。
// 形状与设计稿 05 应用图标 完全一致。
const fs = require("fs");
const path = require("path");
const zlib = require("zlib");

const OUT = path.join(__dirname, "..", "src-tauri", "icons");
const S = 256; // 设计坐标

// ---------- 基础几何（SDF，返回 <0 表示在形状内） ----------
const sdRoundRect = (x, y, cx, cy, hw, hh, r) => {
  const qx = Math.abs(x - cx) - (hw - r);
  const qy = Math.abs(y - cy) - (hh - r);
  const ax = Math.max(qx, 0);
  const ay = Math.max(qy, 0);
  return Math.hypot(ax, ay) + Math.min(Math.max(qx, qy), 0) - r;
};

// 纸片：圆角矩形，右下角被对角线切掉（折角）
const paper = (x, y) => {
  const body = sdRoundRect(x, y, 128, 128, 104, 104, 28);
  const cut = (x + y - 400) / Math.SQRT2;
  return Math.max(body, cut);
};

const bars = [
  [64, 88, 112, 15],
  [64, 125, 112, 15],
  [64, 162, 64, 15],
];
const barDist = (x, y) => {
  let d = 1e9;
  for (const [bx, by, bw, bh] of bars) {
    d = Math.min(d, sdRoundRect(x, y, bx + bw / 2, by + bh / 2, bw / 2, bh / 2, bh / 2));
  }
  return d;
};

const PAPER = [251, 243, 217];
const LINE = [223, 208, 164];
const INK = [201, 183, 127];
const HALF_STROKE = 4.5;

function shade(x, y) {
  const d = paper(x, y);
  if (d > 0) return null;
  if (d > -HALF_STROKE) return LINE;
  if (barDist(x, y) <= 0) return INK;
  return PAPER;
}

// ---------- 栅格化（4x4 超采样） ----------
function render(size) {
  const buf = Buffer.alloc(size * size * 4);
  const scale = S / size;
  const N = 4;
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let r = 0, g = 0, b = 0, cover = 0;
      for (let sy = 0; sy < N; sy++) {
        for (let sx = 0; sx < N; sx++) {
          const x = (px + (sx + 0.5) / N) * scale;
          const y = (py + (sy + 0.5) / N) * scale;
          const c = shade(x, y);
          if (c) {
            r += c[0]; g += c[1]; b += c[2]; cover++;
          }
        }
      }
      const total = N * N;
      const i = (py * size + px) * 4;
      if (cover === 0) {
        buf[i] = buf[i + 1] = buf[i + 2] = buf[i + 3] = 0;
      } else {
        buf[i] = Math.round(r / cover);
        buf[i + 1] = Math.round(g / cover);
        buf[i + 2] = Math.round(b / cover);
        buf[i + 3] = Math.round((cover / total) * 255);
      }
    }
  }
  return buf;
}

// ---------- PNG 编码 ----------
const crcTable = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = crcTable[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
function png(size, rgba) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    rgba.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// ---------- ICO / ICNS ----------
function ico(entries) {
  const head = Buffer.alloc(6);
  head.writeUInt16LE(0, 0);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(entries.length, 4);
  const dir = Buffer.alloc(16 * entries.length);
  let offset = 6 + 16 * entries.length;
  entries.forEach((e, i) => {
    const b = i * 16;
    dir[b] = e.size >= 256 ? 0 : e.size;
    dir[b + 1] = e.size >= 256 ? 0 : e.size;
    dir[b + 2] = 0;
    dir[b + 3] = 0;
    dir.writeUInt16LE(1, b + 4);
    dir.writeUInt16LE(32, b + 6);
    dir.writeUInt32LE(e.png.length, b + 8);
    dir.writeUInt32LE(offset, b + 12);
    offset += e.png.length;
  });
  return Buffer.concat([head, dir, ...entries.map((e) => e.png)]);
}

function icns(pairs) {
  const chunks = pairs.map(([type, data]) => {
    const h = Buffer.alloc(8);
    h.write(type, 0, 4, "ascii");
    h.writeUInt32BE(data.length + 8, 4);
    return Buffer.concat([h, data]);
  });
  const body = Buffer.concat(chunks);
  const head = Buffer.alloc(8);
  head.write("icns", 0, 4, "ascii");
  head.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([head, body]);
}

// ---------- 输出 ----------
fs.mkdirSync(OUT, { recursive: true });
const cache = new Map();
const get = (s) => {
  if (!cache.has(s)) cache.set(s, render(s));
  return cache.get(s);
};

for (const s of [32, 64, 128, 256, 512, 1024]) {
  fs.writeFileSync(path.join(OUT, `icon-${s}.png`), png(s, get(s)));
}
fs.writeFileSync(path.join(OUT, "32x32.png"), png(32, get(32)));
fs.writeFileSync(path.join(OUT, "128x128.png"), png(128, get(128)));
fs.writeFileSync(path.join(OUT, "128x128@2x.png"), png(256, get(256)));
fs.writeFileSync(path.join(OUT, "icon.png"), png(512, get(512)));
fs.writeFileSync(
  path.join(OUT, "icon.ico"),
  ico([16, 32, 48, 64, 128, 256].map((s) => ({ size: s, png: png(s, get(s)) })))
);
fs.writeFileSync(
  path.join(OUT, "icon.icns"),
  icns([
    ["ic07", png(128, get(128))],
    ["ic08", png(256, get(256))],
    ["ic09", png(512, get(512))],
  ])
);

console.log("icons written to", OUT);
for (const f of fs.readdirSync(OUT)) {
  console.log(" ", f, fs.statSync(path.join(OUT, f)).size, "bytes");
}
