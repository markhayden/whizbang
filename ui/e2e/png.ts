// A small PNG whose header says it is large: 10,000 by 10,000 pixels of one bit of grey,
// all zero, which deflates to a few kilobytes. It is a whole, valid picture of 100,000,000
// pixels, twice the core's limit of 50,000,000 (`DecodeLimits`), so an upload of it must be
// refused before it is decoded.
import { crc32, deflateSync } from "node:zlib";

function chunk(type: string, data: Buffer): Buffer {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
  const sum = Buffer.alloc(4);
  sum.writeUInt32BE(crc32(body) >>> 0);
  return Buffer.concat([length, body, sum]);
}

export function largePng(width = 10_000, height = 10_000): Buffer {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 1; // bit depth
  header[9] = 0; // grey
  const row = 1 + Math.ceil(width / 8); // a filter byte, then the row's bits
  const pixels = deflateSync(Buffer.alloc(row * height), { level: 9 });
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", header), chunk("IDAT", pixels), chunk("IEND", Buffer.alloc(0))]);
}
