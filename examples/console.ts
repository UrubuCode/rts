import { process, print } from "rts";

class Console {
  public log(...messages: Array<string | number | boolean | object | null | undefined>): void {
    this.stdout.write("Log: ");
    if (messages.length > 0) {
      this.stdout.write(String(messages[0]));
    }
  }

  constructor(public stdout: typeof process.stdout, public stderr: typeof process.stderr) {}
}

export const console = new Console(process.stdout, process.stderr);

console.log("hello from console.ts");
print("hello from print() call");
