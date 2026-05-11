import { ui, io } from "rts";

// ── Estado global ────────────────────────────────────────────────────────
let display: string = "0";
let accumulator: number = 0.0;
let pendingOp: string = "";
let resetOnNext: boolean = false;

// ── Display widget (handle global, atribuido apos criacao) ───────────────
let screen: number = 0;

function refresh(): void {
    ui.output_set_value(screen, display);
}

function formatNum(n: number): string {
    const truncated = Math.trunc(n);
    if (n === truncated && Math.abs(n) < 1e15) {
        return truncated.toString();
    }
    return n.toString();
}

function applyOp(op: string, a: number, b: number): number {
    if (op === "+") return a + b;
    if (op === "-") return a - b;
    if (op === "*") return a * b;
    if (op === "/") {
        if (b === 0.0) return 0.0;
        return a / b;
    }
    return b;
}

// ── Acoes (uma fn nomeada por botao — callbacks apontam direto pra elas)
function pushDigit(d: string): void {
    if (resetOnNext || display === "0") {
        display = d;
        resetOnNext = false;
    } else {
        display = display + d;
    }
    refresh();
}

function on0(): void { pushDigit("0"); }
function on1(): void { pushDigit("1"); }
function on2(): void { pushDigit("2"); }
function on3(): void { pushDigit("3"); }
function on4(): void { pushDigit("4"); }
function on5(): void { pushDigit("5"); }
function on6(): void { pushDigit("6"); }
function on7(): void { pushDigit("7"); }
function on8(): void { pushDigit("8"); }
function on9(): void { pushDigit("9"); }

function onDot(): void {
    if (resetOnNext) {
        display = "0.";
        resetOnNext = false;
        refresh();
        return;
    }
    if (!display.includes(".")) {
        display = display + ".";
        refresh();
    }
}

function setOp(op: string): void {
    const current = parseFloat(display);
    if (pendingOp !== "" && !resetOnNext) {
        accumulator = applyOp(pendingOp, accumulator, current);
        display = formatNum(accumulator);
        refresh();
    } else {
        accumulator = current;
    }
    pendingOp = op;
    resetOnNext = true;
}

function onAdd(): void { setOp("+"); }
function onSub(): void { setOp("-"); }
function onMul(): void { setOp("*"); }
function onDiv(): void { setOp("/"); }

function onEquals(): void {
    if (pendingOp === "") return;
    const current = parseFloat(display);
    const result = applyOp(pendingOp, accumulator, current);
    accumulator = result;
    display = formatNum(result);
    pendingOp = "";
    resetOnNext = true;
    refresh();
}

function onClear(): void {
    display = "0";
    accumulator = 0.0;
    pendingOp = "";
    resetOnNext = false;
    refresh();
}

function onNegate(): void {
    if (display === "0") return;
    if (display.startsWith("-")) {
        display = display.substring(1);
    } else {
        display = "-" + display;
    }
    refresh();
}

function onPercent(): void {
    const v = parseFloat(display) / 100.0;
    display = formatNum(v);
    refresh();
}

// ── App + Window ─────────────────────────────────────────────────────────
const app = ui.app_new();
const win = ui.window_new(280, 380, "RTS Calculator");
ui.window_set_color(win, 25, 25, 35);

screen = ui.output_new(15, 15, 250, 50, "");
ui.widget_set_color(screen, 240, 240, 230);
ui.widget_set_label_color(screen, 20, 20, 20);
ui.output_set_value(screen, "0");

// ── Layout ───────────────────────────────────────────────────────────────
const X0 = 15;
const Y0 = 80;
const BW = 60;
const BH = 50;
const GAP = 5;

// Linha 0: AC, +/-, %, /
const bAC = ui.button_new(X0, Y0, BW, BH, "AC");
ui.widget_set_color(bAC, 180, 90, 90);
ui.widget_set_label_color(bAC, 255, 255, 255);
ui.widget_set_callback(bAC, onClear);

const bNeg = ui.button_new(X0 + (BW + GAP), Y0, BW, BH, "+/-");
ui.widget_set_color(bNeg, 90, 90, 110);
ui.widget_set_label_color(bNeg, 255, 255, 255);
ui.widget_set_callback(bNeg, onNegate);

const bPct = ui.button_new(X0 + 2 * (BW + GAP), Y0, BW, BH, "%");
ui.widget_set_color(bPct, 90, 90, 110);
ui.widget_set_label_color(bPct, 255, 255, 255);
ui.widget_set_callback(bPct, onPercent);

const bDiv = ui.button_new(X0 + 3 * (BW + GAP), Y0, BW, BH, "/");
ui.widget_set_color(bDiv, 230, 150, 40);
ui.widget_set_label_color(bDiv, 255, 255, 255);
ui.widget_set_callback(bDiv, onDiv);

// Linha 1: 7 8 9 *
const Y1 = Y0 + (BH + GAP);
const b7 = ui.button_new(X0, Y1, BW, BH, "7");
ui.widget_set_color(b7, 60, 60, 70);
ui.widget_set_label_color(b7, 255, 255, 255);
ui.widget_set_callback(b7, on7);

const b8 = ui.button_new(X0 + (BW + GAP), Y1, BW, BH, "8");
ui.widget_set_color(b8, 60, 60, 70);
ui.widget_set_label_color(b8, 255, 255, 255);
ui.widget_set_callback(b8, on8);

const b9 = ui.button_new(X0 + 2 * (BW + GAP), Y1, BW, BH, "9");
ui.widget_set_color(b9, 60, 60, 70);
ui.widget_set_label_color(b9, 255, 255, 255);
ui.widget_set_callback(b9, on9);

const bMul = ui.button_new(X0 + 3 * (BW + GAP), Y1, BW, BH, "*");
ui.widget_set_color(bMul, 230, 150, 40);
ui.widget_set_label_color(bMul, 255, 255, 255);
ui.widget_set_callback(bMul, onMul);

// Linha 2: 4 5 6 -
const Y2 = Y0 + 2 * (BH + GAP);
const b4 = ui.button_new(X0, Y2, BW, BH, "4");
ui.widget_set_color(b4, 60, 60, 70);
ui.widget_set_label_color(b4, 255, 255, 255);
ui.widget_set_callback(b4, on4);

const b5 = ui.button_new(X0 + (BW + GAP), Y2, BW, BH, "5");
ui.widget_set_color(b5, 60, 60, 70);
ui.widget_set_label_color(b5, 255, 255, 255);
ui.widget_set_callback(b5, on5);

const b6 = ui.button_new(X0 + 2 * (BW + GAP), Y2, BW, BH, "6");
ui.widget_set_color(b6, 60, 60, 70);
ui.widget_set_label_color(b6, 255, 255, 255);
ui.widget_set_callback(b6, on6);

const bSub = ui.button_new(X0 + 3 * (BW + GAP), Y2, BW, BH, "-");
ui.widget_set_color(bSub, 230, 150, 40);
ui.widget_set_label_color(bSub, 255, 255, 255);
ui.widget_set_callback(bSub, onSub);

// Linha 3: 1 2 3 +
const Y3 = Y0 + 3 * (BH + GAP);
const b1 = ui.button_new(X0, Y3, BW, BH, "1");
ui.widget_set_color(b1, 60, 60, 70);
ui.widget_set_label_color(b1, 255, 255, 255);
ui.widget_set_callback(b1, on1);

const b2 = ui.button_new(X0 + (BW + GAP), Y3, BW, BH, "2");
ui.widget_set_color(b2, 60, 60, 70);
ui.widget_set_label_color(b2, 255, 255, 255);
ui.widget_set_callback(b2, on2);

const b3 = ui.button_new(X0 + 2 * (BW + GAP), Y3, BW, BH, "3");
ui.widget_set_color(b3, 60, 60, 70);
ui.widget_set_label_color(b3, 255, 255, 255);
ui.widget_set_callback(b3, on3);

const bAdd = ui.button_new(X0 + 3 * (BW + GAP), Y3, BW, BH, "+");
ui.widget_set_color(bAdd, 230, 150, 40);
ui.widget_set_label_color(bAdd, 255, 255, 255);
ui.widget_set_callback(bAdd, onAdd);

// Linha 4: 0 (largo) . =
const Y4 = Y0 + 4 * (BH + GAP);
const b0 = ui.button_new(X0, Y4, BW * 2 + GAP, BH, "0");
ui.widget_set_color(b0, 60, 60, 70);
ui.widget_set_label_color(b0, 255, 255, 255);
ui.widget_set_callback(b0, on0);

const bDot = ui.button_new(X0 + 2 * (BW + GAP), Y4, BW, BH, ".");
ui.widget_set_color(bDot, 60, 60, 70);
ui.widget_set_label_color(bDot, 255, 255, 255);
ui.widget_set_callback(bDot, onDot);

const bEq = ui.button_new(X0 + 3 * (BW + GAP), Y4, BW, BH, "=");
ui.widget_set_color(bEq, 230, 150, 40);
ui.widget_set_label_color(bEq, 255, 255, 255);
ui.widget_set_callback(bEq, onEquals);

// ── Show & run ───────────────────────────────────────────────────────────
ui.window_end(win);
ui.window_show(win);
io.print("Calculadora rodando — feche a janela para sair");
ui.app_run(app);
io.print("encerrada");
