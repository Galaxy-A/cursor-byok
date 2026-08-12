import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDir, "..");
const frontendDir = join(projectRoot, "frontend");
const outputPath = join(projectRoot, "THIRD_PARTY_NOTICES.txt");

function compareText(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}

function run(command, args, cwd = projectRoot) {
  return execFileSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  }).trim();
}

function licenseFiles(directory) {
  if (!directory || !existsSync(directory)) return [];
  return readdirSync(directory)
    .filter((name) => /^(license|licence|copying|notice)/i.test(name))
    .map((name) => join(directory, name))
    .filter((path) => statSync(path).isFile())
    .sort();
}

function normalizeText(path) {
  return readFileSync(path, "utf8").replace(/\r\n/g, "\n").trim();
}

const moduleRows = run("go", [
  "list",
  "-m",
  "-f",
  "{{.Path}}\t{{.Version}}\t{{.Dir}}",
  "all",
])
  .split(/\r?\n/)
  .map((line) => {
    const [path, version, directory] = line.split("\t");
    return { path, version: version || "local", directory };
  })
  .filter((item) => item.directory)
  .sort((a, b) => b.path.length - a.path.length || compareText(a.path, b.path));

const goReport = run("go", [
  "run",
  "github.com/google/go-licenses@v1.6.0",
  "report",
  "./...",
  "--ignore",
  "cursor",
]);

const npmReportText = process.platform === "win32"
  ? run(process.env.ComSpec || "cmd.exe", [
      "/d",
      "/s",
      "/c",
      "npx --yes license-checker@25.0.1 --production --json",
    ], frontendDir)
  : run("npx", ["--yes", "license-checker@25.0.1", "--production", "--json"], frontendDir);
const npmReport = JSON.parse(npmReportText);

const components = [];
const noticeGroups = new Map();

function addNoticeText(component, fileName, text) {
  text = text.replace(/\r\n/g, "\n").trim();
  if (!text) return;
  const digest = createHash("sha256").update(text).digest("hex");
  const current = noticeGroups.get(digest) || { text, files: new Set(), components: new Set() };
  current.files.add(fileName);
  current.components.add(component);
  noticeGroups.set(digest, current);
}

function addNotice(component, path) {
  addNoticeText(component, basename(path), normalizeText(path));
}

async function fetchRepositoryLicense(component, metadata) {
  const repository = String(metadata.repository || "")
    .replace(/^git\+/, "")
    .replace(/\.git$/, "");
  const match = repository.match(/^https:\/\/github\.com\/([^/]+)\/([^/]+)$/i);
  if (!match) throw new Error(`Cannot resolve a repository license for ${component}`);

  const version = component.slice(component.lastIndexOf("@") + 1);
  const [, owner, repo] = match;
  const candidates = [`v${version}`, version, "main", "master"];
  for (const ref of candidates) {
    const url = `https://raw.githubusercontent.com/${owner}/${repo}/${ref}/LICENSE`;
    const response = await fetch(url);
    if (response.ok) return { fileName: "LICENSE", text: await response.text() };
  }
  throw new Error(`Cannot download a repository license for ${component}`);
}

for (const line of goReport.split(/\r?\n/).filter(Boolean).sort()) {
  const [packagePath, reportedLicenseURL, reportedLicense] = line.split(",");
  const module = moduleRows.find(
    (item) => packagePath === item.path || packagePath.startsWith(`${item.path}/`),
  );
  if (!module) throw new Error(`Cannot resolve Go module for ${packagePath}`);

  const component = `${module.path}@${module.version}`;
  const license = packagePath === "modernc.org/mathutil" ? "BSD-3-Clause" : reportedLicense;
  const licenseURL = packagePath === "modernc.org/mathutil"
    ? "https://pkg.go.dev/modernc.org/mathutil@v1.7.1?tab=licenses"
    : reportedLicenseURL;
  components.push({ ecosystem: "Go", component, license, source: licenseURL });
  const files = licenseFiles(module.directory);
  if (!files.length) throw new Error(`Cannot find a license file for ${component}`);
  for (const path of files) addNotice(component, path);
}

for (const [component, metadata] of Object.entries(npmReport).sort(([a], [b]) => compareText(a, b))) {
  if (component === "frontend@0.0.0") continue;
  const source = metadata.repository || metadata.url || "See the package registry metadata";
  components.push({
    ecosystem: "npm",
    component,
    license: String(metadata.licenses || "Unknown"),
    source,
  });

  if (metadata.licenseFile && existsSync(metadata.licenseFile)) {
    addNotice(component, metadata.licenseFile);
    for (const path of licenseFiles(dirname(metadata.licenseFile))) {
      addNotice(component, path);
    }
  } else {
    const fallback = await fetchRepositoryLicense(component, metadata);
    addNoticeText(component, fallback.fileName, fallback.text);
  }
}

const uniqueComponents = [...new Map(components.map((item) => [
  `${item.ecosystem}:${item.component}`,
  item,
])).values()].sort((a, b) =>
  compareText(`${a.ecosystem}:${a.component}`, `${b.ecosystem}:${b.component}`),
);

const lines = [
  "THIRD-PARTY SOFTWARE NOTICES AND LICENSES",
  "=========================================",
  "",
  "Cursor助手 includes third-party software. The project itself is distributed",
  "under the MIT License found in the accompanying LICENSE file. The notices and",
  "license texts below apply only to the identified third-party components.",
  "",
  "MPL-2.0 source availability",
  "---------------------------",
  "The executable includes github.com/cyphar/filepath-securejoin v0.6.1. Its",
  "corresponding source code is available from:",
  "https://github.com/cyphar/filepath-securejoin/tree/v0.6.1",
  "",
  "Component inventory",
  "-------------------",
  ...uniqueComponents.map(
    ({ ecosystem, component, license, source }) =>
      `[${ecosystem}] ${component} | ${license} | ${source}`,
  ),
  "",
  "License texts and notices",
  "-------------------------",
];

for (const group of [...noticeGroups.values()].sort((a, b) =>
  compareText([...a.components].sort()[0], [...b.components].sort()[0]),
)) {
  lines.push(
    "",
    "===============================================================================",
    `Components: ${[...group.components].sort().join(", ")}`,
    `Files: ${[...group.files].sort().join(", ")}`,
    "===============================================================================",
    "",
    group.text,
  );
}

writeFileSync(outputPath, `${lines.join("\n")}\n`, "utf8");
console.log(`Wrote ${outputPath}`);
console.log(`${uniqueComponents.length} components, ${noticeGroups.size} unique notices`);
