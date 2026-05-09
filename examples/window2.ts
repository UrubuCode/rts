//
// RTS LIVE WEB ↔ DESKTOP SYNC
//
// sincronizacao REAL entre:
//
//  - pagina web
//  - UI desktop nativa
//
// sem travar
// thread-safe
// async
//

import {
    ui,
    io,
    http_server,
    promise
} from "rts";

// ╔════════════════════════════════════════════╗
// ║              SHARED STATE                  ║
// ╚════════════════════════════════════════════╝

let sharedProgress: f64 = 25.0;

let sharedMessage: string =
    "Waiting...";

let dirty: bool = true;

// ╔════════════════════════════════════════════╗
// ║                 UI APP                     ║
// ╚════════════════════════════════════════════╝

const app = ui.app_new();

// ╔════════════════════════════════════════════╗
// ║                 WINDOW                     ║
// ╚════════════════════════════════════════════╝

const win = ui.window_new(
    760,
    540,
    "RTS Live Sync Dashboard"
);

ui.window_set_color(
    win,
    15,
    18,
    28
);

// ╔════════════════════════════════════════════╗
// ║                 HEADER                     ║
// ╚════════════════════════════════════════════╝

const header = ui.frame_new(
    0,
    0,
    760,
    70,
    ""
);

ui.widget_set_color(
    header,
    25,
    30,
    45
);

const title = ui.frame_new(
    20,
    20,
    500,
    30,
    "🌐 RTS LIVE WEB ↔ DESKTOP"
);

ui.widget_set_label_color(
    title,
    255,
    255,
    255
);

// ╔════════════════════════════════════════════╗
// ║                 STATUS                     ║
// ╚════════════════════════════════════════════╝

const status = ui.frame_new(
    20,
    90,
    500,
    30,
    "Status: waiting..."
);

ui.widget_set_label_color(
    status,
    180,
    180,
    180
);

// ╔════════════════════════════════════════════╗
// ║             PROGRESS LABEL                 ║
// ╚════════════════════════════════════════════╝

const progLabel = ui.frame_new(
    20,
    140,
    200,
    20,
    "Realtime Progress"
);

ui.widget_set_label_color(
    progLabel,
    220,
    220,
    220
);

// ╔════════════════════════════════════════════╗
// ║              PROGRESS BAR                  ║
// ╚════════════════════════════════════════════╝

const prog = ui.progress_new(
    20,
    170,
    420,
    32,
    ""
);

ui.progress_set_value(
    prog,
    sharedProgress
);

ui.widget_set_color(
    prog,
    80,
    180,
    120
);

// ╔════════════════════════════════════════════╗
// ║                 SLIDER                     ║
// ╚════════════════════════════════════════════╝

const slider = ui.slider_new(
    20,
    230,
    420,
    20,
    ""
);

ui.slider_set_bounds(
    slider,
    0.0,
    100.0
);

ui.slider_set_value(
    slider,
    sharedProgress
);

// desktop slider → state
ui.widget_set_callback(
    slider,
    () => {

        sharedProgress =
            ui.slider_value(slider);

        sharedMessage =
            "Desktop slider changed to " +
            sharedProgress;

        dirty = true;
    }
);

// ╔════════════════════════════════════════════╗
// ║                  OUTPUT                    ║
// ╚════════════════════════════════════════════╝

const output = ui.output_new(
    20,
    290,
    700,
    30,
    "Last Event:"
);

// ╔════════════════════════════════════════════╗
// ║                 BUTTONS                    ║
// ╚════════════════════════════════════════════╝

const btnReset = ui.button_new(
    20,
    350,
    140,
    40,
    "Reset"
);

ui.widget_set_color(
    btnReset,
    220,
    60,
    60
);

ui.widget_set_callback(
    btnReset,
    () => {

        sharedProgress = 0.0;

        sharedMessage =
            "Desktop reset progress";

        dirty = true;
    }
);

// ╔════════════════════════════════════════════╗
// ║                 CANVAS                     ║
// ╚════════════════════════════════════════════╝

const canvas = ui.frame_new(
    500,
    120,
    200,
    200,
    ""
);

ui.widget_set_draw(
    canvas,
    () => {

        // background
        ui.set_draw_color(
            25,
            30,
            50
        );

        ui.draw_rect_fill(
            500,
            120,
            200,
            200
        );

        // circle
        ui.set_draw_color(
            sharedProgress * 2,
            180,
            255
        );

        ui.draw_circle(
            600,
            220,
            60
        );

        // line
        ui.set_draw_color(
            255,
            255,
            255
        );

        ui.draw_line(
            540,
            280,
            660,
            280
        );

        // text
        ui.set_draw_color(
            255,
            255,
            255
        );

        ui.set_font(
            0,
            18
        );

        ui.draw_text(
            "LIVE",
            570,
            220
        );
    }
);

// ╔════════════════════════════════════════════╗
// ║                 FOOTER                     ║
// ╚════════════════════════════════════════════╝

const footer = ui.frame_new(
    0,
    510,
    760,
    30,
    "RTS • Async Runtime • HTTP Server • Native UI"
);

ui.widget_set_color(
    footer,
    20,
    24,
    36
);

ui.widget_set_label_color(
    footer,
    140,
    140,
    140
);

// finish
ui.window_end(win);

// show
ui.window_show(win);

// ╔════════════════════════════════════════════╗
// ║             HTTP SERVER ASYNC              ║
// ╚════════════════════════════════════════════╝
//
// Server roda em outra thread (tokio worker via promise.create).
// Handler so' toca shared state — nao chama ui.* direto, FLTK
// e' single-threaded. ui.app_awake() acorda a main thread pra
// proximo tick de ui.app_wait().

async function runServer(): i64 {

    io.print("Servidor online:");
    io.print("http://127.0.0.1:8080");

    http_server.serve(
        "127.0.0.1:8080",
        handler
    );

    return 0;
}

promise.create(runServer, []);

// ╔════════════════════════════════════════════╗
// ║                 ROUTER                     ║
// ╚════════════════════════════════════════════╝

function handler(req: i64): void {

    const path =
        http_server.req_path(req);

    // HOME
    if (path == "/") {

        http_server.respond(
            req,
            200,
            "text/html",
            renderHome()
        );

        return;
    }

    // PROGRESS
    if (
        startsWith(
            path,
            "/api/progress/"
        )
    ) {

        const value =
            parseFloat(
                path.substring(14)
            );

        sharedProgress =
            value;

        sharedMessage =
            "WEB changed progress to " +
            value;

        dirty = true;
        ui.app_awake();

        http_server.respond(
            req,
            200,
            "application/json",
            '{"ok":true}'
        );

        return;
    }

    // MESSAGE
    if (
        startsWith(
            path,
            "/api/message/"
        )
    ) {

        sharedMessage =
            decodeURIComponent(
                path.substring(13)
            );

        dirty = true;
        ui.app_awake();

        http_server.respond(
            req,
            200,
            "application/json",
            '{"ok":true}'
        );

        return;
    }

    // 404
    http_server.respond(
        req,
        404,
        "text/plain",
        "404"
    );
}

// ╔════════════════════════════════════════════╗
// ║                  HTML                      ║
// ╚════════════════════════════════════════════╝

function renderHome(): string {

return `
<!DOCTYPE html>
<html lang="pt-br">

<head>

<meta charset="utf-8">

<meta
name="viewport"
content="width=device-width, initial-scale=1"
>

<title>RTS LIVE SYNC</title>

<style>

*{
    margin:0;
    padding:0;
    box-sizing:border-box;
}

body{

    background:
    linear-gradient(
        135deg,
        #0f172a,
        #111827
    );

    color:white;

    font-family:Arial;

    min-height:100vh;

    display:flex;

    align-items:center;
    justify-content:center;
}

.card{

    width:850px;

    background:#111827;

    border:1px solid #1f2937;

    border-radius:24px;

    padding:40px;

    box-shadow:
    0 20px 80px rgba(0,0,0,0.5);
}

h1{

    font-size:48px;

    margin-bottom:10px;

    background:
    linear-gradient(
        90deg,
        #60a5fa,
        #c084fc
    );

    -webkit-background-clip:text;
    -webkit-text-fill-color:transparent;
}

p{

    color:#94a3b8;

    margin-bottom:25px;
}

.row{

    display:flex;

    gap:14px;

    flex-wrap:wrap;
}

button{

    border:none;

    padding:14px 24px;

    border-radius:14px;

    color:white;

    font-size:16px;

    cursor:pointer;

    transition:0.2s;
}

button:hover{

    transform:translateY(-2px);
}

.blue{
    background:#2563eb;
}

.green{
    background:#16a34a;
}

.red{
    background:#dc2626;
}

.purple{
    background:#9333ea;
}

.orange{
    background:#ea580c;
}

.slider-box{

    margin-top:35px;
}

input[type=range]{

    width:100%;

    margin-top:14px;
}

.log{

    margin-top:30px;

    background:#020617;

    border:1px solid #1e293b;

    border-radius:16px;

    padding:20px;

    min-height:120px;

    color:#94a3b8;

    white-space:pre-wrap;
}

</style>

</head>

<body>

<div class="card">

<h1>🌐 RTS LIVE SYNC</h1>

<p>
Esta pagina controla
uma UI desktop nativa
em tempo real.
</p>

<div class="row">

<button
class="blue"
onclick="setProgress(10)">
10%
</button>

<button
class="green"
onclick="setProgress(35)">
35%
</button>

<button
class="purple"
onclick="setProgress(70)">
70%
</button>

<button
class="red"
onclick="setProgress(100)">
100%
</button>

</div>

<div class="slider-box">

<p>Controle realtime</p>

<input
id="slider"
type="range"
min="0"
max="100"
value="25"
oninput="live(this.value)"
>

</div>

<div
class="row"
style="margin-top:30px;"
>

<button
class="orange"
onclick="sendMsg('Hello from browser 🌐')">
Send Hello
</button>

<button
class="blue"
onclick="sendMsg('RTS sync funcionando 🚀')">
Send Test
</button>

</div>

<div
id="log"
class="log"
>
Waiting...
</div>

</div>

<script>

async function setProgress(v){

    await fetch(
        '/api/progress/' + v
    );

    log(
        'progress = ' + v
    );
}

async function live(v){

    await fetch(
        '/api/progress/' + v
    );

    log(
        'live slider = ' + v
    );
}

async function sendMsg(msg){

    await fetch(
        '/api/message/' +
        encodeURIComponent(msg)
    );

    log(
        'message: ' + msg
    );
}

function log(t){

    document
        .getElementById('log')
        .textContent = t;
}

</script>

</body>
</html>
`;
}

// ╔════════════════════════════════════════════╗
// ║                 HELPERS                    ║
// ╚════════════════════════════════════════════╝

function startsWith(
    s: string,
    prefix: string
): bool {

    return s.substring(
        0,
        prefix.length
    ) == prefix;
}

// ╔════════════════════════════════════════════╗
// ║              MAIN THREAD TICK              ║
// ╚════════════════════════════════════════════╝
//
// FLTK e' single-threaded: todas as chamadas ui.* tem que ser feitas
// na thread que criou os widgets. Em vez de ui.app_run() (bloqueante),
// usamos ui.app_wait() (nao-bloqueante) num loop:
//
//   - app_wait_for(0.05) processa eventos pendentes (mouse, redraw,
//     callbacks dos widgets) e bloqueia ate 50ms esperando o proximo
//     evento — assim nao queima CPU em busy-wait.
//   - logo apos, conferimos `dirty` e sincronizamos os widgets na
//     thread UI (correto).
//   - quando o handler HTTP (em outra thread tokio) seta dirty=true e
//     chama ui.app_awake(), o app_wait_for retorna imediatamente e o
//     update aparece no proximo loop.
//
// Sem leak de Promise por iteracao, sem cross-thread ui.* call.

while (ui.app_wait_for(app, 0.05)) {

    if (dirty) {

        dirty = false;

        ui.progress_set_value(prog, sharedProgress);
        ui.slider_set_value(slider, sharedProgress);
        ui.output_set_value(output, sharedMessage);

        ui.widget_set_label(
            status,
            "Progress: " + sharedProgress
        );
    }
}