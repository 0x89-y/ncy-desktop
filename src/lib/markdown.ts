// Lightweight subset-of-Markdown helpers for the desktop compose box. Goal isn't
// CommonMark fidelity — it's "give the user inline formatting that survives a
// round-trip to the phone." The parser maps:
//
//   `# heading`       → HEADING1     `## heading`  → HEADING2     `### heading` → HEADING3
//   `- item`          → BULLET_LIST  (one block per item)
//   ```...```         → CODE_BLOCK
//   `**bold**`        → span.bold
//   `*italic*`        → span.italic
//   plain text        → PARAGRAPH    (one block per non-empty line)
//
// Anything we don't understand (e.g. inline code, links, underline) is left as
// literal text. The phone renders the same model so messages display identically
// on both sides.

import type { RichTextBlock, RichTextSpan } from "./ipc";

function blankSpan(text: string): RichTextSpan {
  return {
    text,
    bold: false,
    italic: false,
    underline: false,
    strikethrough: false,
    color: null,
    link: null,
    inlineImage: null,
  };
}

function parseSpans(text: string): RichTextSpan[] {
  // Single pass: maintain bold/italic/strikethrough toggles, emit on every marker
  // boundary. Markers: `**` bold, `*` italic, `~~` strikethrough.
  const spans: RichTextSpan[] = [];
  let buffer = "";
  let bold = false;
  let italic = false;
  let strikethrough = false;

  const flush = () => {
    if (buffer.length === 0) return;
    spans.push({ ...blankSpan(buffer), bold, italic, strikethrough });
    buffer = "";
  };

  let i = 0;
  while (i < text.length) {
    // `**` for bold takes priority over `*` for italic.
    if (text[i] === "*" && text[i + 1] === "*") {
      flush();
      bold = !bold;
      i += 2;
      continue;
    }
    if (text[i] === "~" && text[i + 1] === "~") {
      flush();
      strikethrough = !strikethrough;
      i += 2;
      continue;
    }
    if (text[i] === "*") {
      // Reject `*` when it's adjacent to whitespace on the "outside" — that's almost
      // always literal (e.g. `2 * 3 = 6` or a list bullet someone forgot to space).
      const prev = i === 0 ? " " : text[i - 1];
      const next = text[i + 1] ?? " ";
      const looksLikeOpening = italic === false && next !== " " && next !== undefined;
      const looksLikeClosing = italic === true && prev !== " ";
      if (looksLikeOpening || looksLikeClosing) {
        flush();
        italic = !italic;
        i += 1;
        continue;
      }
    }
    buffer += text[i];
    i += 1;
  }
  flush();
  return spans.length > 0 ? spans : [blankSpan("")];
}

export function parseMarkdown(input: string): RichTextBlock[] {
  const lines = input.replace(/\r\n?/g, "\n").split("\n");
  const blocks: RichTextBlock[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trimStart();

    // Fenced code block — collect until the closing fence.
    if (trimmed.startsWith("```")) {
      const fenceIndent = line.length - trimmed.length;
      const buf: string[] = [];
      i += 1;
      while (i < lines.length) {
        const inner = lines[i];
        if (inner.trimStart().startsWith("```")) {
          i += 1;
          break;
        }
        // Preserve indentation relative to the fence for code blocks.
        buf.push(fenceIndent > 0 ? inner.slice(Math.min(fenceIndent, inner.length)) : inner);
        i += 1;
      }
      blocks.push({ type: "CODE_BLOCK", spans: [blankSpan(buf.join("\n"))] });
      continue;
    }

    if (trimmed.startsWith("### ")) {
      blocks.push({ type: "HEADING3", spans: parseSpans(trimmed.slice(4)) });
      i += 1;
      continue;
    }
    if (trimmed.startsWith("## ")) {
      blocks.push({ type: "HEADING2", spans: parseSpans(trimmed.slice(3)) });
      i += 1;
      continue;
    }
    if (trimmed.startsWith("# ")) {
      blocks.push({ type: "HEADING1", spans: parseSpans(trimmed.slice(2)) });
      i += 1;
      continue;
    }
    if (trimmed.startsWith("- ")) {
      blocks.push({ type: "BULLET_LIST", spans: parseSpans(trimmed.slice(2)) });
      i += 1;
      continue;
    }

    if (line.trim() === "") {
      i += 1;
      continue;
    }
    blocks.push({ type: "PARAGRAPH", spans: parseSpans(line) });
    i += 1;
  }

  return blocks;
}

/**
 * Apply a `prefix...suffix` wrap (or unwrap) around the current selection in a textarea.
 * If `suffix` is omitted it defaults to `prefix` (symmetric markers like `**`).
 *
 * Returns the new full value and the selection range to restore, leaving the caller
 * to bind both back into Svelte state and refocus the textarea.
 */
export function wrapSelection(
  textarea: HTMLTextAreaElement,
  prefix: string,
  suffix: string = prefix,
): { value: string; selectionStart: number; selectionEnd: number } {
  const start = textarea.selectionStart ?? textarea.value.length;
  const end = textarea.selectionEnd ?? start;
  const before = textarea.value.slice(0, start);
  const middle = textarea.value.slice(start, end);
  const after = textarea.value.slice(end);
  const value = `${before}${prefix}${middle}${suffix}${after}`;
  // If there was no selection, drop the caret between the markers.
  if (start === end) {
    const caret = start + prefix.length;
    return { value, selectionStart: caret, selectionEnd: caret };
  }
  return {
    value,
    selectionStart: start + prefix.length,
    selectionEnd: end + prefix.length,
  };
}

/**
 * Apply a line-prefix (e.g. `# `, `- `, `## `) to every line touched by the current
 * selection. Useful for headings and lists where the marker lives at the start of a line.
 */
export function prefixSelectedLines(
  textarea: HTMLTextAreaElement,
  prefix: string,
): { value: string; selectionStart: number; selectionEnd: number } {
  const start = textarea.selectionStart ?? textarea.value.length;
  const end = textarea.selectionEnd ?? start;
  const text = textarea.value;

  // Expand selection to full lines.
  const lineStart = text.lastIndexOf("\n", start - 1) + 1;
  const nextNewline = text.indexOf("\n", end);
  const lineEnd = nextNewline === -1 ? text.length : nextNewline;

  const before = text.slice(0, lineStart);
  const after = text.slice(lineEnd);
  const block = text.slice(lineStart, lineEnd);
  const prefixed = block
    .split("\n")
    .map((l) => (l.startsWith(prefix) ? l : `${prefix}${l}`))
    .join("\n");

  const delta = prefixed.length - block.length;
  return {
    value: `${before}${prefixed}${after}`,
    selectionStart: start + prefix.length,
    selectionEnd: end + delta,
  };
}
