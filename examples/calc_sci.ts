import { ui, io } from "rts";

// ── Estado de UI ─────────────────────────────────────────────────────────
let expr: string = "";
let display: number = 0;
let history: number = 0;
let lastResult: string = "";

function refresh(): void {
    if (expr === "") {
        ui.output_set_value(display, "0");
    } else {
        ui.output_set_value(display, expr);
    }
}

function setHistory(s: string): void {
    ui.output_set_value(history, s);
}

// ── Tokenizer ────────────────────────────────────────────────────────────
// Tipos de token (kind):
//   0 = number    (val: f64)
//   1 = op        (op_id: 0=+, 1=-, 2=*, 3=/, 4=^, 5=unary-, 6=%)
//   2 = lparen
//   3 = rparen
//   4 = func      (fn_id: 0=sin, 1=cos, 2=tan, 3=ln, 4=log, 5=sqrt, 6=exp,
//                  7=asin, 8=acos, 9=atan, 10=abs)
//   5 = const     (val: f64)  — pi, e
//   6 = comma     (sem uso por enquanto)

let tokKinds: number[] = [];
let tokNums: number[] = [];
let tokIds: number[] = [];

function isDigit(c: string): boolean {
    const k = c.charCodeAt(0);
    return k >= 48 && k <= 57;
}

function isAlpha(c: string): boolean {
    const k = c.charCodeAt(0);
    if (k >= 97 && k <= 122) return true;
    if (k >= 65 && k <= 90) return true;
    return false;
}

function fnIdFromName(name: string): number {
    if (name === "sin") return 0;
    if (name === "cos") return 1;
    if (name === "tan") return 2;
    if (name === "ln") return 3;
    if (name === "log") return 4;
    if (name === "sqrt") return 5;
    if (name === "exp") return 6;
    if (name === "asin") return 7;
    if (name === "acos") return 8;
    if (name === "atan") return 9;
    if (name === "abs") return 10;
    return -1;
}

function pushTok(k: number, v: number, id: number): void {
    tokKinds.push(k);
    tokNums.push(v);
    tokIds.push(id);
}

function scanNumber(src: string, start: number): number {
    let j = start;
    let sawDot = false;
    const n = src.length;
    while (j < n) {
        const cc = src.charAt(j);
        if (isDigit(cc)) {
            j = j + 1;
        } else if (cc === "." && !sawDot) {
            sawDot = true;
            j = j + 1;
        } else {
            return j;
        }
    }
    return j;
}

function scanIdent(src: string, start: number): number {
    let j = start;
    const n = src.length;
    while (j < n && isAlpha(src.charAt(j))) j = j + 1;
    return j;
}

function opIdFromChar(c: string): number {
    if (c === "+") return 0;
    if (c === "-") return 1;
    if (c === "*") return 2;
    if (c === "/") return 3;
    if (c === "^") return 4;
    if (c === "%") return 6;
    return -1;
}

// retorna 0 ok, 1 erro
function tokenize(src: string): number {
    tokKinds = [];
    tokNums = [];
    tokIds = [];
    let i = 0;
    const n = src.length;
    while (i < n) {
        const c = src.charAt(i);
        if (c === " " || c === "\t") {
            i = i + 1;
        } else if (isDigit(c) || c === ".") {
            const j = scanNumber(src, i);
            const v = parseFloat(src.substring(i, j));
            pushTok(0, v, 0);
            i = j;
        } else if (isAlpha(c)) {
            const j = scanIdent(src, i);
            const name = src.substring(i, j);
            if (name === "pi") {
                pushTok(5, 3.141592653589793, 0);
            } else if (name === "e") {
                pushTok(5, 2.718281828459045, 0);
            } else {
                const fid = fnIdFromName(name);
                if (fid < 0) return 1;
                pushTok(4, 0.0, fid);
            }
            i = j;
        } else if (c === "(") {
            pushTok(2, 0.0, 0);
            i = i + 1;
        } else if (c === ")") {
            pushTok(3, 0.0, 0);
            i = i + 1;
        } else {
            const oid = opIdFromChar(c);
            if (oid < 0) return 1;
            pushTok(1, 0.0, oid);
            i = i + 1;
        }
    }
    return 0;
}

// ── Arvore AST (arrays paralelos) ────────────────────────────────────────
// kinds: 0=num, 1=binop, 2=unary, 3=func
let nKind: number[] = [];
let nNum: number[] = [];
let nOp: number[] = [];
let nLeft: number[] = [];
let nRight: number[] = [];

function newNode(kind: number, num: number, op: number, left: number, right: number): number {
    nKind.push(kind);
    nNum.push(num);
    nOp.push(op);
    nLeft.push(left);
    nRight.push(right);
    return nKind.length - 1;
}

function resetAst(): void {
    nKind = [];
    nNum = [];
    nOp = [];
    nLeft = [];
    nRight = [];
}

// ── Parser recursivo descendente ─────────────────────────────────────────
// Gramatica:
//   expr   := term (('+'|'-') term)*
//   term   := power (('*'|'/'|'%') power)*
//   power  := unary ('^' power)?            (right-assoc)
//   unary  := '-' unary | atom
//   atom   := number | const | '(' expr ')' | func '(' expr ')'

let pos: number = 0;
let parseError: number = 0;

function peekKind(): number {
    if (pos >= tokKinds.length) return -1;
    return tokKinds[pos];
}

function parseExpr(): number {
    let lhs = parseTerm();
    if (parseError !== 0) return -1;
    while (true) {
        const k = peekKind();
        if (k !== 1) break;
        const op = tokIds[pos];
        if (op !== 0 && op !== 1) break;
        pos = pos + 1;
        const rhs = parseTerm();
        if (parseError !== 0) return -1;
        lhs = newNode(1, 0.0, op, lhs, rhs);
    }
    return lhs;
}

function parseTerm(): number {
    let lhs = parsePower();
    if (parseError !== 0) return -1;
    while (true) {
        const k = peekKind();
        if (k !== 1) break;
        const op = tokIds[pos];
        if (op !== 2 && op !== 3 && op !== 6) break;
        pos = pos + 1;
        const rhs = parsePower();
        if (parseError !== 0) return -1;
        lhs = newNode(1, 0.0, op, lhs, rhs);
    }
    return lhs;
}

function parsePower(): number {
    const lhs = parseUnary();
    if (parseError !== 0) return -1;
    const k = peekKind();
    if (k === 1 && tokIds[pos] === 4) {
        pos = pos + 1;
        const rhs = parsePower();
        if (parseError !== 0) return -1;
        return newNode(1, 0.0, 4, lhs, rhs);
    }
    return lhs;
}

function parseUnary(): number {
    const k = peekKind();
    if (k === 1 && tokIds[pos] === 1) {
        pos = pos + 1;
        const child = parseUnary();
        if (parseError !== 0) return -1;
        return newNode(2, 0.0, 5, child, -1);
    }
    return parseAtom();
}

function parseAtom(): number {
    const k = peekKind();
    if (k === 0 || k === 5) {
        const v = tokNums[pos];
        pos = pos + 1;
        return newNode(0, v, 0, -1, -1);
    }
    if (k === 2) {
        pos = pos + 1;
        const inner = parseExpr();
        if (parseError !== 0) return -1;
        if (peekKind() !== 3) { parseError = 1; return -1; }
        pos = pos + 1;
        return inner;
    }
    if (k === 4) {
        const fid = tokIds[pos];
        pos = pos + 1;
        if (peekKind() !== 2) { parseError = 1; return -1; }
        pos = pos + 1;
        const arg = parseExpr();
        if (parseError !== 0) return -1;
        if (peekKind() !== 3) { parseError = 1; return -1; }
        pos = pos + 1;
        return newNode(3, 0.0, fid, arg, -1);
    }
    parseError = 1;
    return -1;
}

// ── Eval ─────────────────────────────────────────────────────────────────
let evalError: number = 0;

function evalNode(idx: number): number {
    if (idx < 0) { evalError = 1; return 0.0; }
    const k = nKind[idx];
    if (k === 0) return nNum[idx];
    if (k === 2) {
        const v = evalNode(nLeft[idx]);
        return -v;
    }
    if (k === 1) {
        const a = evalNode(nLeft[idx]);
        const b = evalNode(nRight[idx]);
        const op = nOp[idx];
        if (op === 0) return a + b;
        if (op === 1) return a - b;
        if (op === 2) return a * b;
        if (op === 3) {
            if (b === 0.0) { evalError = 1; return 0.0; }
            return a / b;
        }
        if (op === 4) return Math.pow(a, b);
        if (op === 6) {
            if (b === 0.0) { evalError = 1; return 0.0; }
            return a - Math.trunc(a / b) * b;
        }
        evalError = 1;
        return 0.0;
    }
    if (k === 3) {
        const a = evalNode(nLeft[idx]);
        const fid = nOp[idx];
        if (fid === 0) return Math.sin(a);
        if (fid === 1) return Math.cos(a);
        if (fid === 2) return Math.tan(a);
        if (fid === 3) {
            if (a <= 0.0) { evalError = 1; return 0.0; }
            return Math.log(a);
        }
        if (fid === 4) {
            if (a <= 0.0) { evalError = 1; return 0.0; }
            return Math.log(a) / Math.log(10.0);
        }
        if (fid === 5) {
            if (a < 0.0) { evalError = 1; return 0.0; }
            return Math.sqrt(a);
        }
        if (fid === 6) return Math.exp(a);
        if (fid === 7) return Math.asin(a);
        if (fid === 8) return Math.acos(a);
        if (fid === 9) return Math.atan(a);
        if (fid === 10) return Math.abs(a);
        evalError = 1;
        return 0.0;
    }
    evalError = 1;
    return 0.0;
}

function formatResult(n: number): string {
    if (n !== n) return "NaN";
    const truncated = Math.trunc(n);
    if (n === truncated && Math.abs(n) < 1e15) {
        return truncated.toString();
    }
    return n.toString();
}

function evaluate(): void {
    if (expr === "") return;
    const terr = tokenize(expr);
    if (terr !== 0) {
        setHistory(expr + " = ERR (token)");
        lastResult = "";
        return;
    }
    resetAst();
    pos = 0;
    parseError = 0;
    const root = parseExpr();
    if (parseError !== 0 || pos !== tokKinds.length) {
        setHistory(expr + " = ERR (parse)");
        lastResult = "";
        return;
    }
    evalError = 0;
    const v = evalNode(root);
    if (evalError !== 0) {
        setHistory(expr + " = ERR (math)");
        lastResult = "";
        return;
    }
    const r = formatResult(v);
    setHistory(expr + " =");
    expr = r;
    lastResult = r;
    refresh();
}

// ── Acoes de input ───────────────────────────────────────────────────────
function append(s: string): void {
    // se ultimo eval gerou resultado e proximo input eh digito/dot, comeca novo
    if (lastResult !== "" && expr === lastResult) {
        const c = s.charAt(0);
        if (isDigit(c) || c === ".") {
            expr = "";
        }
    }
    lastResult = "";
    expr = expr + s;
    refresh();
}

function on0(): void { append("0"); }
function on1(): void { append("1"); }
function on2(): void { append("2"); }
function on3(): void { append("3"); }
function on4(): void { append("4"); }
function on5(): void { append("5"); }
function on6(): void { append("6"); }
function on7(): void { append("7"); }
function on8(): void { append("8"); }
function on9(): void { append("9"); }
function onDot(): void { append("."); }
function onAdd(): void { append("+"); }
function onSub(): void { append("-"); }
function onMul(): void { append("*"); }
function onDiv(): void { append("/"); }
function onPow(): void { append("^"); }
function onMod(): void { append("%"); }
function onLPar(): void { append("("); }
function onRPar(): void { append(")"); }

function onSin(): void { append("sin("); }
function onCos(): void { append("cos("); }
function onTan(): void { append("tan("); }
function onAsin(): void { append("asin("); }
function onAcos(): void { append("acos("); }
function onAtan(): void { append("atan("); }
function onLn(): void { append("ln("); }
function onLog(): void { append("log("); }
function onSqrt(): void { append("sqrt("); }
function onExp(): void { append("exp("); }
function onAbs(): void { append("abs("); }
function onPi(): void { append("pi"); }
function onE(): void { append("e"); }

function onClear(): void {
    expr = "";
    lastResult = "";
    setHistory("");
    refresh();
}

function onBack(): void {
    if (expr.length === 0) return;
    expr = expr.substring(0, expr.length - 1);
    lastResult = "";
    refresh();
}

function onEquals(): void { evaluate(); }

// ── App + Window ─────────────────────────────────────────────────────────
const app = ui.app_new();
const win = ui.window_new(440, 460, "RTS Scientific");
ui.window_set_color(win, 25, 25, 35);

// Display
display = ui.output_new(15, 15, 410, 50, "");
ui.widget_set_color(display, 240, 240, 230);
ui.widget_set_label_color(display, 20, 20, 20);
ui.output_set_value(display, "0");

// Historia (small, acima)
history = ui.output_new(15, 70, 410, 22, "");
ui.widget_set_color(history, 35, 35, 50);
ui.widget_set_label_color(history, 180, 180, 200);
ui.output_set_value(history, "");

// Layout grid 7 cols x 6 linhas
const X0 = 15;
const Y0 = 105;
const BW = 56;
const BH = 50;
const GAP = 4;

function place(c: number, r: number): number[] {
    const x = X0 + c * (BW + GAP);
    const y = Y0 + r * (BH + GAP);
    return [x, y];
}

function mkBtn(c: number, r: number, label: string, cb: () => void, rR: number, gG: number, bB: number): number {
    const p = place(c, r);
    const b = ui.button_new(p[0], p[1], BW, BH, label);
    ui.widget_set_color(b, rR, gG, bB);
    ui.widget_set_label_color(b, 255, 255, 255);
    ui.widget_set_callback(b, cb);
    return b;
}

// Cores
const FNCR = 70;  const FNCG = 90;  const FNCB = 130;   // funcoes — azul-cinza
const NUMR = 60;  const NUMG = 60;  const NUMB = 70;    // numeros
const OPR  = 230; const OPG  = 150; const OPB  = 40;    // operadores
const SPR  = 90;  const SPG  = 90;  const SPB  = 110;   // especiais
const CLR1 = 180; const CLG1 = 90;  const CLB1 = 90;    // limpar/back

// Linha 0: funcoes inversas
mkBtn(0, 0, "asin", onAsin, FNCR, FNCG, FNCB);
mkBtn(1, 0, "acos", onAcos, FNCR, FNCG, FNCB);
mkBtn(2, 0, "atan", onAtan, FNCR, FNCG, FNCB);
mkBtn(3, 0, "ln",   onLn,   FNCR, FNCG, FNCB);
mkBtn(4, 0, "log",  onLog,  FNCR, FNCG, FNCB);
mkBtn(5, 0, "AC",   onClear, CLR1, CLG1, CLB1);
mkBtn(6, 0, "<-",   onBack,  CLR1, CLG1, CLB1);

// Linha 1: trig + sqrt + exp + parens
mkBtn(0, 1, "sin",  onSin,  FNCR, FNCG, FNCB);
mkBtn(1, 1, "cos",  onCos,  FNCR, FNCG, FNCB);
mkBtn(2, 1, "tan",  onTan,  FNCR, FNCG, FNCB);
mkBtn(3, 1, "sqrt", onSqrt, FNCR, FNCG, FNCB);
mkBtn(4, 1, "exp",  onExp,  FNCR, FNCG, FNCB);
mkBtn(5, 1, "(",    onLPar, SPR, SPG, SPB);
mkBtn(6, 1, ")",    onRPar, SPR, SPG, SPB);

// Linha 2: 7 8 9 / abs ^ %
mkBtn(0, 2, "7", on7, NUMR, NUMG, NUMB);
mkBtn(1, 2, "8", on8, NUMR, NUMG, NUMB);
mkBtn(2, 2, "9", on9, NUMR, NUMG, NUMB);
mkBtn(3, 2, "/", onDiv, OPR, OPG, OPB);
mkBtn(4, 2, "abs", onAbs, FNCR, FNCG, FNCB);
mkBtn(5, 2, "^", onPow, OPR, OPG, OPB);
mkBtn(6, 2, "%", onMod, OPR, OPG, OPB);

// Linha 3: 4 5 6 *  pi e (vazio)
mkBtn(0, 3, "4", on4, NUMR, NUMG, NUMB);
mkBtn(1, 3, "5", on5, NUMR, NUMG, NUMB);
mkBtn(2, 3, "6", on6, NUMR, NUMG, NUMB);
mkBtn(3, 3, "*", onMul, OPR, OPG, OPB);
mkBtn(4, 3, "pi", onPi, SPR, SPG, SPB);
mkBtn(5, 3, "e", onE, SPR, SPG, SPB);

// Linha 4: 1 2 3 -
mkBtn(0, 4, "1", on1, NUMR, NUMG, NUMB);
mkBtn(1, 4, "2", on2, NUMR, NUMG, NUMB);
mkBtn(2, 4, "3", on3, NUMR, NUMG, NUMB);
mkBtn(3, 4, "-", onSub, OPR, OPG, OPB);

// Linha 5: 0 . + =
mkBtn(0, 5, "0", on0, NUMR, NUMG, NUMB);
mkBtn(1, 5, ".", onDot, NUMR, NUMG, NUMB);
mkBtn(2, 5, "+", onAdd, OPR, OPG, OPB);
// = ocupa cols 3..6
const pEq = place(3, 5);
const bEq = ui.button_new(pEq[0], pEq[1], BW * 4 + GAP * 3, BH, "=");
ui.widget_set_color(bEq, 230, 150, 40);
ui.widget_set_label_color(bEq, 255, 255, 255);
ui.widget_set_callback(bEq, onEquals);

// ── Show & run ───────────────────────────────────────────────────────────
ui.window_end(win);
ui.window_show(win);
io.print("Calc cientifica rodando — feche a janela para sair");
ui.app_run(app);
io.print("encerrada");
