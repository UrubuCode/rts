// The exporting half of `main.ts`. See its header for what is being measured.

export const STEP = 3;
export const LIMIT = 1000;

export function mix(a: number, b: number): number {
    return (a + b) | 0;
}
