export function formatTime(ms: number | null | undefined): string {
  if (!ms) return "";
  const d = new Date(ms);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  if (sameDay) {
    return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }
  const dayDiff =
    (now.getTime() - d.getTime()) / (1000 * 60 * 60 * 24);
  if (dayDiff < 7) {
    return d.toLocaleDateString(undefined, { weekday: "short" });
  }
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function formatFullTime(ms: number | null | undefined): string {
  if (!ms) return "";
  const d = new Date(ms);
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatMessageTime(ms: number | null | undefined): string {
  if (!ms) return "";
  const d = new Date(ms);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();

  if (sameDay) {
    return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }

  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfMessageDay = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const dayDiff = Math.max(1, Math.floor((startOfToday - startOfMessageDay) / 86400000));

  if (dayDiff === 1) return "Yesterday";
  if (dayDiff < 7) return `${dayDiff} days ago`;

  const weekDiff = Math.floor(dayDiff / 7);
  if (dayDiff < 60) return weekDiff === 1 ? "1 week ago" : `${weekDiff} weeks ago`;

  const monthDiff =
    (now.getFullYear() - d.getFullYear()) * 12 +
    now.getMonth() -
    d.getMonth() -
    (now.getDate() < d.getDate() ? 1 : 0);
  if (monthDiff < 12) return monthDiff <= 1 ? "1 month ago" : `${monthDiff} months ago`;

  const yearDiff = now.getFullYear() - d.getFullYear() - (
    now.getMonth() < d.getMonth() ||
    (now.getMonth() === d.getMonth() && now.getDate() < d.getDate())
      ? 1
      : 0
  );
  return yearDiff <= 1 ? "1 year ago" : `${yearDiff} years ago`;
}

/**
 * Preview text for a live message, mirroring `Message::reply_preview` in ncy-core.
 * Used by the composer's reply bar, which quotes a message that hasn't been snapshotted
 * yet. Returns empty for images and untranscribed voice — pair it with
 * {@link replyPreviewText} to get a label for those.
 */
export function messagePreview(m: {
  type: "TEXT" | "VOICE" | "IMAGE" | "CHECKLIST";
  content: { spans: { text: string }[] }[];
  checklistItems: { text: string; checked: boolean }[];
  transcript: string | null;
}): string {
  let text: string;
  if (m.type === "CHECKLIST") {
    const done = m.checklistItems.filter((i) => i.checked).length;
    const first = m.checklistItems[0]?.text ?? "";
    text = `${done}/${m.checklistItems.length} ${first}`;
  } else {
    // Spans concatenate within a block; blocks join with a space.
    const fromContent = m.content.map((b) => b.spans.map((s) => s.text).join("")).join(" ");
    text = fromContent.trim() ? fromContent : (m.transcript ?? "");
  }
  return text.slice(0, 100);
}

/**
 * One line of text standing in for a quoted message. The snapshot stored on a reply
 * is deliberately empty for images and untranscribed voice notes — those carry no
 * text — so each client supplies its own label here.
 */
export function replyPreviewText(
  preview: string | null,
  type: "TEXT" | "VOICE" | "IMAGE" | "CHECKLIST" | null,
): string {
  if (preview && preview.trim()) return preview;
  switch (type) {
    case "VOICE":
      return "Voice message";
    case "IMAGE":
      return "Photo";
    case "CHECKLIST":
      return "Checklist";
    default:
      return "Message";
  }
}

export function avatarInitials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "·";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[1][0]).toUpperCase();
}

const PALETTE = [
  "#6750a4",
  "#7d5260",
  "#1e88e5",
  "#43a047",
  "#fb8c00",
  "#8e24aa",
  "#00897b",
  "#5e35b1",
];

export function avatarColor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) {
    h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  }
  return PALETTE[h % PALETTE.length];
}
