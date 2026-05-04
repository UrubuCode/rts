import { describe, test, expect } from "rts:test";

let out = "";

// (#373) url.searchParams getter — liga URL e URLSearchParams.
// Anotacao explicita preserva tipo.
const u = new URL("https://example.com/path?a=1&b=2&c=3");
const sp: URLSearchParams = u.searchParams;

out += sp.get("a") + "\n";
out += sp.get("b") + "\n";
out += (sp.has("a") ? "y" : "n") + "\n";

describe("url_search_params_getter", () => {
  test("url.searchParams (#373)", () => expect(out).toBe(
    "1\n2\ny\n"
  ));
});
