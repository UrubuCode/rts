declare global {
  var global: typeof globalThis;

  interface GlobalThis {
    String: new (value?: string) => { toString(): string };
    console: any;
  }
}

export {};
