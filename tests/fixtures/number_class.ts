// Fixture: Number class — static methods, constants, instance methods
import { io } from "rts";

// ── Static methods ────────────────────────────────────────────────────────────
io.print(Number.isNaN(NaN) ? "true" : "false");
io.print(Number.isNaN(42) ? "true" : "false");
io.print(Number.isFinite(42.0) ? "true" : "false");
io.print(Number.isFinite(Infinity) ? "true" : "false");
io.print(Number.isInteger(3.0) ? "true" : "false");
io.print(Number.isInteger(3.5) ? "true" : "false");
io.print(Number.isSafeInteger(Number.MAX_SAFE_INTEGER) ? "true" : "false");
io.print(Number.isSafeInteger(Number.MAX_SAFE_INTEGER + 1) ? "true" : "false");

// ── Constants ─────────────────────────────────────────────────────────────────
io.print(Number.MAX_SAFE_INTEGER === 9007199254740991 ? "true" : "false");
io.print(Number.MIN_SAFE_INTEGER === -9007199254740991 ? "true" : "false");
io.print(Number.isNaN(Number.NaN) ? "true" : "false");
io.print(Number.isFinite(Number.POSITIVE_INFINITY) ? "true" : "false");
io.print(Number.isFinite(Number.NEGATIVE_INFINITY) ? "true" : "false");

// ── Number() coercion ─────────────────────────────────────────────────────────
io.print(Number("42") === 42 ? "true" : "false");
io.print(Number("3.14") === 3.14 ? "true" : "false");
io.print(Number("") === 0 ? "true" : "false");
io.print(isNaN(Number("abc")) ? "true" : "false");
io.print(Number() === 0 ? "true" : "false");

// ── Instance methods ──────────────────────────────────────────────────────────
io.print((3.14159).toFixed(2));
io.print((1000).toString());
io.print((255).toString(16));
io.print((8).toString(2));
io.print((1.5e10).toExponential(2));
io.print((123.456).toPrecision(5));
io.print((42.0).valueOf() === 42 ? "true" : "false");

// ── Global isNaN / isFinite ───────────────────────────────────────────────────
io.print(isNaN(NaN) ? "true" : "false");
io.print(isNaN(0) ? "true" : "false");
io.print(isFinite(1.0) ? "true" : "false");
io.print(isFinite(Infinity) ? "true" : "false");
