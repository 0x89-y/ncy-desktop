import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { CHANGELOG } from "./changelog";

/**
 * The release notes exist twice: CHANGELOG.md for the repository, and this
 * module for Settings → About. Nothing linked them, so updating the markdown
 * left the app showing an older release — which is exactly what happened, and
 * went unnoticed until someone opened About.
 *
 * The two are deliberately worded differently (the markdown groups by
 * Added/Changed/Fixed, the app shows one flat list), so only the release
 * identity is compared. Drift in the entries themselves is a judgement call;
 * drift in which release you are looking at is a bug.
 */
const root = fileURLToPath(new URL("../../", import.meta.url));

function topOfMarkdown(): { version: string; date: string } {
  const markdown = readFileSync(`${root}CHANGELOG.md`, "utf8");
  // "## 0.7.3 — 2026-07-27", tolerating any dash between the two.
  const match = markdown.match(/^##\s*(\d+\.\d+\.\d+)\s*[—–-]\s*(\d{4}-\d{2}-\d{2})\s*$/m);
  if (!match) throw new Error("no version heading found in CHANGELOG.md");
  return { version: match[1], date: match[2] };
}

describe("in-app changelog", () => {
  it("shows the same release as CHANGELOG.md", () => {
    const markdown = topOfMarkdown();
    expect({ version: CHANGELOG[0].version, date: CHANGELOG[0].date }).toEqual(markdown);
  });

  it("shows the version this build reports", () => {
    const pkg = JSON.parse(readFileSync(`${root}package.json`, "utf8"));
    expect(CHANGELOG[0].version).toBe(pkg.version);
  });

  it("stays newest-first with no repeated release", () => {
    const versions = CHANGELOG.map((entry) => entry.version);
    expect(new Set(versions).size).toBe(versions.length);
    const dates = CHANGELOG.map((entry) => entry.date);
    expect([...dates].sort().reverse()).toEqual(dates);
  });
});
