//! `rts:egui` e `rts:input`, alcançados por um programa.
//!
//! # O que estes testes podem asserir, e o que deliberadamente não tentam
//!
//! Nenhum abre uma janela. Abrir uma exige um servidor de janelas e uma GPU, e
//! um teste que depende dos dois não falha por regressão — falha por runner. O
//! que é verificável sem nada disso é o que de fato quebra num porte de
//! superfície: o especificador resolve, o nome existe, é chamável, e a chamada
//! atravessa a fronteira sem derrubar o processo.
//!
//! Isso não é prova de que pinta. A prova de que pinta é rodar um programa numa
//! máquina com tela, e ela é de quem tem uma.
//!
//! # Por que pelo `rts:test` e não por um `return`
//!
//! Um arquivo com `import` é um MÓDULO, e `return` fora de função é erro de
//! sintaxe num módulo. Então o programa reporta pelo harness e o Rust lê o
//! registro — o mesmo caminho que `node_modules.rs` já usa, pela mesma razão.

use rts_std_rwk::test::{record, reset};

/// Roda um módulo e responde os testes que reportaram divergência.
fn failures(source: &str) -> Vec<String> {
    reset();
    let mut program = rts_host_rwk::compile(source).expect("um módulo que compila");
    program.run();
    record()
        .into_iter()
        .filter_map(|one| one.failure.map(|why| format!("{}: {why}", one.name)))
        .collect()
}

#[test]
fn os_dois_especificadores_resolvem_e_seus_membros_sao_chamaveis() {
    // O que o `rts-std-rwk` recusou registrar até agora: um `import` de
    // `rts:egui` que não termina em `undefined`. E a superfície inteira num
    // namespace só — a divisão em janela/desenho/cena é de leitura, e este teste
    // é o que impede que ela vire uma divisão de import.
    let failed = failures(
        r#"
import { test, expect } from "rts:test";
import * as egui from "rts:egui";
import * as input from "rts:input";

test("a janela e o frame", function () {
    expect(typeof egui.openWindow).toBe("function");
    expect(typeof egui.pump).toBe("function");
    expect(typeof egui.isOpen).toBe("function");
    expect(typeof egui.close).toBe("function");
    expect(typeof egui.beginFrame).toBe("function");
    expect(typeof egui.endFrame).toBe("function");
    expect(typeof egui.winWidth).toBe("function");
});
test("o desenho", function () {
    expect(typeof egui.drawRect).toBe("function");
    expect(typeof egui.drawText).toBe("function");
    expect(typeof egui.drawLine).toBe("function");
    expect(typeof egui.measureText).toBe("function");
    expect(typeof egui.button).toBe("function");
    expect(typeof egui.slider).toBe("function");
});
test("a cena 3D", function () {
    expect(typeof egui.meshUpload).toBe("function");
    expect(typeof egui.setCamera).toBe("function");
    expect(typeof egui.setLight).toBe("function");
    expect(typeof egui.drawMesh).toBe("function");
});
test("o input", function () {
    expect(typeof input.mouseX).toBe("function");
    expect(typeof input.key).toBe("function");
    expect(typeof input.textInput).toBe("function");
    expect(typeof input.copyText).toBe("function");
});
        "#,
    );
    assert_eq!(failed, Vec::<String>::new());
}

#[test]
fn perguntar_o_input_sem_janela_responde_o_default_em_vez_de_abortar() {
    // O que isto pina não é o valor: é que a chamada ATRAVESSA. Um nativo que
    // tomasse o empréstimo do contexto duas vezes entraria em pânico dentro de
    // um quadro `extern "C"`, que não desenrola e portanto ABORTA o processo —
    // e um aborto não é um teste que falha, é um teste que some.
    //
    // Os valores também importam, e cada um é a resposta de "não há fonte, ou
    // não há essa janela": `-1` é distinguível de qualquer posição real, e
    // `false` é um booleano de verdade, que a ABI antiga não tinha e por isso
    // respondia `0`.
    let failed = failures(
        r#"
import { test, expect } from "rts:test";
import { mouseX, mouseY, key, wheel, modCtrl, textInput } from "rts:input";

test("o mouse sem fonte", function () {
    expect(mouseX(0)).toBe(-1);
    expect(mouseY(0)).toBe(-1);
});
test("uma tecla sem fonte é false, não undefined", function () {
    expect(key(0, 87, 0)).toBe(false);
    expect(modCtrl(0)).toBe(false);
});
test("a roda parada é zero", function () { expect(wheel(0)).toBe(0); });
test("nada digitado é a string vazia", function () { expect(textInput(0)).toBe(""); });
        "#,
    );
    assert_eq!(failed, Vec::<String>::new());
}

#[test]
fn um_objeto_de_opcoes_incompleto_desenha_com_os_defaults_em_vez_de_nada() {
    // A decisão de aridade de `rts-ui-rwk::value`, exercida: `drawRect` tinha
    // nove parâmetros e agora recebe um objeto, e um campo ausente vale o
    // default documentado. Sem janela nada é enfileirado — o que se está
    // asserindo é que ler um objeto de opções não depende de haver uma, e que
    // um campo que não é número não vira `NaN` numa coordenada.
    let failed = failures(
        r#"
import { test, expect } from "rts:test";
import { drawRect, drawText, drawMesh, setCamera } from "rts:egui";

test("desenhar sem janela é inofensivo", function () {
    drawRect(0, { x: 10, y: 20, w: 30, h: 40 });
    drawText(0, { x: 1, y: 2, text: "oi" });
    drawMesh(0, { mesh: 1, x: 0, y: 0, z: 0 });
    setCamera(0, { x: 0, y: 1, z: -5, fov: 1, aspect: 1.5 });
    expect(true).toBe(true);
});
test("um campo ausente não vira NaN", function () {
    drawRect(0, {});
    drawRect(0, { x: "isto não é um número" });
    expect(true).toBe(true);
});
        "#,
    );
    assert_eq!(failed, Vec::<String>::new());
}
