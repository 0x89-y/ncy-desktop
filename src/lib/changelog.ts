// Ncy desktop release notes, shown in Settings → About. Newest first. Keep
// entries short and user-facing; bump the top version to match package.json on
// release. The first entry will be 0.4.1 — do not add anything before that.

export interface ChangelogEntry {
  version: string;
  /** ISO date (YYYY-MM-DD). */
  date: string;
  changes: string[];
}

export const CHANGELOG: ChangelogEntry[] = [];
