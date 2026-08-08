//! Uma janela de verdade, aberta e pintada por um programa, pelo motor NOVO.
//!
//! ```text
//! cargo run -p rts-host-rwk --example janela
//! ```
//!
//! # Por que um exemplo e não um teste
//!
//! Duas razões, e as duas são de plataforma:
//!
//! - O winit entra em pânico ao criar o event loop fora da thread PRINCIPAL, e
//!   um `#[test]` do cargo roda numa secundária. Existe um escape
//!   (`EventLoopBuilderExtWindows::any_thread`), e usá-lo seria contornar um
//!   aviso de compatibilidade só para poder chamar isto de teste.
//! - Abrir uma janela precisa de tela e GPU. Num runner sem as duas isto falha
//!   por ambiente, e um teste que falha por ambiente deixa de ser lido.
//!
//! O que É testado automaticamente está em `tests/ui_surface.rs`: os
//! especificadores resolvem, os membros são chamáveis, e chamar sem janela
//! atravessa a fronteira. Isto aqui responde a pergunta que nenhum deles
//! responde — se pinta — e por isso desenha algo reconhecível em vez de um frame
//! vazio: se a janela abrir preta, o loop rodou e o desenho não chegou.

fn main() {
    let source = r#"
import { openWindow, pump, isOpen, close, beginFrame, endFrame,
         drawRect, drawText, winWidth, winHeight } from "rts:egui";
import { mouseX, mouseY } from "rts:input";

const win = openWindow("rts-ui-rwk — motor novo", 720, 420, 0);
if (win > 0) {
    let frame = 0;
    while (frame < 600 && isOpen(win)) {
        pump(win);
        beginFrame(win);
        const w = winWidth(win);
        const h = winHeight(win);
        // fundo, uma barra que anda, o cursor seguido, e o texto por cima:
        // quatro caminhos num frame só.
        drawRect(win, { x: 0, y: 0, w: w, h: h, fill: 0x101820FF });
        drawRect(win, { x: 40 + (frame % 120) * 4, y: 220, w: 90, h: 90,
                        fill: 0x30A0FFFF, radius: 14 });
        drawRect(win, { x: mouseX(win) - 6, y: mouseY(win) - 6, w: 12, h: 12,
                        fill: 0xFFD060FF, radius: 6 });
        drawText(win, { x: 40, y: 60, text: "pintado pelo motor novo",
                        color: 0xFFFFFFFF, size: 26 });
        drawText(win, { x: 40, y: 100, text: "rts:egui + rts:input, via rts-ui-rwk",
                        color: 0x90A0B0FF, size: 16 });
        endFrame(win);
        frame = frame + 1;
    }
    close(win);
} else {
    print("openWindow devolveu 0 — o motivo saiu no stderr acima");
}
"#;

    let mut program = match rts_host_rwk::compile(source) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("o programa não compilou: {error:?}");
            std::process::exit(1);
        }
    };
    program.run();
}
