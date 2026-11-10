// Tests du script de préparation de llama-server (sans réseau).
// Lancer : node --test scripts/*.test.mjs
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { alreadyInstalled, defaultVariant, hostPlatform, install, isShipped, pickAsset, resolveTargetKey } from "./fetch-llama.mjs";

const SCRIPT = fileURLToPath(new URL("./fetch-llama.mjs", import.meta.url));

test("plateformes et variantes", () => {
  assert.equal(hostPlatform("linux", "x64"), "linux-x64");
  assert.equal(hostPlatform("darwin", "arm64"), "macos-arm64");
  assert.equal(hostPlatform("win32", "x64"), "win-x64");
  assert.throws(() => hostPlatform("freebsd", "x64"));
  assert.equal(defaultVariant("macos-arm64"), "metal");
  assert.equal(defaultVariant("win-x64"), "cpu");
  assert.equal(resolveTargetKey({ target: "x86_64-pc-windows-msvc", variant: "vulkan" }), "win-x64/vulkan");
  assert.equal(resolveTargetKey({ target: "aarch64-apple-darwin" }), "macos-arm64/metal");
  assert.throws(() => resolveTargetKey({ target: "riscv64gc-unknown-linux-gnu" }));
});

test("une seule archive doit correspondre au motif", () => {
  const assets = [{ name: "llama-v1-bin-ubuntu-x64.tar.gz" }, { name: "llama-v1-bin-ubuntu-vulkan-x64.tar.gz" }];
  assert.equal(pickAsset(assets, "-bin-ubuntu-x64\\.(tar\\.gz|zip)$").name, "llama-v1-bin-ubuntu-x64.tar.gz");
  assert.throws(() => pickAsset(assets, "-bin-ubuntu-"), /2 archive/);
  assert.throws(() => pickAsset(assets, "-bin-win-"), /0 archive/);
});

test("seuls le serveur, ses bibliothèques et les licences sont livrés", () => {
  for (const name of ["llama-server", "llama-server.exe", "libggml.so", "libllama.so.0", "ggml-vulkan.dll", "libmtmd.dylib", "LICENSE", "LICENSE-curl"]) {
    assert.ok(isShipped(name), name);
  }
  for (const name of ["llama-cli", "llama-quantize.exe", "rpc-server", "README.md", "convert.py"]) assert.ok(!isShipped(name), name);
});

test("installation depuis une archive extraite", () => {
  const dir = mkdtempSync(join(tmpdir(), "hf-test-"));
  const bin = join(dir, "x", "build", "bin");
  mkdirSync(bin, { recursive: true });
  for (const f of ["llama-server", "llama-cli", "libllama.so", "libggml-base.so"]) writeFileSync(join(bin, f), f);
  writeFileSync(join(dir, "x", "LICENSE"), "MIT");
  const dest = join(dir, "dest");
  mkdirSync(join(dir, "x", "build", "lib"));
  writeFileSync(join(dir, "x", "build", "lib", "libmtmd.so"), "lib");
  assert.deepEqual(install(join(dir, "x"), dest), ["LICENSE", "libggml-base.so", "libllama.so", "libmtmd.so", "llama-server"]);
  assert.ok(!existsSync(join(dest, "llama-cli")));
});

test("bout en bout avec une archive locale et une empreinte vérifiée", { skip: process.platform === "win32" }, () => {
  const dir = mkdtempSync(join(tmpdir(), "hf-test-"));
  mkdirSync(join(dir, "pkg", "bin"), { recursive: true });
  writeFileSync(join(dir, "pkg", "bin", "llama-server"), "#!/bin/sh\n");
  writeFileSync(join(dir, "pkg", "bin", "libllama.so"), "lib");
  const archive = join(dir, "llama-vX-bin-ubuntu-x64.tar.gz");
  execFileSync("tar", ["-czf", archive, "-C", join(dir, "pkg"), "bin"]);
  const sha256 = createHash("sha256").update(readFileSync(archive)).digest("hex");
  const entry = (sha) => ({ pattern: "x", file: "llama-vX-bin-ubuntu-x64.tar.gz", sha256: sha });
  const lock = (sha) => ({ repo: "o/r", tag: "vX", targets: { "linux-x64/cpu": [entry(sha)] } });
  const dest = join(dir, "dest");
  mkdirSync(dest);
  writeFileSync(join(dest, "README.md"), "gardé");
  const run = (sha) => {
    writeFileSync(join(dir, "lock.json"), JSON.stringify(lock(sha)));
    return execFileSync("node", [SCRIPT, "--target", "x86_64-unknown-linux-gnu", "--from", archive], {
      env: { ...process.env, HF_LLAMA_LOCK: join(dir, "lock.json"), HF_LLAMA_DEST: dest },
      stdio: "pipe",
    });
  };

  run(sha256);
  assert.deepEqual(readdirSync(dest).sort(), ["README.md", "VERSION.json", "libllama.so", "llama-server"]);
  assert.equal(JSON.parse(readFileSync(join(dest, "VERSION.json"), "utf8")).tag, "vX");

  assert.throws(() => run("0".repeat(64)), /empreinte SHA-256 incorrecte/);
  assert.ok(alreadyInstalled(dest, "vX", "linux-x64/cpu"), "une archive refusée ne touche pas à l'installation en place");

  // --if-missing : la bonne version est déjà là, rien n'est refait (même avec une empreinte fausse).
  run(sha256);
  writeFileSync(join(dir, "lock.json"), JSON.stringify(lock("0".repeat(64))));
  execFileSync("node", [SCRIPT, "--target", "x86_64-unknown-linux-gnu", "--from", archive, "--if-missing"], {
    env: { ...process.env, HF_LLAMA_LOCK: join(dir, "lock.json"), HF_LLAMA_DEST: dest },
    stdio: "pipe",
  });
  assert.ok(alreadyInstalled(dest, "vX", "linux-x64/cpu"));
  assert.ok(!alreadyInstalled(dest, "vX", "linux-x64/vulkan") && alreadyInstalled(dest, "vX", "linux-x64/vulkan", true));
  assert.ok(!alreadyInstalled(dest, "vY", "linux-x64/cpu") && !alreadyInstalled(dest, "vX", "win-x64/cpu", true));
  // --optional : un échec n'arrête pas le build.
  execFileSync("node", [SCRIPT, "--target", "x86_64-unknown-linux-gnu", "--from", archive, "--optional"], {
    env: { ...process.env, HF_LLAMA_LOCK: join(dir, "lock.json"), HF_LLAMA_DEST: dest },
    stdio: "pipe",
  });
});
