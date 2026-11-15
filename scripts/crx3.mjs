// Wraps the extension zip into a CRX3 package Chrome/Edge accept.
//
// Usage: node scripts/crx3.mjs <extension.zip> <private-key.pem> <out.crx>
//
// The key pair is generated once (see extension/README.md). The public key is
// pinned in manifest.json as "key", which fixes the extension id; without that
// the browser would hand out a new id on every install and the policy entry
// that points at update.xml would stop matching.

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const [zipPath, keyPath, outPath] = process.argv.slice(2);
if (!zipPath || !keyPath || !outPath) {
  console.error("usage: node scripts/crx3.mjs <zip> <key.pem> <out.crx>");
  process.exit(2);
}

const MAGIC = Buffer.from("Cr24", "ascii");
const SIGNATURE_CONTEXT = Buffer.from("CRX3 SignedData\x00", "binary");

/** Minimal protobuf writer: only length-delimited fields are needed here. */
function varint(value) {
  const bytes = [];
  let rest = value;
  while (rest > 0x7f) {
    bytes.push((rest & 0x7f) | 0x80);
    rest = Math.floor(rest / 128);
  }
  bytes.push(rest);
  return Buffer.from(bytes);
}

function field(number, payload) {
  const body = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
  return Buffer.concat([varint(number * 8 + 2), varint(body.length), body]);
}

function message(parts) {
  return Buffer.concat(parts);
}

const zip = fs.readFileSync(zipPath);
const privateKey = crypto.createPrivateKey(fs.readFileSync(keyPath));
const publicKey = crypto.createPublicKey(privateKey);
const spki = publicKey.export({ type: "spki", format: "der" });

// Chrome derives the id from the first 16 bytes of SHA-256 over the public key.
const crxId = crypto.createHash("sha256").update(spki).digest().subarray(0, 16);

// SignedData { bytes crx_id = 1; }
const signedHeaderData = message([field(1, crxId)]);

const signature = crypto
  .createSign("RSA-SHA256")
  .update(SIGNATURE_CONTEXT)
  .update(uint32le(signedHeaderData.length))
  .update(signedHeaderData)
  .update(zip)
  .sign(privateKey);

// AsymmetricKeyProof { bytes public_key = 1; bytes signature = 2; }
const proof = message([field(1, spki), field(2, signature)]);
// CrxFileHeader { repeated sha256_with_rsa = 2; SignedData signed_header_data = 10000; }
const header = message([field(2, proof), field(10000, signedHeaderData)]);

const crx = Buffer.concat([MAGIC, uint32le(3), uint32le(header.length), header, zip]);
fs.writeFileSync(outPath, crx);

const id = [...crxId]
  .map((byte) => String.fromCharCode(97 + (byte >> 4)) + String.fromCharCode(97 + (byte & 15)))
  .join("");
// The desktop app reads this file when it answers update.xml.
fs.writeFileSync(`${path.dirname(outPath)}/id.txt`, `${id}\n`);
console.log(`CRX3 ${outPath} (${crx.length} bytes, extension id ${id})`);

function uint32le(value) {
  const buffer = Buffer.alloc(4);
  buffer.writeUInt32LE(value >>> 0, 0);
  return buffer;
}