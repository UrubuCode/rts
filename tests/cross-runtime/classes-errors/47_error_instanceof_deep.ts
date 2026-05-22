// Cross-runtime compatibility: error subclassing and instanceof chains.
class CustomTypeError extends TypeError {
  code: number;

  constructor(message: string, code: number) {
    super(message);
    this.code = code;
  }
}

const err = new CustomTypeError("bad", 17);
console.log("name=" + err.name);
console.log("msg=" + err.message);
console.log("code=" + err.code);
console.log("is_custom=" + (err instanceof CustomTypeError));
console.log("is_type=" + (err instanceof TypeError));
console.log("is_error=" + (err instanceof Error));
