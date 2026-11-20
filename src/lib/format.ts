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
  type: "TEXT" | "VOICE" | "IMAGE" | "CHECKLIST" | "FILE";
  content: { spans: { text: string }[] }[];
  checklistItems: { text: string; checked: boolean }[];
  transcript: string | null;
  fileAttachments: { name: string }[];
}): string {
  let text: string;
  if (m.type === "FILE") {
    text = m.fileAttachments[0]?.name ?? "File";
  } else if (m.type === "CHECKLIST") {
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
  type: "TEXT" | "VOICE" | "IMAGE" | "CHECKLIST" | "FILE" | null,
): string {
  if (preview && preview.trim()) return preview;
  switch (type) {
    case "VOICE":
      return "Voice message";
    case "IMAGE":
      return "Photo";
    case "CHECKLIST":
      return "Checklist";
    case "FILE":
      return "File";
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

// Chat hues, the same as Android's NcyChatHues: earthy enough to sit as a tint
// on any accent's paper, distinct enough to tell apart.
const PALETTE = [
  "#b4471f", // terracotta
  "#c69214", // ochre
  "#2f855a", // green
  "#0f766e", // teal
  "#2b6cb0", // blue
  "#805ad5", // violet
  "#b83280", // plum
  "#685f54", // stone
];

export function avatarColor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) {
    h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  }
  return PALETTE[h % PALETTE.length];
}

/** Human-readable byte count. Shared by the file card and transfer progress. */
export function formatFileSize(bytes: number | null | undefined): string {
  if (bytes == null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
