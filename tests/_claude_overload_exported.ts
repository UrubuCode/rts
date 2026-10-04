// Exported overloads, for `claude-overload-signature-elision.test.ts`.
//
// A separate module because the failure this half pins is a MODULE one: the
// signatures reached `check::module::duplicate_export` as further runtime
// declarations of `pick`, and the file was refused before anything ran with
// ``Syntax("`pick` is exported twice")``. Written inside the test file the
// refusal would take the whole suite file with it rather than one test.
export function pick<T extends string>(v: T): string;
export function pick(v: number): number;
export function pick(v: string | number): string | number {
  return typeof v === "string" ? v + "!" : v * 2;
}

// `export declare function` is spelled the same way the signatures are — a
// declaration with no body — and it is erased for the same reason. It names
// something that exists elsewhere, so there is nothing here to export.
export declare function elsewhere(v: string): string;
