// `Number`, `Boolean`, and the global coercions.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

check("convert", Number("12") === 12);
check("convert-empty", Number("") === 0);
check("convert-none", Number() === 0);
check("convert-bad", isNaN(Number("abc")));
check("convert-bool", Number(true) === 1);

// The pair this keeps apart: one converts first, the other asks what arrived.
check("is-nan-strict", Number.isNaN("abc") === false);
check("is-nan-real", Number.isNaN(0 / 0) === true);
check("global-is-nan", isNaN("abc") === true);
check("is-finite-strict", Number.isFinite("12") === false);
check("global-is-finite", isFinite("12") === true);

check("is-integer", Number.isInteger(3) && !Number.isInteger(3.5));
check("is-safe", Number.isSafeInteger(3) && !Number.isSafeInteger(1e300));

check("max-safe", Number.MAX_SAFE_INTEGER === 9007199254740991);
check("min-safe", Number.MIN_SAFE_INTEGER === -9007199254740991);
check("epsilon", Number.EPSILON > 0);
check("nan-constant", isNaN(Number.NaN));

check("parse-int", parseInt("42px") === 42);
check("parse-int-hex", parseInt("0x1f") === 31);
check("parse-int-radix", parseInt("ff", 16) === 255);
check("parse-int-sign", parseInt("-17") === -17);
check("parse-int-bad", isNaN(parseInt("px")));
check("parse-float", parseFloat("3.5px") === 3.5);
check("parse-float-exp", parseFloat("-2.5e1x") === -25);
check("parse-float-partial-exp", parseFloat("1e") === 1);
check("number-parse-int", Number.parseInt("42px") === 42);

// A method on a primitive receiver, which needs no wrapper object.
check("to-string-radix", (255).toString(16) === "ff");
check("to-string-binary", (10).toString(2) === "1010");
check("to-string-default", (10).toString() === "10");
check("value-of", (5).valueOf() === 5);
check("to-fixed", (1.5).toFixed(1) === "1.5");
// Half away from zero, where Rust formats to nearest even.
check("to-fixed-half", (2.5).toFixed(0) === "3");
check("to-fixed-zero", (1.4).toFixed(0) === "1");

check("boolean-true", Boolean(1) === true);
check("boolean-empty-string", Boolean("") === false);
check("boolean-to-string", true.toString() === "true");
check("boolean-value-of", false.valueOf() === false);

// A computed key reaches the same receiver a named one does. It did not:
// `(255).toString(16)` answered "ff" while `(255)["toString"](16)` answered
// `undefined`, because only the named path had the primitive fallback.
check("computed-on-a-number", (255)["toString"](16) === "ff");
check("computed-on-a-boolean", true["toString"]() === "true");
check("computed-through-a-variable", (function () {
    let k = "valueOf";
    return (5)[k]() === 5;
})());
check("computed-absent-is-still-absent", (5)["nope"] === undefined);

return failed;
