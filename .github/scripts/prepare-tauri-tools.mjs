import { createHash } from "node:crypto";
import { execFile as execFileCallback } from "node:child_process";
import {
  chmod,
  lstat,
  mkdir,
  mkdtemp,
  realpath,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { promisify } from "node:util";
import { dirname, join, relative, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";

const execFile = promisify(execFileCallback);
const REQUIRED_CLI_VERSION = "2.11.4";
const GITHUB_API = "https://api.github.com";
const GITHUB_API_VERSION = "2022-11-28";
const REQUEST_TIMEOUT_MS = 30_000;
const MAX_ATTEMPTS = 2;
const ASSET_REDIRECT_HOSTS = new Set([
  "api.github.com",
  "github.com",
  "objects.githubusercontent.com",
  "release-assets.githubusercontent.com",
]);

const NSIS_RELEASE = {
  repository: "tauri-apps/binary-releases",
  tag: "nsis-3.11",
  asset: "nsis-3.11.zip",
  sha1: "EF7FF767E5CBD9EDD22ADD3A32C9B8F4500BB10D",
};
const NSIS_PLUGIN_RELEASE = {
  repository: "tauri-apps/nsis-tauri-utils",
  tag: "nsis_tauri_utils-v0.5.3",
  asset: "nsis_tauri_utils.dll",
  sha1: "75197FEE3C6A814FE035788D1C34EAD39349B860",
};
const NSIS_REQUIRED_FILES = [
  "makensis.exe",
  "Bin/makensis.exe",
  "Stubs/lzma-x86-unicode",
  "Stubs/lzma_solid-x86-unicode",
  "Plugins/x86-unicode/additional/nsis_tauri_utils.dll",
  "Include/MUI2.nsh",
  "Include/FileFunc.nsh",
  "Include/x64.nsh",
  "Include/nsDialogs.nsh",
  "Include/WinMessages.nsh",
  "Include/Win/COM.nsh",
  "Include/Win/Propkey.nsh",
  "Include/Win/RestartManager.nsh",
];
const NSIS_REQUIRED_FILE_SHA256 = {
  "makensis.exe":
    "F497E92DEB9F179F7B8F7553FCB3BD04F511BB2949E5E4A2AEE80A10F7B20431",
  "Bin/makensis.exe":
    "42850802704ECB11163F7E0329D35EE54BD288953200D4966E226D572848CFC5",
  "Stubs/lzma-x86-unicode":
    "E916A205C5421F4287411D402FE1D2CB37E3DF93DB00FF4CFBACEC3B8EC8877E",
  "Stubs/lzma_solid-x86-unicode":
    "A0D065B62D34BE5F0AAAF7C162E101A5E25D7CD3EB10A13FDB37F91B02EBFCE2",
  "Include/MUI2.nsh":
    "2C847D2009B55FF46F8BC1591FFDA00F0194FB08FFEBC0876CDA669B7957B396",
  "Include/FileFunc.nsh":
    "476F33722FF773CFEC60876C5F270F2AADB9529E9B73730FA8C41E9490F4B0D6",
  "Include/x64.nsh":
    "AEA6AB8BAE28E987D4CEAC5385B29D781AAB5C3A1879F66A4639BE1EE7A5D269",
  "Include/nsDialogs.nsh":
    "C7D9E29CAE063EEF051866692699D225E1D33A1919F3D20C39FB4A937DE76422",
  "Include/WinMessages.nsh":
    "97C154539B1761DF7CE9B9DF922BE266697BBB28234D381DFA611529CCB50AEC",
  "Include/Win/COM.nsh":
    "ECE68EA2E0C50DB3A241E133CB278126BE9CFE779A6F70AFABEFD827B354A541",
  "Include/Win/Propkey.nsh":
    "B97960D2F114E39BF3600C08656C235EA762CEC3845C821C1D8D8B1A85B33468",
  "Include/Win/RestartManager.nsh":
    "3F1CA29B7EC6A5A604063792391D9B811D57484B002E6B87D50AAF584FF012CF",
};

const APP_RUN = {
  repository: "tauri-apps/binary-releases",
  tag: "apprun-old",
  asset: "AppRun-x86_64",
  sha256: "F30140A43A0A59E46DB21BDEFDF749B9E9F2C6946E92AFABBACF98B8AE73FB4F",
};
const LINUXDEPLOY = {
  repository: "tauri-apps/binary-releases",
  tag: "linuxdeploy",
  assetId: 182515537,
  asset: "linuxdeploy-x86_64.AppImage",
  size: 13_264_064,
  sha256:
    "E762BEA85C8EB0D4B3508D46E5C1F037F717D0F9303AE3B4AAFC8B04991FA1EF",
  mutatedSha256:
    "20EEBDE3C18AE2E44279BD624FC72482503AECE216D5D77F10932235342F71C1",
};
const LINUX_PLUGINS = [
  {
    repository: "tauri-apps/linuxdeploy-plugin-gtk",
    path: "linuxdeploy-plugin-gtk.sh",
    sha: "7735e3a962b66fc8010311da41daeb7c2e0bd369",
  },
  {
    repository: "tauri-apps/linuxdeploy-plugin-gstreamer",
    path: "linuxdeploy-plugin-gstreamer.sh",
    sha: "0db47bac65b3ebff1d69ef6edcb2b60d5af968b7",
  },
];

const releaseCache = new Map();

function safePath(url) {
  try {
    return new URL(url).pathname;
  } catch {
    return "<invalid-url>";
  }
}

function tokenFromEnvironment() {
  const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
  if (!token) throw new Error("GH_TOKEN is required for Tauri tool prefetch");
  return token;
}

async function pathWithinRealRoot(root, child) {
  try {
    const rootPath = await realpath(root);
    let candidate = resolve(child);
    while (true) {
      try {
        const candidatePath = await realpath(candidate);
        return (
          candidatePath === rootPath ||
          candidatePath.startsWith(`${rootPath}${sep}`)
        );
      } catch {
        const parent = dirname(candidate);
        if (parent === candidate) return false;
        candidate = parent;
      }
    }
  } catch {
    return false;
  }
}

function retryableStatus(status) {
  return status === 408 || status === 429 || status >= 500;
}

async function request(url, { accept, token, parse }) {
  let lastError;
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt += 1) {
    let response;
    try {
      const headers = {
        Accept: accept,
        "X-GitHub-Api-Version": GITHUB_API_VERSION,
        "User-Agent": "cursor-byok-tauri-prefetch",
      };
      if (token) headers.Authorization = `Bearer ${token}`;
      response = await fetch(url, {
        headers,
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
        redirect: "error",
      });
    } catch (error) {
      lastError = error;
      if (
        error?.cause?.code === "UND_ERR_REDIRECT" ||
        error?.cause?.message === "unexpected redirect"
      ) {
        throw new Error(`GitHub request redirect rejected at ${safePath(url)}`);
      }
      if (attempt < MAX_ATTEMPTS) continue;
      throw new Error(`GitHub request failed at ${safePath(url)}: ${error?.name ?? "network error"}`);
    }
    if (!response.ok) {
      if (retryableStatus(response.status) && attempt < MAX_ATTEMPTS) continue;
      throw new Error(
        `GitHub request failed (${response.status}) at ${safePath(url)}`,
      );
    }
    return parse(response);
  }
  throw new Error(`GitHub request failed at ${safePath(url)}: ${lastError?.name ?? "network error"}`);
}

async function requestJson(url, token) {
  return request(url, {
    accept: "application/vnd.github+json",
    token,
    parse: async (response) => {
      try {
        return await response.json();
      } catch {
        throw new Error(`GitHub returned invalid JSON at ${safePath(url)}`);
      }
    },
  });
}

async function requestBytes(url, token) {
  let lastError;
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt += 1) {
    try {
      const headers = {
        Accept: "application/octet-stream",
        "X-GitHub-Api-Version": GITHUB_API_VERSION,
        "User-Agent": "cursor-byok-tauri-prefetch",
      };
      if (token) headers.Authorization = `Bearer ${token}`;
      const response = await fetch(url, {
        headers,
        redirect: "manual",
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      let bodyResponse = response;
      if (response.status >= 300 && response.status < 400) {
        const location = response.headers.get("location");
        if (!location) throw new Error("GitHub asset redirect has no location");
        const target = new URL(location, url);
        if (
          target.protocol !== "https:" ||
          (target.port !== "" && target.port !== "443") ||
          !ASSET_REDIRECT_HOSTS.has(target.hostname)
        ) {
          throw new Error(`GitHub asset redirect has an untrusted host: ${target.hostname}`);
        }
        bodyResponse = await fetch(target, {
          headers: {
            Accept: "application/octet-stream",
            "X-GitHub-Api-Version": GITHUB_API_VERSION,
            "User-Agent": "cursor-byok-tauri-prefetch",
          },
          redirect: "manual",
          signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
        });
        if (bodyResponse.status >= 300 && bodyResponse.status < 400) {
          throw new Error("GitHub asset redirect chain is not allowed");
        }
      }
      if (!bodyResponse.ok) {
        if (retryableStatus(bodyResponse.status) && attempt < MAX_ATTEMPTS) continue;
        throw new Error(
          `GitHub request failed (${bodyResponse.status}) at ${safePath(url)}`,
        );
      }
      return new Uint8Array(await bodyResponse.arrayBuffer());
    } catch (error) {
      lastError = error;
      if (
        error instanceof Error &&
        (error.message.includes("untrusted host") ||
          error.message.includes("no location") ||
          error.message.includes("redirect chain"))
      ) {
        throw error;
      }
      if (error instanceof Error) {
        const status = Number(error.message.match(/GitHub request failed \((\d{3})\)/)?.[1]);
        if (Number.isInteger(status) && !retryableStatus(status)) throw error;
      }
      if (attempt < MAX_ATTEMPTS) continue;
    }
  }
  throw new Error(
    `GitHub request failed at ${safePath(url)}: ${lastError?.name ?? "network error"}`,
  );
}

function hashBytes(bytes, algorithm) {
  return createHash(algorithm).update(bytes).digest("hex").toUpperCase();
}

async function hashFile(file, algorithm) {
  return hashBytes(await readFile(file), algorithm);
}

function gitBlobSha(bytes) {
  const header = Buffer.from(`blob ${bytes.length}\0`, "utf8");
  return createHash("sha1")
    .update(Buffer.concat([header, Buffer.from(bytes)]))
    .digest("hex");
}

function assertSafeChild(root, child) {
  const rootPath = resolve(root);
  const childPath = resolve(child);
  if (childPath !== rootPath && !childPath.startsWith(`${rootPath}${sep}`)) {
    throw new Error(`path escapes cache root: ${child}`);
  }
  return childPath;
}

async function isRegularFile(file) {
  try {
    return (await lstat(file)).isFile();
  } catch {
    return false;
  }
}

async function isDirectory(directory) {
  try {
    return (await lstat(directory)).isDirectory();
  } catch {
    return false;
  }
}

async function pathExists(path) {
  try {
    await lstat(path);
    return true;
  } catch {
    return false;
  }
}

async function ensureDirectory(directory) {
  await mkdir(directory, { recursive: true });
}

async function readCliVersion(workspace) {
  const lockPath = join(workspace, "apps", "desktop", "package-lock.json");
  let lock;
  try {
    lock = JSON.parse(await readFile(lockPath, "utf8"));
  } catch {
    throw new Error("unable to read apps/desktop/package-lock.json");
  }
  const version = lock.packages?.["node_modules/@tauri-apps/cli"]?.version;
  if (version !== REQUIRED_CLI_VERSION) {
    throw new Error(
      `@tauri-apps/cli ${REQUIRED_CLI_VERSION} is required; found ${version ?? "missing"}`,
    );
  }
  return version;
}

async function getRelease(repository, tag, token) {
  const key = `${repository}@${tag}`;
  if (!releaseCache.has(key)) {
    releaseCache.set(
      key,
      requestJson(
        `${GITHUB_API}/repos/${repository}/releases/tags/${encodeURIComponent(tag)}`,
        token,
      ),
    );
  }
  return releaseCache.get(key);
}

async function downloadReleaseAsset({
  repository,
  tag,
  assetName,
  assetId,
  size,
  token,
}) {
  const release = await getRelease(repository, tag, token);
  const asset = release.assets?.find(
    (candidate) =>
      candidate?.name === assetName &&
      (assetId === undefined || candidate.id === assetId),
  );
  if (!asset?.url || !asset.url.startsWith(`${GITHUB_API}/repos/`)) {
    throw new Error(`required GitHub release asset is missing: ${assetName}`);
  }
  if (size !== undefined && asset.size !== size) {
    throw new Error(`unexpected size for release asset: ${assetName}`);
  }
  const bytes = await requestBytes(asset.url, token);
  if (size !== undefined && bytes.byteLength !== size) {
    throw new Error(`downloaded asset size mismatch: ${assetName}`);
  }
  return bytes;
}

async function downloadGitBlob({ repository, path, sha, token }) {
  const metadata = await requestJson(
    `${GITHUB_API}/repos/${repository}/contents/${path}?ref=master`,
    token,
  );
  const expectedBlobUrl =
    `${GITHUB_API}/repos/${repository}/git/blobs/${sha}`;
  if (metadata.sha !== sha || metadata.git_url !== expectedBlobUrl) {
    throw new Error(`unexpected Git blob metadata for ${repository}/${path}`);
  }
  const blob = await requestJson(expectedBlobUrl, token);
  if (blob.sha !== sha || blob.encoding !== "base64" || typeof blob.content !== "string") {
    throw new Error(`unexpected Git blob content for ${repository}/${path}`);
  }
  const encoded = blob.content.replace(/\s/g, "");
  if (
    !encoded ||
    encoded.length % 4 !== 0 ||
    !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(
      encoded,
    )
  ) {
    throw new Error(`invalid Git blob content for ${repository}/${path}`);
  }
  const bytes = Buffer.from(encoded, "base64");
  if (Buffer.from(bytes).toString("base64") !== encoded) {
    throw new Error(`invalid Git blob content for ${repository}/${path}`);
  }
  if (gitBlobSha(bytes) !== sha) {
    throw new Error(`Git blob verification failed for ${repository}/${path}`);
  }
  return bytes;
}

async function assertTreeSafe(root) {
  const rootPath = resolve(root);
  const pending = [rootPath];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const child = assertSafeChild(rootPath, join(current, entry.name));
      const info = await lstat(child);
      if (info.isSymbolicLink()) {
        throw new Error(`symlink in extracted archive: ${relative(rootPath, child)}`);
      }
      if (info.isDirectory()) pending.push(child);
    }
  }
}

async function expandArchive(zipPath, destination) {
  const command =
    process.env.ComSpec && process.env.ComSpec.toLowerCase().endsWith("cmd.exe")
      ? "powershell.exe"
      : "pwsh";
  const script =
    "$ErrorActionPreference = 'Stop'; " +
    "Expand-Archive -LiteralPath $env:TAURI_PREFETCH_ZIP -DestinationPath $env:TAURI_PREFETCH_DEST -Force";
  await execFile(
    command,
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
    {
      env: {
        ...process.env,
        TAURI_PREFETCH_ZIP: zipPath,
        TAURI_PREFETCH_DEST: destination,
      },
      windowsHide: true,
    },
  );
}

async function validateWindowsCache(cache) {
  if (!(await isDirectory(cache))) return false;
  if (!(await pathWithinRealRoot(dirname(cache), cache))) return false;
  for (const relativePath of NSIS_REQUIRED_FILES) {
    const file = assertSafeChild(cache, join(cache, relativePath));
    if (!(await isRegularFile(file))) {
      return false;
    }
    if (!(await pathWithinRealRoot(cache, file))) return false;
    const expected = NSIS_REQUIRED_FILE_SHA256[relativePath];
    if (
      relativePath !== "Plugins/x86-unicode/additional/nsis_tauri_utils.dll" &&
      (!expected || (await hashFile(file, "sha256")) !== expected)
    ) {
      return false;
    }
  }
  return (
    (await hashFile(join(cache, "Plugins", "x86-unicode", "additional", "nsis_tauri_utils.dll"), "sha1")) ===
    NSIS_PLUGIN_RELEASE.sha1
  );
}

async function publishDirectory(source, destination) {
  const backup = `${destination}.backup-${process.pid}-${Date.now()}`;
  let movedOld = false;
  try {
    if (await pathExists(destination)) {
      await rename(destination, backup);
      movedOld = true;
    }
    await rename(source, destination);
    if (movedOld) await rm(backup, { recursive: true, force: true });
  } catch (error) {
    if (movedOld && !(await pathExists(destination)) && (await pathExists(backup))) {
      await rename(backup, destination);
    }
    throw error;
  }
}

async function prepareWindows(workspace) {
  const token = tokenFromEnvironment();
  const localAppData =
    process.env.LOCALAPPDATA ||
    join(process.env.HOME || process.env.USERPROFILE || workspace, "AppData", "Local");
  const toolsRoot = join(localAppData, "tauri");
  const cache = join(toolsRoot, "NSIS");
  await ensureDirectory(toolsRoot);
  if (await validateWindowsCache(cache)) return;

  const tempRoot = await mkdtemp(join(toolsRoot, ".prepare-tauri-"));
  try {
    const zipPath = join(tempRoot, NSIS_RELEASE.asset);
    const extracted = join(tempRoot, "extracted");
    await ensureDirectory(extracted);
    const archive = await downloadReleaseAsset({
      repository: NSIS_RELEASE.repository,
      tag: NSIS_RELEASE.tag,
      assetName: NSIS_RELEASE.asset,
      token,
    });
    if (hashBytes(archive, "sha1") !== NSIS_RELEASE.sha1) {
      throw new Error("NSIS archive verification failed");
    }
    await writeFile(zipPath, archive);
    await expandArchive(zipPath, extracted);
    await assertTreeSafe(extracted);
    const entries = await readdir(extracted, { withFileTypes: true });
    if (entries.length !== 1 || !entries[0].isDirectory() || entries[0].name !== "nsis-3.11") {
      throw new Error("NSIS archive has an unexpected top-level directory");
    }
    const stagedCache = join(extracted, "nsis-3.11");
    const pluginRelative = join("Plugins", "x86-unicode", "additional");
    const pluginDirectory = assertSafeChild(stagedCache, join(stagedCache, pluginRelative));
    await ensureDirectory(pluginDirectory);
    const plugin = await downloadReleaseAsset({
      repository: NSIS_PLUGIN_RELEASE.repository,
      tag: NSIS_PLUGIN_RELEASE.tag,
      assetName: NSIS_PLUGIN_RELEASE.asset,
      token,
    });
    if (hashBytes(plugin, "sha1") !== NSIS_PLUGIN_RELEASE.sha1) {
      throw new Error("NSIS Tauri utility plugin verification failed");
    }
    await writeFile(join(pluginDirectory, "nsis_tauri_utils.dll"), plugin);
    if (!(await validateWindowsCache(stagedCache))) {
      throw new Error("NSIS cache integrity verification failed");
    }
    await publishDirectory(stagedCache, cache);
  } finally {
    await rm(tempRoot, { recursive: true, force: true });
  }
}

function linuxCacheRoot(workspace) {
  const home = process.env.HOME || workspace;
  return process.env.XDG_CACHE_HOME || join(home, ".cache");
}

async function validateLinuxFile(file, definition) {
  if (!(await isRegularFile(file))) return false;
  let info = await lstat(file);
  if ((info.mode & 0o111) === 0) {
    try {
      await chmod(file, 0o755);
      info = await lstat(file);
    } catch {
      return false;
    }
  }
  if (definition.size !== undefined) {
    if (info.size !== definition.size) return false;
  }
  const bytes = await readFile(file);
  const hash = definition.gitBlob
    ? gitBlobSha(bytes)
    : hashBytes(bytes, definition.algorithm);
  return definition.acceptedHashes.map((value) => value.toUpperCase()).includes(hash.toUpperCase());
}

async function publishFileAtomic(source, destination, mode = 0o755) {
  await chmod(source, mode);
  const temporaryDestination = `${destination}.new-${process.pid}-${Date.now()}`;
  await rm(temporaryDestination, { force: true });
  await rename(source, temporaryDestination);
  await rename(temporaryDestination, destination);
}

async function prepareLinux(workspace) {
  if (process.arch !== "x64") {
    throw new Error("Linux Tauri prefetch supports x64 runners only");
  }
  const token = tokenFromEnvironment();
  const cache = join(linuxCacheRoot(workspace), "tauri");
  await ensureDirectory(cache);
  if (!(await pathWithinRealRoot(dirname(cache), cache))) {
    throw new Error("Linux Tauri cache escapes its parent directory");
  }
  const definitions = [
    {
      name: "AppRun-x86_64",
      definition: {
        algorithm: "sha256",
        acceptedHashes: [APP_RUN.sha256],
      },
      download: () =>
        downloadReleaseAsset({
          repository: APP_RUN.repository,
          tag: APP_RUN.tag,
          assetName: APP_RUN.asset,
          token,
        }),
    },
    {
      name: LINUXDEPLOY.asset,
      definition: {
        algorithm: "sha256",
        size: LINUXDEPLOY.size,
        acceptedHashes: [LINUXDEPLOY.sha256, LINUXDEPLOY.mutatedSha256],
      },
      download: () =>
        downloadReleaseAsset({
          repository: LINUXDEPLOY.repository,
          tag: LINUXDEPLOY.tag,
          assetName: LINUXDEPLOY.asset,
          assetId: LINUXDEPLOY.assetId,
          size: LINUXDEPLOY.size,
          token,
        }),
    },
    ...LINUX_PLUGINS.map((plugin) => ({
      name: plugin.path,
      definition: {
        algorithm: "sha1",
        acceptedHashes: [plugin.sha],
        gitBlob: true,
      },
      download: () => downloadGitBlob({ ...plugin, token }),
    })),
  ];

  const pending = [];
  for (const entry of definitions) {
    const destination = assertSafeChild(cache, join(cache, entry.name));
    if (!(await pathWithinRealRoot(cache, destination))) {
      throw new Error(`Linux cache path escapes cache root: ${entry.name}`);
    }
    if (await validateLinuxFile(destination, entry.definition)) continue;
    const bytes = await entry.download();
    const verified =
      entry.definition.gitBlob
        ? gitBlobSha(bytes) === entry.definition.acceptedHashes[0]
        : entry.definition.acceptedHashes.includes(hashBytes(bytes, entry.definition.algorithm)) &&
          (entry.definition.size === undefined || bytes.byteLength === entry.definition.size);
    if (!verified) throw new Error(`verification failed for ${entry.name}`);
    pending.push({ destination, bytes });
  }

  if (pending.length === 0) return;
  const tempRoot = await mkdtemp(join(cache, ".prepare-tauri-"));
  try {
    const staged = [];
    for (const [index, item] of pending.entries()) {
      const source = join(tempRoot, `${index}-${item.destination.split(sep).pop()}`);
      await writeFile(source, item.bytes, { mode: 0o755 });
      staged.push({ source, destination: item.destination });
    }
    for (const item of staged) {
      await publishFileAtomic(item.source, item.destination);
    }
  } finally {
    await rm(tempRoot, { recursive: true, force: true });
  }
}

export async function prepareTauriTools({
  platform = process.platform,
  workspace = process.env.GITHUB_WORKSPACE || process.cwd(),
} = {}) {
  await readCliVersion(workspace);
  if (platform === "darwin") return;
  if (platform === "win32") return prepareWindows(workspace);
  if (platform === "linux") return prepareLinux(workspace);
  throw new Error(`unsupported platform: ${platform}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  prepareTauriTools().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
