import { describe, expect, it } from "vitest";
import { normalizeAccent } from "./accent";

describe("normalizeAccent", () => {
  it("keeps current accents", () => {
    expect(normalizeAccent("moss")).toBe("moss");
    expect(normalizeAccent("terracotta")).toBe("terracotta");
  });
  it("maps the presets from before the redesign", () => {
    expect(normalizeAccent("purple")).toBe("violet");
    expect(normalizeAccent("blue")).toBe("inkblue");
    expect(normalizeAccent("green")).toBe("moss");
    expect(normalizeAccent("pink")).toBe("plum");
    expect(normalizeAccent("orange")).toBe("terracotta");
    expect(normalizeAccent("teal")).toBe("teal");
  });
  it("defaults to ink blue", () => {
    expect(normalizeAccent(undefined)).toBe("inkblue");
    expect(normalizeAccent("neon")).toBe("inkblue");
  });
});
