import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Static initialization block
class Config {
  static readonly VERSION: string;
  static readonly DEBUG: boolean;
  static readonly MAX_RETRIES: number;

  static {
    Config.VERSION = "1.0.0";
    Config.DEBUG = false;
    Config.MAX_RETRIES = 3;
  }

  static describe(): void {
    print(`v${Config.VERSION} debug=${Config.DEBUG} retries=${Config.MAX_RETRIES}`);
  }
}

Config.describe();
print(`${Config.VERSION}`);

// Multiple static blocks
class Registry {
  static items: string[] = [];
  static count: number;

  static {
    Registry.items.push("default");
  }

  static {
    Registry.count = Registry.items.length;
  }

  static add(item: string): void {
    Registry.items.push(item);
    Registry.count++;
  }
}

Registry.add("extra");
print(`${Registry.count}`);
print(Registry.items.join(","));

describe("class_static_init", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "v1.0.0 debug=false retries=3\n1.0.0\n2\ndefault,extra\n"
  ));
});
