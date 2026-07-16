// Cross-runtime: Date.UTC argument arity + the 0-99 year mapping rule.
// UTC-only, fixed values: fully deterministic.

// Missing args default: month=0, day=1, h/m/s/ms=0.
console.log("y_only=" + Date.UTC(2024));
console.log("y_m=" + Date.UTC(2024, 0));
console.log("y_m_d=" + Date.UTC(2024, 0, 1));
console.log("full=" + Date.UTC(2024, 0, 1, 0, 0, 0, 0));
console.log("all_eq=" + (Date.UTC(2024) === Date.UTC(2024, 0, 1, 0, 0, 0, 0)));

// Progressive arity on a non-trivial instant.
console.log("a2=" + Date.UTC(2024, 5));
console.log("a3=" + Date.UTC(2024, 5, 10));
console.log("a4=" + Date.UTC(2024, 5, 10, 7));
console.log("a5=" + Date.UTC(2024, 5, 10, 7, 8));
console.log("a6=" + Date.UTC(2024, 5, 10, 7, 8, 9));
console.log("a7=" + Date.UTC(2024, 5, 10, 7, 8, 9, 10));

// Zero args => NaN (year is undefined => ToNumber => NaN).
console.log("zero_args_nan=" + Number.isNaN(Date.UTC()));

// Extra args beyond 7 are ignored.
console.log("extra_ignored=" + ((Date.UTC as any)(2024, 0, 1, 0, 0, 0, 0, 99) === Date.UTC(2024, 0, 1)));

// Year 0..99 maps to 1900+year.
console.log("y0=" + new Date(Date.UTC(0, 0, 1)).toISOString());
console.log("y70=" + new Date(Date.UTC(70, 0, 1)).toISOString());
console.log("y99=" + new Date(Date.UTC(99, 11, 31)).toISOString());
console.log("y99_is_1999=" + (new Date(Date.UTC(99, 0, 1)).getUTCFullYear() === 1999));

// 100 is NOT remapped.
console.log("y100=" + new Date(Date.UTC(100, 0, 1)).toISOString());
console.log("y100_year=" + new Date(Date.UTC(100, 0, 1)).getUTCFullYear());

// Fractional year in the 0-99 window truncates before the mapping.
console.log("y95_5=" + new Date(Date.UTC(95.9, 0, 1)).getUTCFullYear());

// Negative year is not remapped.
console.log("neg_year=" + new Date(Date.UTC(-1, 0, 1)).getUTCFullYear());

// NaN in any field poisons the result.
console.log("nan_month=" + Number.isNaN(Date.UTC(2024, NaN)));
console.log("nan_day=" + Number.isNaN(Date.UTC(2024, 0, NaN)));
console.log("nan_ms=" + Number.isNaN(Date.UTC(2024, 0, 1, 0, 0, 0, NaN)));

// Non-integer components truncate toward zero.
console.log("frac_trunc=" + (Date.UTC(2024, 0, 1, 5.9) === Date.UTC(2024, 0, 1, 5)));
