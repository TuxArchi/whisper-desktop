import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync, rmSync, readFileSync } from "node:fs";
import { join } from "node:path";
import assert from "node:assert/strict";
import test from "node:test";

import { buildCatalog, validateCatalog } from "./make-catalog.mjs";

const VERSION = "1.8.4";
const TMP = join(process.env.TMPDIR ?? "/tmp", `wd-catalog-test-${process.pid}`);

function fakeArtifact(name, content) {
  const path = join(TMP, name);
  writeFileSync(path, content);
  return path;
}

const manifest = {
  version: VERSION,
  minAppVersion: "0.1.0",
  backends: [
    {
      id: "vulkan",
      platform: "windows-x86_64",
      file: `whisper-{version}-windows-x64-vulkan.zip`,
      priority: 20,
    },
    {
      id: "cpu",
      platform: "linux-x86_64",
      file: `whisper-{version}-linux-x64-cpu.tar.gz`,
      priority: 10,
    },
  ],
  tools: [
    {
      id: "ffmpeg-lgpl",
      platform: "linux-x86_64",
      version: "n7.1",
      file: "ffmpeg-lgpl-linux64.tar.gz",
    },
  ],
};

test.before(() => {
  mkdirSync(TMP, { recursive: true });
  fakeArtifact(`whisper-${VERSION}-windows-x64-vulkan.zip`, "windows-vulkan");
  fakeArtifact(`whisper-${VERSION}-linux-x64-cpu.tar.gz`, "linux-cpu");
  fakeArtifact("ffmpeg-lgpl-linux64.tar.gz", "ffmpeg");
});

test.after(() => {
  rmSync(TMP, { recursive: true, force: true });
});

test("hashes every artifact and builds release URLs", () => {
  const { catalog } = buildCatalog({
    artifactsDir: TMP,
    manifest,
    repo: "acme/whisper-desktop",
    release: "backends-r1",
    models: [],
  });

  assert.equal(catalog.schema, 1);
  assert.equal(catalog.backends.length, 2);
  assert.equal(catalog.tools.length, 1);

  const vulkan = catalog.backends.find((b) => b.id === "vulkan");
  const expected = createHash("sha256").update("windows-vulkan").digest("hex");
  assert.equal(vulkan.artifact.sha256, expected);
  assert.equal(vulkan.artifact.size, Buffer.byteLength("windows-vulkan"));
  assert.equal(
    vulkan.artifact.url,
    `https://github.com/acme/whisper-desktop/releases/download/backends-r1/whisper-${VERSION}-windows-x64-vulkan.zip`,
  );
  assert.match(vulkan.artifact.sha256, /^[0-9a-f]{64}$/);
  assert.deepEqual(vulkan.env, {});
});

test("produces a catalogue the app accepts", () => {
  const { catalog } = buildCatalog({
    artifactsDir: TMP,
    manifest,
    repo: "acme/whisper-desktop",
    release: "backends-r1",
    models: [
      {
        id: "small",
        file: "ggml-small.bin",
        url: "https://huggingface.co/ggml-org/whisper.cpp/resolve/main/ggml-small.bin",
        sha256: "a".repeat(64),
        size: 487601967,
        vramMb: 1024,
        multilingual: true,
      },
    ],
  });
  assert.deepEqual(validateCatalog(catalog), []);
});

test("skips missing artifacts when allowed", () => {
  const { catalog, skipped } = buildCatalog({
    artifactsDir: TMP,
    manifest: {
      ...manifest,
      backends: [
        { id: "cuda", platform: "windows-x86_64", file: "no-existe.zip", priority: 30 },
        { id: "vulkan", platform: "linux-x86_64", file: manifest.backends[0].file, priority: 20 },
      ],
      tools: [],
    },
    repo: "acme/whisper-desktop",
    release: "backends-r1",
    allowMissing: true,
  });

  assert.equal(catalog.backends.length, 1);
  assert.equal(catalog.backends[0].id, "vulkan");
  assert.deepEqual(skipped, ["no-existe.zip"]);
  assert.deepEqual(validateCatalog(catalog), []);
});

test("fails loudly when an artifact is missing", () => {
  assert.throws(
    () =>
      buildCatalog({
        artifactsDir: TMP,
        manifest: {
          ...manifest,
          backends: [
            { id: "cuda", platform: "windows-x86_64", file: "no-existe.zip", priority: 30 },
          ],
          tools: [],
        },
        repo: "acme/whisper-desktop",
        release: "backends-r1",
      }),
    /falta el artefacto/,
  );
});

test("rejects a catalogue with placeholder hashes", () => {
  const { catalog } = buildCatalog({
    artifactsDir: TMP,
    manifest,
    repo: "acme/whisper-desktop",
    release: "backends-r1",
  });
  catalog.backends[0].artifact.sha256 = "PENDING";
  const errors = validateCatalog(catalog);
  assert.equal(errors.length, 1);
  assert.match(errors[0], /sha256 inválido/);
});

test("rejects insecure URLs", () => {
  const { catalog } = buildCatalog({
    artifactsDir: TMP,
    manifest,
    repo: "acme/whisper-desktop",
    release: "backends-r1",
  });
  catalog.backends[0].artifact.url = "http://example.com/x.zip";
  assert.match(validateCatalog(catalog)[0], /HTTPS/);
});

test("the checked-in catalogue passes the same validation as the app", () => {
  const raw = readFileSync(join(import.meta.dirname, "..", "catalog", "catalog.json"), "utf8");
  const catalog = JSON.parse(raw);
  assert.deepEqual(validateCatalog(catalog), []);
});
