//! `a[i]` e `a[i] = v`: o que a LINGUAGEM faz, pinado.
//!
//! Estes casos existem porque duas otimizações no caminho indexado mexeram em
//! coisas observáveis: a conversão da chave passou a acontecer só quando há um
//! proxy, e `length` passou a ser escrito só quando o array cresce. As duas são
//! invisíveis num teste de velocidade e visíveis aqui.

use rts_cranelift::tags;
use rts_host::compile;

/// O número que um programa produziu.
fn numero(fonte: &str) -> f64 {
    let mut programa = compile(fonte).unwrap_or_else(|erro| panic!("compilar falhou: {erro:?}"));
    tags::decode_double(programa.run())
}

#[test]
fn escrever_dentro_do_array_nao_mexe_no_length() {
    // A otimização que isto pina: `set_length` só é chamado quando o array
    // CRESCE. Se alguém voltar a chamá-lo sempre, o teste continua passando —
    // mas se alguém parar de chamá-lo AO CRESCER, os dois de baixo quebram.
    assert_eq!(numero("const a=[1.0,2.0,3.0]; a[0]=9.0; return a.length;"), 3.0);
    assert_eq!(numero("const a=[1.0,2.0,3.0]; a[0]=9.0; return a[0];"), 9.0);
    assert_eq!(numero("const a=[1.0,2.0]; a[1]=7.0; return a.length;"), 2.0);
}

#[test]
fn escrever_alem_do_fim_cresce_o_array() {
    assert_eq!(numero("const a=[1.0,2.0]; a[2]=7.0; return a.length;"), 3.0);
    assert_eq!(numero("const a=[]; a[2]=1.0; return a.length;"), 3.0);
    // `let a = []; a[2] = 1` deixa length 3 e os dois primeiros `undefined`.
    assert_eq!(numero("const a=[]; a[2]=1.0; return a[0]===undefined?1.0:0.0;"), 1.0);
    assert_eq!(numero("const a=[1.0,2.0]; a[4]=9.0; return a[0]+a[1]+a[4];"), 12.0);
}

#[test]
fn um_zero_armazenado_sobrevive_a_um_crescimento() {
    // O comentário em `computed.rs` conta que crescer já destruiu um valor: o
    // resize preenchia com `0`, e `0` é o padrão de bits de `+0.0`, então uma
    // varredura trocava zeros genuínos por `undefined`. Um valor destruído por
    // uma escrita em OUTRO lugar é a pior forma de resposta errada.
    assert_eq!(numero("const a=[]; a[0]=0.0; a[2]=1.0; return a[0]===0.0?1.0:0.0;"), 1.0);
}

#[test]
fn o_length_acompanha_um_laco_que_preenche() {
    assert_eq!(
        numero("const a=[]; let i=0; while(i<10){a[i]=i*1.0;i=i+1;} return a.length;"),
        10.0
    );
    assert_eq!(
        numero(
            "const a=[]; let i=0; while(i<10){a[i]=i*1.0;i=i+1;} \
             let s=0.0; let j=0; while(j<10){s=s+a[j];j=j+1;} return s;"
        ),
        45.0
    );
}

#[test]
fn um_typed_array_nao_cresce_e_a_escrita_fora_some() {
    // Diferença real com o array comum, e o motivo de os dois caminhos serem
    // separados no runtime: um typed array tem tamanho fixo, então `a[9] = 1`
    // numa view de dois elementos não guarda nada que alguém possa reler.
    assert_eq!(numero("const a=new Float32Array(2); a[5]=1.0; return a.length;"), 2.0);
    assert_eq!(
        numero("const a=new Float32Array(2); a[5]=1.0; return a[5]===undefined?1.0:0.0;"),
        1.0
    );
}

#[test]
fn um_proxy_continua_interceptando_o_acesso_indexado() {
    // O que a outra otimização arriscava: a conversão da chave passou a
    // acontecer só depois de saber que há um proxy. Se a ordem ficar errada, o
    // proxy deixa de ser consultado e estes dois calam.
    assert_eq!(
        numero("const p=new Proxy({}, { get: function(t,k){ return 42.0; } }); return p[7];"),
        42.0
    );
    assert_eq!(
        numero(
            "let v=0.0; const p=new Proxy({}, { set: function(t,k,x){ v=x; return true; } }); \
             p[3]=8.0; return v;"
        ),
        8.0
    );
}
