import { expect, it } from "vitest";
import { roundTripMarkdown } from "@/features/editor/markdown";
import { GETTING_STARTED_BODY } from "./gettingStarted";

it("the getting-started page is already in the editor's canonical form", () => {
  expect(roundTripMarkdown(GETTING_STARTED_BODY)).toBe(GETTING_STARTED_BODY);
});
