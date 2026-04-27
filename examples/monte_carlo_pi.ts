// Monte Carlo Pi com UI (single-thread)
// Inspirado em https://github.com/TetieWasTaken/monte-carlo-pi
//
// - clicar "Run" gera POINTS_PER_RUN pontos aleatorios em [-1,1]^2
// - acumula total / inside; π ≈ 4 * inside / total
// - canvas mostra circulo + amostra dos primeiros pontos
// - clicar Run varias vezes refina a estimativa
//
// Single-thread porque thread.spawn dentro de UI callback crasha (issue #206).

import { ui, io, gc, buffer, math } from "rts";

// ── Config ───────────────────────────────────────────────────────────────
const CANVAS = 360;
const CANVAS_X = 20;
const CANVAS_Y = 50;
const SAMPLE_MAX = 8000;
const POINTS_PER_RUN = 10000;

// ── Estado ───────────────────────────────────────────────────────────────
let total = 0.0;
let inside = 0.0;
let sampled = 0;          // total amostrado (cresce sem parar)
let sampleHead = 0;       // cursor circular dentro do buffer
let sampleStride = 1;     // amostra 1 a cada N pontos
// 9 bytes por amostra: i32 px, i32 py, u8 inside
const sampleBuf = buffer.alloc_zeroed(SAMPLE_MAX * 9);

// ── App + Window ─────────────────────────────────────────────────────────
const app = ui.app_new();
const win = ui.window_new(720, 460, "Monte Carlo π — RTS");
ui.window_set_color(win, 25, 25, 30);

// ── Canvas ───────────────────────────────────────────────────────────────
const canvas = ui.frame_new(CANVAS_X, CANVAS_Y, CANVAS, CANVAS, "");
ui.widget_set_color(canvas, 15, 15, 20);
ui.widget_set_draw(canvas, () => {
    ui.set_draw_color(15, 15, 20);
    ui.draw_rect_fill(CANVAS_X, CANVAS_Y, CANVAS, CANVAS);
    ui.set_draw_color(80, 80, 100);
    ui.draw_rect(CANVAS_X, CANVAS_Y, CANVAS, CANVAS);
    ui.set_draw_color(120, 160, 200);
    ui.set_line_style(0, 2);
    ui.draw_circle(CANVAS_X + (CANVAS / 2), CANVAS_Y + (CANVAS / 2), (CANVAS / 2) as number);
    ui.set_line_style(0, 1);

    let count = sampled;
    if (count > SAMPLE_MAX) { count = SAMPLE_MAX; }
    let i = 0;
    while (i < count) {
        const off = i * 9;
        const px = buffer.read_i32(sampleBuf, off);
        const py = buffer.read_i32(sampleBuf, off + 4);
        const ins = buffer.read_u8(sampleBuf, off + 8);
        if (ins == 1) {
            ui.set_draw_color(120, 220, 140);
        } else {
            ui.set_draw_color(230, 110, 110);
        }
        ui.draw_rect_fill(CANVAS_X + px - 1, CANVAS_Y + py - 1, 2, 2);
        i = i + 1;
    }
});

// ── Painel ───────────────────────────────────────────────────────────────
const PANEL_X = CANVAS_X + CANVAS + 20;

const title = ui.frame_new(PANEL_X, CANVAS_Y, 280, 25, "Monte Carlo π");
ui.widget_set_label_color(title, 255, 220, 120);

const totalOut = ui.output_new(PANEL_X + 80, CANVAS_Y + 40, 200, 25, "Total:");
const insideOut = ui.output_new(PANEL_X + 80, CANVAS_Y + 75, 200, 25, "Inside:");
const piOut = ui.output_new(PANEL_X + 80, CANVAS_Y + 110, 200, 25, "π ≈");
const errOut = ui.output_new(PANEL_X + 80, CANVAS_Y + 145, 200, 25, "error:");
ui.widget_set_label_color(totalOut, 255, 255, 255);
ui.widget_set_label_color(insideOut, 255, 255, 255);
ui.widget_set_label_color(piOut, 255, 255, 255);
ui.widget_set_label_color(errOut, 255, 255, 255);

ui.output_set_value(totalOut, "0");
ui.output_set_value(insideOut, "0");
ui.output_set_value(piOut, "—");
ui.output_set_value(errOut, "—");

function updateStats(): void {
    const ht = gc.string_from_f64(total);
    const hi = gc.string_from_f64(inside);
    ui.output_set_value(totalOut, ht);
    ui.output_set_value(insideOut, hi);
    gc.string_free(ht);
    gc.string_free(hi);

    if (total > 0.0) {
        const pi = 4.0 * inside / total;
        const err = math.abs_f64(pi - math.PI);
        const hp = gc.string_from_f64(pi);
        const he = gc.string_from_f64(err);
        ui.output_set_value(piOut, hp);
        ui.output_set_value(errOut, he);
        gc.string_free(hp);
        gc.string_free(he);
    }
}

// ── Controles: pontos por run + seed ─────────────────────────────────────
const spinPts = ui.spinner_new(PANEL_X + 100, CANVAS_Y + 180, 180, 25, "points/run:");
ui.spinner_set_bounds(spinPts, 1000.0, 999999.0);
ui.spinner_set_value(spinPts, POINTS_PER_RUN as number);
ui.widget_set_label_color(spinPts, 255, 255, 255);

function runBatch(n: number): void {
    let i = 0;
    while (i < n) {
        const rx = math.random_f64();
        const ry = math.random_f64();
        const x = rx * 2.0 - 1.0;
        const y = ry * 2.0 - 1.0;
        const isIn = (x * x + y * y) <= 1.0;
        total = total + 1.0;
        if (isIn) { inside = inside + 1.0; }

        // amostragem em ring buffer com stride: 1 a cada `sampleStride`
        if ((i % sampleStride) == 0) {
            const off = sampleHead * 9;
            const px = (rx * CANVAS) as number;
            const py = (ry * CANVAS) as number;
            buffer.write_i32(sampleBuf, off, px);
            buffer.write_i32(sampleBuf, off + 4, py);
            buffer.write_u8(sampleBuf, off + 8, isIn ? 1 : 0);
            sampleHead = sampleHead + 1;
            if (sampleHead >= SAMPLE_MAX) { sampleHead = 0; }
            if (sampled < SAMPLE_MAX) { sampled = sampled + 1; }
        }
        i = i + 1;
    }
}

// ── Botoes ───────────────────────────────────────────────────────────────
const btnRun = ui.button_new(PANEL_X, CANVAS_Y + 220, 90, 30, "Run");
ui.widget_set_color(btnRun, 40, 120, 60);
ui.widget_set_label_color(btnRun, 255, 255, 255);
function setStride(totalPoints: number): void {
    // queremos ~SAMPLE_MAX amostras espalhadas pelo batch inteiro
    const s = (totalPoints / SAMPLE_MAX) as number;
    sampleStride = s | 0;
    if (sampleStride < 1) { sampleStride = 1; }
}

ui.widget_set_callback(btnRun, () => {
    const n = (ui.spinner_value(spinPts) as number) | 0;
    setStride(n);
    runBatch(n);
    updateStats();
    ui.widget_redraw(canvas);
});

const btnRun1k = ui.button_new(PANEL_X + 95, CANVAS_Y + 220, 90, 30, "Run × 1k");
ui.widget_set_color(btnRun1k, 40, 100, 140);
ui.widget_set_label_color(btnRun1k, 255, 255, 255);
ui.widget_set_callback(btnRun1k, () => {
    const n = (ui.spinner_value(spinPts) as number) | 0;
    setStride(n * 1000);
    let k = 0;
    while (k < 1000) { runBatch(n); k = k + 1; }
    updateStats();
    ui.widget_redraw(canvas);
});

const btnRun100k = ui.button_new(PANEL_X + 190, CANVAS_Y + 220, 90, 30, "Run × 100k");
ui.widget_set_color(btnRun100k, 80, 60, 160);
ui.widget_set_label_color(btnRun100k, 255, 255, 255);
ui.widget_set_callback(btnRun100k, () => {
    const n = (ui.spinner_value(spinPts) as number) | 0;
    setStride(n * 100000);
    let k = 0;
    while (k < 100000) { runBatch(n); k = k + 1; }
    updateStats();
    ui.widget_redraw(canvas);
});

const btnReset = ui.button_new(PANEL_X + 95, CANVAS_Y + 255, 185, 30, "Reset");
ui.widget_set_color(btnReset, 120, 50, 50);
ui.widget_set_label_color(btnReset, 255, 255, 255);

ui.widget_set_callback(btnReset, () => {
    total = 0.0;
    inside = 0.0;
    sampled = 0;
    sampleHead = 0;
    updateStats();
    ui.widget_redraw(canvas);
});

const info2 = ui.frame_new(PANEL_X, CANVAS_Y + 295, 280, 25, "verde = dentro do círculo");
const info3 = ui.frame_new(PANEL_X, CANVAS_Y + 320, 280, 25, "vermelho = fora");
ui.widget_set_label_color(info2, 120, 220, 140);
ui.widget_set_label_color(info3, 230, 110, 110);

ui.window_end(win);
ui.window_show(win);
io.print("Monte Carlo π UI rodando — feche a janela pra sair");
ui.app_run(app);
