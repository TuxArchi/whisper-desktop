#!/usr/bin/env node
// Generates NOTICES.md: every Rust and npm dependency with its licence and the
// full licence texts, deduplicated.
//
// The licence texts have to travel with the binary (MIT and LGPL both require
// it), and a hand-written list would be stale after the first new dependency.
//
//   node scripts/make-notices.mjs

import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const OWN = new Set(["wd-core", "wd-cli", "whisper-desktop"]);

function readText(path) {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return null;
  }
}

// --- Rust ------------------------------------------------------------------

function cargoPackages() {
  const lock = readText("Cargo.lock");
  if (!lock) throw new Error("no se encontró Cargo.lock");

  return lock
    .split("[[package]]")
    .slice(1)
    .map((block) => ({
      name: /name = "([^"]+)"/.exec(block)?.[1],
      version: /version = "([^"]+)"/.exec(block)?.[1],
    }))
    .filter((entry) => entry.name && entry.version && !OWN.has(entry.name));
}

function registryRoots() {
  const root = join(process.env.CARGO_HOME ?? join(homedir(), ".cargo"), "registry", "src");
  if (!existsSync(root)) return [];
  return readdirSync(root).map((entry) => join(root, entry));
}

function rustNotices() {
  const roots = registryRoots();
  const notices = [];

  for (const { name, version } of cargoPackages()) {
    let dir = null;
    for (const root of roots) {
      const candidate = join(root, `${name}-${version}`);
      if (existsSync(candidate)) {
        dir = candidate;
        break;
      }
    }
    if (!dir) continue;

    const manifest = readText(join(dir, "Cargo.toml")) ?? "";
    const licence = /^license\s*=\s*"([^"]*)"/m.exec(manifest)?.[1] ?? "desconocida";

    let text = null;
    for (const file of readdirSync(dir)) {
      if (/^(licen[cs]e|copying|copyright)/i.test(file)) {
        text = readText(join(dir, file));
        if (text) break;
      }
    }
    notices.push({ ecosystem: "rust", name, version, licence, text });
  }

  return notices;
}

// --- npm -------------------------------------------------------------------

function npmNotices() {
  const pkg = JSON.parse(readText("package.json"));
  const notices = [];

  for (const name of Object.keys(pkg.dependencies ?? {})) {
    const dir = join("node_modules", name);
    const manifest = readText(join(dir, "package.json"));
    if (!manifest) continue;

    const parsed = JSON.parse(manifest);
    let text = null;
    for (const file of readdirSync(dir)) {
      if (/^(licen[cs]e|copying)/i.test(file)) {
        text = readText(join(dir, file));
        if (text) break;
      }
    }
    notices.push({
      ecosystem: "npm",
      name,
      version: parsed.version ?? "?",
      licence: parsed.license ?? "desconocida",
      text,
    });
  }

  return notices;
}

// --- render ----------------------------------------------------------------

const notices = [...rustNotices(), ...npmNotices()].sort(
  (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
);

const texts = new Map();
for (const notice of notices) {
  if (!notice.text) continue;
  const hash = createHash("sha256").update(notice.text.trim()).digest("hex").slice(0, 12);
  if (!texts.has(hash)) texts.set(hash, { text: notice.text.trim(), names: [] });
  texts.get(hash).names.push(`${notice.name} ${notice.version}`);
}

const lines = [];
lines.push("# Avisos de terceros");
lines.push("");
lines.push(
  "Generado por `node scripts/make-notices.mjs`. No editar a mano: se regenera",
);
lines.push("en cada publicación.");
lines.push("");
lines.push("El código de Whisper Desktop es MIT (ver [LICENSE](LICENSE)).");
lines.push("Los binarios que distribuye la aplicación se listan en [THIRD-PARTY.md](THIRD-PARTY.md).");
lines.push("");
lines.push("## Dependencias");
lines.push("");
lines.push("| Componente | Versión | Licencia |");
lines.push("|---|---|---|");
for (const notice of notices) {
  lines.push(`| ${notice.name} | ${notice.version} | ${notice.licence} |`);
}
lines.push("");
lines.push("## Textos de licencia");
lines.push("");

let index = 0;
for (const [hash, entry] of texts) {
  index += 1;
  lines.push(`### Texto ${index} — cubre ${entry.names.length} componente(s)`);
  lines.push("");
  lines.push("<details><summary>Mostrar</summary>");
  lines.push("");
  lines.push("```");
  lines.push(entry.text);
  lines.push("```");
  lines.push("");
  lines.push("</details>");
  lines.push("");
  void hash;
}

writeFileSync("NOTICES.md", `${lines.join("\n")}\n`);
console.log(
  `NOTICES.md: ${notices.length} dependencias, ${texts.size} textos únicos`,
);
