import { describe, expect, it } from "vitest";
import { safeLink } from "./links";

describe("safeLink", () => {
  it("accepts http(s) links", () => {
    expect(safeLink("https://example.com")).toBe("https://example.com");
    expect(safeLink("http://example.com/a?b=c#d")).toBe("http://example.com/a?b=c#d");
    expect(safeLink("HTTPS://EXAMPLE.COM")).toBe("HTTPS://EXAMPLE.COM");
  });

  it("rejects every other scheme a peer could send", () => {
    for (const link of [
      "javascript:alert(1)",
      "JaVaScRiPt:alert(1)",
      "file:///C:/Windows/System32/calc.exe",
      "data:text/html,<script>alert(1)</script>",
      "vbscript:msgbox(1)",
      "//example.com",
      "/relative/path",
      "example.com",
    ]) {
      expect(safeLink(link)).toBeNull();
    }
  });

  it("rejects empty and absent links", () => {
    expect(safeLink("")).toBeNull();
    expect(safeLink(null)).toBeNull();
    expect(safeLink(undefined)).toBeNull();
  });
});
