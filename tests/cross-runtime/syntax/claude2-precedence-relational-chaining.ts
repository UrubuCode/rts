// Cross-runtime: operator precedence where it surprises — relational operators
// chain left to right through a boolean, `+` binds tighter than `in` and
// `instanceof`, equality binds tighter than the bitwise operators, and a unary
// operator applies before any of them.

// 1) Relational operators chain: the first comparison's BOOLEAN is compared to
//    the next operand, which is coerced to a number.
console.log("chain_1_2_3=" + (1 < 2 < 3));
console.log("chain_3_2_1=" + (3 > 2 > 1));
console.log("chain_5_4_3=" + (5 > 4 > 3));
console.log("chain_true_gt_zero=" + (true > 0));
console.log("chain_false_lt_one=" + (false < 1));

// 2) Equality is looser than relational, so a comparison result is what gets
//    compared.
console.log("eq_after_rel=" + (1 == 1 < 2));
console.log("strict_after_rel=" + (true === 1 < 2));
console.log("rel_then_eq_false=" + (1 === 1 < 2));

// 3) `+` binds tighter than `in`: the concatenation makes the KEY.
const obj: any = { ab: "yes", a: "no" };
console.log("plus_before_in=" + ("a" + "b" in obj));
console.log("parenthesised_in=" + ("a" + ("b" in obj)));

// 4) `+` binds tighter than `instanceof` too.
console.log("plus_before_instanceof=" + (1 + 2 instanceof Object));
console.log("string_instanceof=" + ("x" + "y" instanceof String));

// 5) A unary operator applies to its operand alone, before `in`.
const keyed: any = { true: "T", false: "F" };
console.log("not_before_in=" + (!0 in keyed));
console.log("not_of_in=" + !(0 in keyed));
console.log("typeof_before_eq=" + (typeof 1 === "number"));
console.log("typeof_of_comparison=" + typeof (1 === 1));

// 6) Equality binds tighter than the bitwise operators.
console.log("bitand_after_eq=" + (1 & 3) + "|" + (1 & (3 == 1 ? 1 : 0)));
console.log("bitor_after_eq=" + (0 | (1 == 1 ? 1 : 0)));
console.log("xor_precedence=" + (1 ^ 2 & 3));

// 7) Shifts bind looser than `+`.
console.log("shift_after_plus=" + (1 + 2 << 3));
console.log("shift_parens=" + (1 + (2 << 3)));
console.log("shift_right=" + (16 >> 1 + 1));

// 8) `&&` binds tighter than `||`.
console.log("and_before_or=" + (false && false || true));
console.log("or_parens=" + (false && (false || true)));
console.log("mixed_values=" + String(0 || "a" && "b"));

// 9) The conditional operator is looser than `||`, so the whole disjunction is
//    the test.
console.log("ternary_test=" + (false || true ? "taken" : "not"));
console.log("ternary_grouped=" + (false || (true ? "taken" : "not")));

// 10) Assignment is looser than everything except the comma, so a comparison on
//     the right is computed first.
let assigned: any = 0;
assigned = 1 < 2;
console.log("assign_after_compare=" + assigned);

// 11) `void` binds tighter than `===`.
console.log("void_precedence=" + (void 0 === undefined));
console.log("void_of_comparison=" + String(void (0 === undefined)));

// 12) Unary minus and exponentiation must be parenthesised, and the grouped
//     forms differ.
console.log("neg_pow=" + (-(2 ** 2)) + "|" + ((-2) ** 2));

// 13) `typeof` of an undeclared name is safe; of a declared-but-unread name in
//     the dead zone it is not.
console.log("typeof_undeclared=" + typeof (globalThis as any).neverDeclaredHere);
function tdzTypeof(): string {
  try {
    const probe = typeof pending;
    return "got:" + probe;
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
  let pending = 1;
}
console.log("typeof_tdz=" + tdzTypeof());

// 14) `in` and `instanceof` share a precedence level and associate left to
//     right, so the result of the first feeds the second.
class Box {}
const box = new Box();
console.log("instanceof_then_in=" + (box instanceof Box in { true: 1 }));

// 15) The comma operator is the loosest: it needs parentheses inside a call.
function firstArg(v: any): string { return "arg=" + String(v); }
console.log("comma_in_call=" + firstArg((1, 2, 3)));
let commaTarget = 0;
commaTarget = (1, 2);
console.log("comma_assign=" + commaTarget);

// 16) Optional chaining binds as a member access, so `??` around it applies to
//     the whole chain.
const maybe: any = { inner: { value: 0 } };
const missing: any = undefined;
console.log("optional_then_nullish=" + String(missing?.inner?.value ?? "fallback"));
console.log("optional_zero_kept=" + String(maybe?.inner?.value ?? "fallback"));

// 17) `!` applies to the member access, not to the whole comparison.
console.log("not_member=" + !maybe.inner);
console.log("not_then_compare=" + (!maybe.inner === false));

// 18) Postfix binds tighter than any arithmetic around it.
let counter = 5;
console.log("postfix_in_expression=" + (counter++ + 1) + "|counter=" + counter);
let counter2 = 5;
console.log("prefix_in_expression=" + (++counter2 + 1) + "|counter=" + counter2);

// 19) String concatenation and numeric addition mix left to right.
console.log("left_to_right=" + (1 + 2 + "3") + "|" + ("1" + 2 + 3));

// 20) Relational comparison of strings is lexicographic, and chaining one into
//     another coerces the boolean.
console.log("string_relational=" + ("apple" < "banana"));
console.log("string_chain=" + ("apple" < "banana" < "cherry"));

// 21) `instanceof` with a non-callable right side throws; the left side was
//     still evaluated.
const evaluated: string[] = [];
function leftSide(): any {
  evaluated.push("left");
  return {};
}
function badInstanceof(): string {
  try {
    return String(leftSide() instanceof ({} as any));
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("bad_instanceof=" + badInstanceof() + "|" + evaluated.join(","));
