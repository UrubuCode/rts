// An exported class with a decorator, beside one without, for
// `claude-decorator-export.test.ts`. The undecorated neighbour is the control:
// it exported fine while the decorated one published nothing.

export const trace: string[] = [];

export function Mark(name: string) {
  return (target: any) => {
    trace.push("marked:" + name + "|" + typeof target);
  };
}

@Mark("target")
export class Target {
  x = 1;
  hello() {
    return "target";
  }
}

export class Plain {
  y = 2;
}

@Mark("replaced")
export class Replaced {
  z = 3;
}
