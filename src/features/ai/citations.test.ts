import { describe, expect, it } from "vitest";
import type { Source } from "@/lib/ipc/bindings/Source";
import { checkCitations, citedKeys, nextSourceNumber, splitCitations } from "./citations";

const source = (key: string): Source => ({
  key,
  kind: "passage",
  page_id: "p",
  title: "T",
  section: [],
  excerpt: "",
});

describe("citations", () => {
  it("finds the usual ways models write them", () => {
    expect(splitCitations("A [S1]. B [S2, s3][S4] and 【S5】、[S06]")).toEqual([
      "A ",
      { raw: "[S1]", keys: ["S1"] },
      ". B ",
      { raw: "[S2, s3]", keys: ["S2", "S3"] },
      { raw: "[S4]", keys: ["S4"] },
      " and ",
      { raw: "【S5】", keys: ["S5"] },
      "、",
      { raw: "[S06]", keys: ["S6"] },
    ]);
    expect(splitCitations("no [citations] here [S] or [1]")).toEqual([
      "no [citations] here [S] or [1]",
    ]);
    expect(citedKeys("[S2] [S1] [S2]")).toEqual(["S2", "S1"]);
  });

  it("verifies keys against the sources provided, so invented ones don't count", () => {
    const { cited, unverified } = checkCitations("x [S1] y [S9] z [S2]", [
      source("S1"),
      source("S2"),
      source("S3"),
    ]);
    expect(cited.map((s) => s.key)).toEqual(["S1", "S2"]);
    expect(unverified).toEqual(["S9"]);
    expect(checkCitations("[S1]", []).unverified).toEqual(["S1"]);
  });

  it("numbers new sources after the highest key", () => {
    expect(nextSourceNumber([])).toBe(1);
    expect(nextSourceNumber([source("S2"), source("S10"), source("S3")])).toBe(11);
  });
});
