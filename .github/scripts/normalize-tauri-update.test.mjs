import assert from "node:assert/strict";
import test from "node:test";
import { normalizeTauriUpdate } from "./normalize-tauri-update.mjs";

const repository = "Galaxy-A/cursor-byok";
const version = "0.1.7";
const asset = {
  id: 17,
  name: "Cursor BYOK_0.1.7_x64-setup.exe",
  browser_download_url: `https://github.com/${repository}/releases/download/v${version}/Cursor.BYOK_0.1.7_x64-setup.exe`,
};
const release = { tag_name: `v${version}`, assets: [asset] };
const platforms = ["linux-x86_64", "windows-x86_64", "darwin-aarch64", "darwin-x86_64"];
const manifest = (url, signature = "signed-payload") => ({
  version,
  platforms: Object.fromEntries(platforms.map((platform) => [platform, { url, signature }])),
});

test("normalizes draft API and download URLs to the fork's published asset", () => {
  for (const url of [
    `https://api.github.com/repos/${repository}/releases/assets/${asset.id}`,
    asset.browser_download_url,
  ]) {
    const result = normalizeTauriUpdate(manifest(url), release, repository, version);
    assert.equal(
      result.platforms["windows-x86_64"].url,
      `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(asset.name)}`,
    );
    assert.equal(result.platforms["windows-x86_64"].signature, "signed-payload");
  }
});

test("rejects upstream assets, unknown assets, and unsigned entries", () => {
  assert.throws(() => normalizeTauriUpdate(
    manifest("https://github.com/leookun/cursor-byok/releases/download/v0.1.7/app.exe"),
    release, repository, version,
  ), /unknown release asset/);
  assert.throws(() => normalizeTauriUpdate(
    manifest(`https://api.github.com/repos/${repository}/releases/assets/999`),
    release, repository, version,
  ), /unknown release asset/);
  assert.throws(() => normalizeTauriUpdate(
    manifest(asset.browser_download_url, ""),
    release, repository, version,
  ), /missing its URL or signature/);
});

test("rejects a mismatched manifest version or release tag", () => {
  assert.throws(() => normalizeTauriUpdate(
    { ...manifest(asset.browser_download_url), version: "0.1.4" },
    release, repository, version,
  ), /expected 0.1.7/);
  assert.throws(() => normalizeTauriUpdate(
    manifest(asset.browser_download_url),
    { ...release, tag_name: "v0.1.4" }, repository, version,
  ), /expected v0.1.7/);
});

test("requires every release platform before publishing an updater manifest", () => {
  assert.throws(() => normalizeTauriUpdate(
    { version, platforms: {} }, release, repository, version,
  ), /missing required platform/);
  for (const platform of platforms) {
    const incomplete = manifest(asset.browser_download_url);
    delete incomplete.platforms[platform];
    assert.throws(() => normalizeTauriUpdate(
      incomplete, release, repository, version,
    ), /missing required platform/);
  }
});
