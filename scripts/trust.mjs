#!/usr/bin/env node
// Ed25519 key management and catalogue signing.
//
// Uses only `node:crypto`, so CI and maintainers do not need an external
// binary. The public key is a raw 32-byte Ed25519 key in base64; the private
// key is a PKCS#8 PEM that must never be committed.
//
//   node scripts/trust.mjs keygen --out-dir ./keys
//   node scripts/trust.mjs sign --key ./keys/catalog.key --in catalog.json --out catalog.json.sig
//   node scripts/trust.mjs verify --pub <base64> --in catalog.json --sig catalog.json.sig

import { createPrivateKey, createPublicKey, generateKeyPairSync, sign, verify } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

function args(argv) {
  const out = { _: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i];
    if (key.startsWith("--")) {
      out[key.slice(2)] = argv[i + 1];
      i += 1;
    } else {
      out._.push(key);
    }
  }
  return out;
}

function rawPublicKey(publicKey) {
  // Ed25519 SPKI ends with the 32 raw bytes.
  const der = publicKey.export({ type: "spki", format: "der" });
  return der.subarray(der.length - 32);
}

function keygen(outDir) {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  mkdirSync(outDir, { recursive: true });
  const keyPath = join(outDir, "catalog.key");
  writeFileSync(keyPath, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });

  const pub = rawPublicKey(publicKey).toString("base64");
  writeFileSync(join(outDir, "catalog.pub"), `${pub}\n`);
  console.log(`clave privada: ${keyPath}`);
  console.log(`clave pública: ${pub}`);
}

function signFile(keyPath, input, output) {
  const key = createPrivateKey(readFileSync(keyPath));
  const signature = sign(null, readFileSync(input), key);
  writeFileSync(output, `${signature.toString("base64")}\n`);
  console.log(`firmado: ${output}`);
}

function verifyFile(publicKeyB64, input, signaturePath) {
  const raw = Buffer.from(publicKeyB64.trim(), "base64");
  const spki = Buffer.concat([
    Buffer.from("302a300506032b6570032100", "hex"),
    raw,
  ]);
  const key = createPublicKey({ key: spki, format: "der", type: "spki" });
  const signature = Buffer.from(readFileSync(signaturePath, "utf8").trim(), "base64");
  const ok = verify(null, readFileSync(input), key, signature);
  console.log(ok ? "firma válida" : "FIRMA INVÁLIDA");
  process.exit(ok ? 0 : 1);
}

const options = args(process.argv.slice(2));
const command = options._[0];

try {
  if (command === "keygen") {
    keygen(options["out-dir"] ?? "keys");
  } else if (command === "sign") {
    signFile(options.key, options.in, options.out);
  } else if (command === "verify") {
    verifyFile(options.pub, options.in, options.sig);
  } else {
    console.error("uso: trust.mjs <keygen|sign|verify> [opciones]");
    process.exit(2);
  }
} catch (error) {
  console.error(error.message);
  process.exit(1);
}

// Keep the import used when only `keygen` runs.
void dirname;
