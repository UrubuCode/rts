//! Arrays ESPARSOS: uma posição que não existe, e não um `undefined` guardado.
//!
//! O runtime ganhou um marcador de ausência para isto. O que ele compra é um
//! par de respostas que antes era uma só: `[,1][0]` é `undefined` e `0 in [,1]`
//! é falso, enquanto `0 in [undefined,1]` é verdadeiro.
//!
//! Antes disso o motor era INCONSISTENTE de um jeito que vale registrar: ele
//! recusava o literal `[,1]` para não dar a resposta errada, e dava exatamente
//! a mesma resposta errada quando o programa escrevia `a[2] = 1` num array
//! vazio.

use rts_cranelift::tags;
use rts_host_rwk::compile;

fn numero(fonte: &str) -> f64 {
    let mut p = compile(fonte).unwrap_or_else(|e| panic!("compilar falhou: {e:?}"));
    tags::decode_double(p.run())
}
fn verdade(fonte: &str) -> bool {
    numero(&format!("return ({fonte}) ? 1 : 0;")) == 1.0
}

#[test]
fn um_buraco_le_como_undefined_mas_nao_existe() {
    // O par inteiro da mudança, em quatro linhas.
    assert_eq!(numero("return [,1][0] === undefined ? 1 : 0;"), 1.0);
    assert!(!verdade("0 in [,1]"), "um buraco não existe");
    assert!(verdade("0 in [undefined,1]"), "um undefined GUARDADO existe");
    assert!(!verdade("[,1].hasOwnProperty(0)"));
}

#[test]
fn o_literal_conta_buracos_no_comprimento_e_a_virgula_final_nao() {
    assert_eq!(numero("return [1,,3].length;"), 3.0);
    assert_eq!(numero("return [1,,3][2];"), 3.0);
    // `[1, 2, ]` é 2 — aqui a vírgula é separador. Um a mais e vira buraco.
    assert_eq!(numero("return [1,2,].length;"), 2.0);
    assert_eq!(numero("return [1,2,,].length;"), 3.0);
    assert_eq!(numero("return [,,].length;"), 2.0);
}

#[test]
fn crescer_por_indice_deixa_buracos_e_nao_undefined() {
    // O caminho que já existia e estava errado: `a[2] = 1` num array vazio.
    assert_eq!(numero("const a=[]; a[2]=1; return a.length;"), 3.0);
    assert_eq!(numero("const a=[]; a[2]=1; return (0 in a) ? 1 : 0;"), 0.0);
    assert_eq!(numero("const a=[]; a[2]=1; return (2 in a) ? 1 : 0;"), 1.0);
}

#[test]
fn new_array_de_n_sao_n_buracos() {
    assert_eq!(numero("return new Array(3).length;"), 3.0);
    assert!(!verdade("0 in new Array(3)"));
    assert_eq!(numero("return new Array(3)[0] === undefined ? 1 : 0;"), 1.0);
}

#[test]
fn object_keys_lista_so_o_que_existe() {
    // Deste ponto saem também `for-in`, `Object.values/entries`, `assign` e o
    // `JSON.stringify` de objeto.
    assert_eq!(numero("return Object.keys([1,,3]).length;"), 2.0);
    assert_eq!(numero("const a=[]; a[2]=1; return Object.keys(a).length;"), 1.0);
    assert_eq!(numero("return Object.keys([1,2,3]).length;"), 3.0);
}

#[test]
fn a_iteracao_pula_buracos_menos_onde_a_especificacao_manda_visitar() {
    assert_eq!(
        numero("let n=0; [1,,3].forEach(function(){n=n+1;}); return n;"),
        2.0,
        "forEach percorre as chaves que existem"
    );
    assert_eq!(numero("return [1,,3].filter(function(){return true;}).length;"), 2.0);
    // `map` é o caso especial: preserva comprimento E esparsidade, sem chamar
    // a callback no buraco.
    assert_eq!(numero("return [1,,3].map(function(){return 9;}).length;"), 3.0);
    assert_eq!(
        numero("const m = [1,,3].map(function(){return 9;}); return (1 in m) ? 1 : 0;"),
        0.0
    );
    // `find` NÃO pula — visita com `undefined`. É a exceção deliberada.
    assert_eq!(
        numero("return [,1].find(function(x){return x===undefined;}) === undefined ? 1 : 0;"),
        1.0
    );
}

#[test]
fn includes_acha_o_buraco_e_indexof_o_pula() {
    // A diferença entre os dois é que `includes` percorre `0..length` e
    // `indexOf` percorre as chaves que existem. É o par mais fino disto tudo.
    assert!(verdade("[,1].includes(undefined)"));
    assert_eq!(numero("return [,undefined].indexOf(undefined);"), 1.0);
}

#[test]
fn um_array_denso_nao_muda_em_nada() {
    // A rede de segurança: nada acima pode ter custado o caso comum.
    assert_eq!(numero("const a=[1,2,3]; return a.length;"), 3.0);
    assert!(verdade("0 in [1,2,3]"));
    assert_eq!(numero("let n=0; [1,2,3].forEach(function(){n=n+1;}); return n;"), 3.0);
    assert_eq!(numero("return [1,2].map(function(x){return x*2;})[1];"), 4.0);
    assert_eq!(numero("return [1,2,3].filter(function(x){return x>1;}).length;"), 2.0);
    assert_eq!(numero("const a=[1]; a.push(2); return a.length;"), 2.0);
    assert_eq!(numero("return [1,2,3].pop();"), 3.0);
    assert_eq!(numero("return [1,2,3].shift();"), 1.0);
}

#[test]
fn os_pontos_que_devolvem_o_valor_cru_nao_vazam_o_marcador() {
    // `pop`, `shift`, `at` e `find` entregam o word direto ao programa sem
    // passar pelo funil de `a[i]`. Se algum esquecer a conversão, o programa vê
    // um valor que não existe em JavaScript — e `typeof` o denuncia.
    assert_eq!(numero("return [1,].pop() === 1 ? 1 : 0;"), 1.0);
    assert_eq!(numero("return [,1].shift() === undefined ? 1 : 0;"), 1.0);
    assert_eq!(numero("return [,1].at(0) === undefined ? 1 : 0;"), 1.0);
    assert_eq!(numero("return typeof [,1].at(0) === 'undefined' ? 1 : 0;"), 1.0);
    assert_eq!(numero("return typeof [,1][0] === 'undefined' ? 1 : 0;"), 1.0);
}

#[test]
fn delete_de_um_elemento_deixa_um_buraco() {
    // Isto NÃO era uma recusa: `delete a[0]` compilava, percorria o caminho de
    // shape, não achava o índice ali (um elemento não está na shape) e
    // respondia `true` sem apagar nada. Um `delete` que diz ter apagado e não
    // apagou é a forma de erro que este projeto persegue em toda parte.
    //
    // É o único caminho que cria um buraco num array que já existe — os outros
    // dois (o literal e o crescimento por índice) o criam ao nascer.
    assert_eq!(numero("const a=[1,2]; return (delete a[0]) ? 1 : 0;"), 1.0);
    assert_eq!(numero("const a=[1,2]; delete a[0]; return (0 in a) ? 1 : 0;"), 0.0);
    assert_eq!(numero("const a=[1,2]; delete a[0]; return a.length;"), 2.0, "delete não encolhe");
    assert_eq!(numero("const a=[1,2]; delete a[0]; return a[0] === undefined ? 1 : 0;"), 1.0);
    assert_eq!(numero("const a=[1,2]; delete a[0]; return a[1];"), 2.0, "o vizinho fica");
    assert_eq!(numero("const a=[1,2]; delete a[0]; return Object.keys(a).length;"), 1.0);
    assert_eq!(
        numero("const a=[1,2]; delete a[0]; let n=0; a.forEach(function(){n=n+1;}); return n;"),
        1.0
    );
    // Além do fim não há o que remover, e `delete` responde `true` por não
    // existir — a mesma leitura que o caminho de propriedade faz.
    assert_eq!(numero("const a=[1]; return (delete a[9]) ? 1 : 0;"), 1.0);
    // E o caminho de propriedade continua intacto.
    assert_eq!(numero("const o={x:1}; delete o.x; return ('x' in o) ? 1 : 0;"), 0.0);
}

#[test]
fn math_clz32_e_imul_existem_e_respeitam_os_32_bits() {
    // Os dois únicos métodos padrão que faltavam entre os que a suíte usa. O que
    // eles pinam é a aritmética de 32 bits, onde a implementação óbvia erra:
    // `as u32` em Rust SATURA, e `clz32(2**32)` responderia 0 em vez de 32.
    assert_eq!(numero("return Math.clz32(1);"), 31.0);
    assert_eq!(numero("return Math.clz32(0);"), 32.0);
    assert_eq!(numero("return Math.clz32(4294967296);"), 32.0, "2**32 envolve para 0");
    assert_eq!(numero("return Math.clz32(-1);"), 0.0);
    // `imul` existe justamente porque `a * b` é um double: este é o produto que
    // TRANSBORDA, e é o que um programa portado de C espera.
    assert_eq!(numero("return Math.imul(3,4);"), 12.0);
    assert_eq!(numero("return Math.imul(65535,65535);"), -131071.0);
    assert_eq!(numero("return Math.imul(2147483648,2);"), 0.0);
}
