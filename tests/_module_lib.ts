// Helper module — usado por tests/module_imports.test.ts. Nao roda
// sozinho (prefixo `_` evita coleta pela suite).

export function add(a: i64, b: i64): i64 {
    return a + b;
}

export function greet(name: string): string {
    return "hello, " + name;
}

export const MAGIC: i64 = 42;
