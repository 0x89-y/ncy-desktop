// Release helpers shared by the GitHub release workflow and publish-release.bat.
//
//   node scripts/release.mjs version [tag]
//       Prints the app version. Fails if package.json, src-tauri/Cargo.toml and
//       src-tauri/tauri.conf.json disagree, or if a tag is given and it is not
//       v<version>.
//
//   node scripts/release.mjs manifest <sig file> <out file> [notes]
//       Writes the updater's latest.json for the current version, with the
//       signature inlined and the installer URL pointing at the GitHub release.
//
//   node scripts/release.mjs notes <out file>
//       Writes the CHANGELOG.md section for the current version (without its
//       heading), used as the GitHub release body.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");

const REPO = "0x89-y/ncy-desktop";
// Stable asset name, so https://github.com/<repo>/releases/latest/download/<name>
// always points at the newest installer.
const INSTALLER_NAME = "Ncy-windows-x64-setup.exe";

function fail(msg) {
  console.error(`release.mjs: ${msg}`);
  process.exit(1);
}

function appVersion() {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json")).version;
  const pkg = JSON.parse(read("package.json")).version;
  const cargo = read("src-tauri/Cargo.toml").match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!conf) fail("no version in src-tauri/tauri.conf.json");
  if (pkg !== conf || cargo !== conf) {
    fail(
      `versions disagree: tauri.conf.json ${conf}, package.json ${pkg}, ` +
        `src-tauri/Cargo.toml ${cargo}. Bump all three together.`,
    );
  }
  return conf;
}

function changelogSection(version) {
  const lines = read("CHANGELOG.md").split(/\r?\n/);
  const start = lines.findIndex((l) => l.startsWith(`## ${version} `) || l === `## ${version}`);
  if (start < 0) fail(`CHANGELOG.md has no "## ${version}" section`);
  let end = lines.findIndex((l, i) => i > start && l.startsWith("## "));
  if (end < 0) end = lines.length;
  return lines.slice(start + 1, end).join("\n").trim() + "\n";
}

const [cmd, ...args] = process.argv.slice(2);

if (cmd === "version") {
  const version = appVersion();
  const tag = args[0]?.replace(/^refs\/tags\//, "");
  if (tag && tag !== `v${version}`) fail(`tag ${tag} does not match app version ${version}`);
  console.log(version);
} else if (cmd === "manifest") {
  const [sigFile, outFile, notes] = args;
  if (!sigFile || !outFile) fail("usage: manifest <sig file> <out file> [notes]");
  const version = appVersion();
  const manifest = {
    version,
    notes: notes || `Ncy ${version}. See Settings > About for the full changelog.`,
    pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms: {
      "windows-x86_64": {
        // The .sig file is one base64 line; trim so no newline sneaks in.
        signature: readFileSync(sigFile, "utf8").trim(),
        url: `https://github.com/${REPO}/releases/download/v${version}/${INSTALLER_NAME}`,
      },
    },
  };
  // Plain UTF-8, no BOM: the updater parses this with serde_json, which rejects
  // a leading BOM ("error decoding response body").
  writeFileSync(outFile, JSON.stringify(manifest, null, 2) + "\n", "utf8");
  console.log(`wrote ${outFile} for ${version}`);
} else if (cmd === "notes") {
  const [outFile] = args;
  if (!outFile) fail("usage: notes <out file>");
  writeFileSync(outFile, changelogSection(appVersion()), "utf8");
  console.log(`wrote ${outFile}`);
} else {
  fail("usage: version [tag] | manifest <sig> <out> [notes] | notes <out>");
}
