import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const config = JSON.parse(read("apps/desktop/src-tauri/tauri.conf.json"));
const repository = "Galaxy-A/cursor-byok";

test("main builds downloadable bundles without publishing a release", () => {
  const ci = read(".github/workflows/ci.yml");
  assert.ok(ci.includes("needs: [rust, frontend, desktop-rust]"));
  assert.ok(ci.includes(
    "if: github.event_name == 'push' && (github.ref == 'refs/heads/main' || startsWith(github.ref, 'refs/heads/Test/')) && github.repository == 'Galaxy-A/cursor-byok'",
  ));
  assert.ok(ci.includes("uses: actions/upload-artifact@v4"));
  assert.ok(ci.includes("retention-days: 7"));
  assert.doesNotMatch(ci, /releaseDraft:|tagName:|gh release/);
});

test("formal releases require a version tag contained in main", () => {
  const release = read(".github/workflows/release.yml");
  assert.match(release, /on:\s+push:\s+tags:\s+- "v\*"/);
  assert.doesNotMatch(release, /workflow_dispatch:|branches:/);
  assert.ok(release.includes('test "${GITHUB_REF_NAME}" = "v${version}"'));
  assert.ok(release.includes('git merge-base --is-ancestor "${GITHUB_SHA}" origin/main'));
  assert.ok(release.includes("prerelease: false"));
});

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
