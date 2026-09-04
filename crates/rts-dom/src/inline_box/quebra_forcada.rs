//! `quebra_forcada_em`: onde um `\n` literal, dentro de uma corrida de
//! whitespace, força uma quebra de linha em `white-space: pre`/`pre-wrap`/
//! `pre-line`.

use super::quebra_forcada_em;

/// Um espaço comum não é uma quebra — só colapsa.
#[test]
fn espaco_sozinho_nao_e_quebra() {
    assert_eq!(quebra_forcada_em("   resto"), None);
}

/// Texto sem whitespace nenhum à frente também não.
#[test]
fn sem_whitespace_a_frente_nao_e_quebra() {
    assert_eq!(quebra_forcada_em("resto"), None);
}

/// Um `\n` sozinho: a quebra fica logo depois dele.
#[test]
fn newline_sozinho_quebra_logo_a_seguir() {
    assert_eq!(quebra_forcada_em("\nresto"), Some(1));
}

/// Espaços ANTES do `\n`, na mesma corrida: o `\n` ainda é achado, e o
/// índice devolvido é depois DELE, não do início da corrida — os espaços
/// anteriores continuam a colapsar, só o `\n` quebra.
#[test]
fn espacos_antes_do_newline_nao_escondem_a_quebra() {
    assert_eq!(quebra_forcada_em("  \nresto"), Some(3));
}

/// Só o PRIMEIRO `\n` da corrida conta — o chamador testa o resto de novo.
#[test]
fn so_o_primeiro_newline_da_corrida_e_respondido() {
    assert_eq!(quebra_forcada_em("\n\nresto"), Some(1));
}

/// Um `\n` fora da corrida INICIAL (depois de uma palavra) não é visto por
/// esta função — ela só olha para o INÍCIO de `rest`, que é a garantia que o
/// scanner de `wrap_runs` já dá ao chamar (só chama quando `rest` começa por
/// whitespace).
#[test]
fn newline_fora_do_inicio_nao_e_visto() {
    assert_eq!(quebra_forcada_em("palavra\nresto"), None);
}
