#!/usr/bin/env node
// Regenerates scripts/models.json with the real SHA-256 of every model.
//
// HuggingFace exposes the hash as `lfs.oid` in the tree API, which is the
// SHA-256 of the file. Do not use the CDN `ETag`: it is not the content hash
// (verified against a downloaded file) and would publish wrong hashes.
//
//   node scripts/fetch-model-hashes.mjs

import { writeFileSync } from "node:fs";

const REPO = "ggerganov/whisper.cpp";
const API = `https://huggingface.co/api/models/${REPO}/tree/main?recursive=true`;
const RESOLVE = `https://huggingface.co/${REPO}/resolve/main`;

// VRAM estimates come from docs/HARDWARE-MATRIX.md.
const WANTED = [
  { id: "base", file: "ggml-base.bin", vramMb: 640 },
  { id: "small", file: "ggml-small.bin", vramMb: 1024 },
  { id: "medium", file: "ggml-medium.bin", vramMb: 2560 },
  { id: "large-v3-turbo", file: "ggml-large-v3-turbo.bin", vramMb: 3072 },
  { id: "large-v3", file: "ggml-large-v3.bin", vramMb: 5120 },
];

const response = await fetch(API);
if (!response.ok) {
  console.error(`la API respondió ${response.status}`);
  process.exit(1);
}

const tree = await response.json();
const byPath = new Map();
for (const entry of tree) {
  if (entry.type !== "file" || !entry.lfs) continue;
  byPath.set(entry.path, entry.lfs);
}

const models = [];
for (const wanted of WANTED) {
  const lfs = byPath.get(wanted.file);
  if (!lfs) {
    console.warn(`aviso: ${wanted.file} no está en ${REPO}`);
    continue;
  }
  if (!/^[0-9a-f]{64}$/.test(lfs.oid)) {
    console.error(`${wanted.file}: oid inesperado ${lfs.oid}`);
    process.exit(1);
  }
  models.push({
    id: wanted.id,
    file: wanted.file,
    url: `${RESOLVE}/${wanted.file}`,
    sha256: lfs.oid,
    size: lfs.size,
    vramMb: wanted.vramMb,
    multilingual: true,
  });
}

if (models.length === 0) {
  console.error("no se encontró ningún modelo");
  process.exit(1);
}

writeFileSync("scripts/models.json", `${JSON.stringify(models, null, 2)}\n`);
console.log(`scripts/models.json: ${models.length} modelos`);
for (const model of models) {
  console.log(`  ${model.id.padEnd(16)} ${model.sha256.slice(0, 16)}… ${model.size} bytes`);
}
