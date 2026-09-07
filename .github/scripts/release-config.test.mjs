import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const config = JSON.parse(read("apps/desktop/src-tauri/tauri.conf.json"));
const repository = "Galaxy-A/cursor-byok";

test("desktop version agrees across package manifests and lockfiles", () => {
  const version = config.version;
  const pkg = JSON.parse(read("apps/desktop/package.json"));
  const lock = JSON.parse(read("apps/desktop/package-lock.json"));
  assert.equal(pkg.version, version);
  assert.equal(lock.version, version);
  assert.equal(lock.packages[""].version, version);
  assert.match(
    read("apps/desktop/src-tauri/Cargo.toml"),
    new RegExp(`^version = "${version.replaceAll(".", "\\.")}"$`, "m"),
  );
  const desktop = read("Cargo.lock")
    .split("[[package]]")
    .find((entry) => entry.includes('name = "cursor-byok-desktop"'));
  assert.ok(desktop);
  assert.ok(desktop.includes(`version = "${version}"`));
});

test("both desktop update channels use the fork release assets", () => {
  assert.deepEqual(config.plugins.updater.endpoints, [
    `https://github.com/${repository}/releases/latest/download/latest.json`,
  ]);
  const portable = read("apps/desktop/src-tauri/src/update/mod.rs");
  assert.ok(
    portable.includes(
      `https://github.com/${repository}/releases/latest/download/portable-latest.json`,
    ),
  );
  assert.doesNotMatch(portable, /https:\/\/github\.com\/leookun\/cursor-byok\/releases/);
  const publicKey = Buffer.from(config.plugins.updater.pubkey, "base64")
    .toString("utf8")
    .trim()
    .split(/\r?\n/);
  assert.equal(publicKey.length, 2);
  assert.equal(Buffer.from(publicKey[1], "base64").length, 42);
});
