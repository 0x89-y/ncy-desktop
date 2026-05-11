// Generate a 1024x1024 PNG with the Ncy logo glyph (purple bg, white "N").
// No deps — emits raw PNG via zlib.
import { writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const SIZE = 1024;
const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(here, "icon.png");

// 8x8 bitmap font for "N" — 1 = ink, 0 = bg.
const glyph = [
  "10000001",
  "11000001",
  "10100001",
  "10010001",
  "10001001",
  "10000101",
  "10000011",
  "10000001",
].map((row) => row.split("").map(Number));

const BG = [0x67, 0x50, 0xa4]; // M3 primary purple
const FG = [0xff, 0xff, 0xff];

const pixels = Buffer.alloc(SIZE * SIZE * 4);
const cell = SIZE / 8;
const inset = cell * 0.4;
for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    const gx = Math.floor(x / cell);
    const gy = Math.floor(y / cell);
    const within =
      x % cell >= inset / 2 &&
      x % cell < cell - inset / 2 &&
      y % cell >= inset / 2 &&
      y % cell < cell - inset / 2;
    const ink = glyph[gy]?.[gx] === 1 && within;
    const [r, g, b] = ink ? FG : BG;
    const i = (y * SIZE + x) * 4;
    pixels[i] = r;
    pixels[i + 1] = g;
    pixels[i + 2] = b;
    pixels[i + 3] = 0xff;
  }
}

// Wrap each row with a filter byte (0 = none).
const rowBytes = SIZE * 4;
const filtered = Buffer.alloc(SIZE * (rowBytes + 1));
for (let y = 0; y < SIZE; y++) {
  filtered[y * (rowBytes + 1)] = 0;
  pixels.copy(filtered, y * (rowBytes + 1) + 1, y * rowBytes, (y + 1) * rowBytes);
}
const idatData = deflateSync(filtered);

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crcInput = Buffer.concat([typeBuf, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(crcInput) >>> 0, 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

function crc32(buf) {
  let c;
  let crc = 0xffffffff;
  for (const byte of buf) {
    c = (crc ^ byte) & 0xff;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc = (crc >>> 8) ^ c;
  }
  return crc ^ 0xffffffff;
}

const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8;        // bit depth
ihdr[9] = 6;        // colour type RGBA
ihdr[10] = 0;       // compression
ihdr[11] = 0;       // filter
ihdr[12] = 0;       // interlace

const png = Buffer.concat([
  sig,
  chunk("IHDR", ihdr),
  chunk("IDAT", idatData),
  chunk("IEND", Buffer.alloc(0)),
]);

writeFileSync(out, png);
console.log("wrote", out, png.length, "bytes");
