import { describe, expect, it } from "vitest";
import { parseMarkdown } from "./markdown";

describe("parseMarkdown", () => {
  it("preserves a blank span for marker-only blocks", () => {
    for (const source of ["# ", "- ", "**"]) {
      const blocks = parseMarkdown(source);
      expect(blocks).toHaveLength(1);
      expect(blocks[0].spans).toHaveLength(1);
      expect(blocks[0].spans[0].text).toBe("");
    }
  });

  it("only creates HTTP(S) links", () => {
    const [block] = parseMarkdown(
      "[safe](https://example.com) [unsafe](javascript:alert(1))",
    );
    expect(block.spans.some((span) => span.link === "https://example.com")).toBe(true);
    expect(block.spans.some((span) => span.link?.startsWith("javascript:"))).toBe(false);
  });

  it("keeps punctuation outside automatically detected links", () => {
    const [block] = parseMarkdown("See https://example.com/test.");
    const linked = block.spans.find((span) => span.link);
    expect(linked?.link).toBe("https://example.com/test");
    expect(block.spans.at(-1)?.text).toBe(".");
  });
});
