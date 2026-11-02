#!/usr/bin/env node
// Builds catalog.json from the release artifacts.
//
// This is the step that can silently break every future install, so it does
// two things on purpose:
//   1. hashes the files itself instead of trusting anything typed by hand;
//   2. re-validates the output with the same rules the app applies, and
//      exits non-zero when the catalogue would be rejected at runtime.

import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { pathToFileURL } from "node:url";

const HEX64 = /^[0-9a-f]{64}$/;

export function sha256(path) {
  const hash = createHash("sha256");
  hash.update(readFileSync(path));
  return hash.digest("hex");
}

export function hashArtifact(artifactsDir, file, baseUrl) {
  const path = join(artifactsDir, file);
  if (!existsSync(path)) {
    throw new Error(`falta el artefacto: ${path}`);
  }
  const size = statSync(path).size;
  if (size === 0) {
    throw new Error(`artefacto vacío: ${path}`);
  }
  return {
    url: `${baseUrl}/${basename(file)}`,
    sha256: sha256(path),
    size,
  };
}

/**
 * @param {object} options
 * @param {string} options.artifactsDir
 * @param {object} options.manifest       parsed backends.manifest.json
 * @param {string} options.repo           "owner/name"
 * @param {string} options.release        tag, e.g. "backends-r1"
 * @param {Array}  [options.models]       already hashed model entries
 * @param {string} [options.channel]
 */
export function buildCatalog({ artifactsDir, manifest, repo, release, models = [], channel = "stable" }) {
  const baseUrl = `https://github.com/${repo}/releases/download/${release}`;

  const backends = manifest.backends.map((entry) => ({
    id: entry.id,
    platform: entry.platform,
    priority: entry.priority ?? 10,
    artifact: hashArtifact(artifactsDir, entry.file.replace("{version}", manifest.version), baseUrl),
    env: entry.env ?? {},
  }));

  const tools = (manifest.tools ?? []).map((entry) => ({
    id: entry.id,
    platform: entry.platform,
    version: entry.version ?? null,
    artifact: hashArtifact(artifactsDir, entry.file.replace("{version}", manifest.version), baseUrl),
  }));

  return {
    schema: 1,
    channel,
    generated: new Date().toISOString(),
    minAppVersion: manifest.minAppVersion ?? "0.1.0",
    backends,
    tools,
    models,
  };
}

/**
 * Mirrors `Catalog::validate` in crates/wd-core/src/catalog.rs.
 * Returns a list of problems; empty means the app will accept it.
 */
export function validateCatalog(catalog) {
  const errors = [];

  if (catalog.schema !== 1) {
    errors.push(`esquema no soportado: ${catalog.schema}`);
  }

  for (const backend of catalog.backends ?? []) {
    checkArtifact(backend.artifact, `backend ${backend.id}/${backend.platform}`, errors);
  }
  for (const tool of catalog.tools ?? []) {
    checkArtifact(tool.artifact, `tool ${tool.id}/${tool.platform}`, errors);
  }
  for (const model of catalog.models ?? []) {
    if (!String(model.url).startsWith("https://")) {
      errors.push(`modelo ${model.id}: URL sin HTTPS`);
    }
    if (!HEX64.test(model.sha256 ?? "")) {
      errors.push(`modelo ${model.id}: sha256 inválido`);
    }
    if (!model.size) {
      errors.push(`modelo ${model.id}: sin tamaño`);
    }
  }

  return errors;
}

function checkArtifact(artifact, label, errors) {
  if (!artifact) {
    errors.push(`${label}: sin artefacto`);
    return;
  }
  if (!String(artifact.url).startsWith("https://")) {
    errors.push(`${label}: URL sin HTTPS`);
  }
  if (!HEX64.test(artifact.sha256 ?? "")) {
    errors.push(`${label}: sha256 inválido`);
  }
  if (!artifact.size) {
    errors.push(`${label}: sin tamaño`);
  }
}

// --- CLI -------------------------------------------------------------------

function parseArgs(argv) {
  const options = {
    artifactsDir: "dist",
    manifestPath: "scripts/backends.manifest.json",
    modelsPath: null,
    repo: process.env.GITHUB_REPOSITORY ?? "TuxArchi/whisper-desktop",
    release: process.env.GITHUB_REF_NAME ?? "backends-r1",
    out: "catalog.json",
    channel: "stable",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    const value = argv[index + 1];
    switch (key) {
      case "--artifacts":
        options.artifactsDir = value;
        index += 1;
        break;
      case "--manifest":
        options.manifestPath = value;
        index += 1;
        break;
      case "--models":
        options.modelsPath = value;
        index += 1;
        break;
      case "--repo":
        options.repo = value;
        index += 1;
        break;
      case "--release":
        options.release = value;
        index += 1;
        break;
      case "--out":
        options.out = value;
        index += 1;
        break;
      case "--channel":
        options.channel = value;
        index += 1;
        break;
      case "--help":
        console.log(
          "uso: make-catalog.mjs [--artifacts dist] [--manifest scripts/backends.manifest.json] " +
            "[--models models.json] [--repo owner/name] [--release tag] [--out catalog.json]",
        );
        process.exit(0);
        break;
      default:
        throw new Error(`argumento desconocido: ${key}`);
    }
  }
  return options;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const manifest = JSON.parse(readFileSync(options.manifestPath, "utf8"));
  const models = options.modelsPath
    ? JSON.parse(readFileSync(options.modelsPath, "utf8"))
    : [];

  const catalog = buildCatalog({ ...options, manifest, models });
  const errors = validateCatalog(catalog);

  if (errors.length > 0) {
    console.error("catálogo inválido:");
    for (const error of errors) console.error(`  - ${error}`);
    process.exit(1);
  }

  writeFileSync(options.out, `${JSON.stringify(catalog, null, 2)}\n`);
  console.log(
    `catalog.json escrito con ${catalog.backends.length} backends, ` +
      `${catalog.tools.length} herramientas y ${catalog.models.length} modelos`,
  );
}

// `file://${path}` breaks with spaces and non-ASCII characters; the
// URL form handles both.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
