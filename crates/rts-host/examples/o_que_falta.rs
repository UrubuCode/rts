//! O que o motor NOVO ainda recusa de um projeto real.
//!
//! Aponta para um diretório de `.ts` e tenta compilar cada arquivo, agrupando os
//! motivos de recusa por frequência. A saída é a lista de trabalho ordenada por
//! quantos arquivos cada gap desbloqueia — que é a informação que uma contagem
//! de "N de M passam" não dá.
//!
//! ```text
//! cargo run -p rts-host --example o_que_falta -- C:\caminho\do\projeto
//! ```
//!
//! Compila arquivo a arquivo, e não o grafo, de propósito: um grafo para no
//! primeiro import quebrado e esconde tudo atrás dele.
//!
//! # O QUE ESTE NÚMERO NÃO DIZ, e a confusão é fácil
//!
//! "Compila" aqui é sobre a LINGUAGEM: sintaxe, construções, nomes que o
//! emissor resolve. **Não** é sobre rodar. Um `import { x } from "rts:coisa"`
//! de um especificador que o host não registrou compila sem reclamar e responde
//! `undefined` em tempo de execução — é o que `entry/modules.rs` documenta como
//! escolha deliberada.
//!
//! Então um projeto pode ter 100 % dos arquivos "compilando" e não rodar uma
//! linha, por falta de namespace. A lista de namespaces é a outra metade da
//! conta, e se obtém com um grep dos `from "rts:…"` contra os
//! `declare_module` que o host chama.

use std::collections::BTreeMap;

fn main() {
    let raiz = match std::env::args().nth(1) {
        Some(caminho) => std::path::PathBuf::from(caminho),
        None => {
            eprintln!("uso: o_que_falta <diretorio com .ts>");
            std::process::exit(2);
        }
    };

    let mut arquivos = Vec::new();
    colher(&raiz, &mut arquivos);
    arquivos.sort();

    let mut motivos: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ok = 0usize;

    for arquivo in &arquivos {
        let fonte = match std::fs::read_to_string(arquivo) {
            Ok(texto) => texto,
            Err(_) => continue,
        };
        let nome = arquivo
            .strip_prefix(&raiz)
            .unwrap_or(arquivo)
            .display()
            .to_string();
        match rts_host::compile(&fonte) {
            Ok(_) => ok += 1,
            Err(erro) => {
                let motivo = resumir(&format!("{erro:?}"));
                motivos.entry(motivo).or_default().push(nome);
            }
        }
    }

    println!("\n=== {} arquivos, {ok} compilam inteiros ===\n", arquivos.len());
    let mut ordenado: Vec<_> = motivos.iter().collect();
    ordenado.sort_by_key(|(_, arqs)| std::cmp::Reverse(arqs.len()));
    for (motivo, arqs) in ordenado {
        println!("{:>4} arquivos  {motivo}", arqs.len());
        for a in arqs.iter().take(3) {
            println!("            {a}");
        }
        if arqs.len() > 3 {
            println!("            (+{} outros)", arqs.len() - 3);
        }
    }
}

/// Reduz a mensagem ao NOME da construção, para que o agrupamento junte os
/// arquivos que esbarram na mesma coisa em vez de listar 82 mensagens únicas.
fn resumir(bruto: &str) -> String {
    if let Some(inicio) = bruto.find("construct: \"") {
        let resto = &bruto[inicio + 12..];
        if let Some(fim) = resto.find('"') {
            return format!("construção: {}", &resto[..fim]);
        }
    }
    if bruto.contains("UnboundName") {
        if let Some(i) = bruto.find("name: ") {
            let resto = &bruto[i + 6..];
            let fim = resto.find([',', ' ', '}']).unwrap_or(resto.len());
            return format!("nome não resolvido: {}", resto[..fim].trim_matches('"'));
        }
    }
    let corte = bruto.len().min(90);
    bruto[..corte].replace('\n', " ")
}

fn colher(dir: &std::path::Path, saida: &mut Vec<std::path::PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(dir) else { return };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        let nome = caminho.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if nome.starts_with('.') || nome == "node_modules" || nome == "build" {
            continue;
        }
        if caminho.is_dir() {
            colher(&caminho, saida);
        } else if caminho.extension().and_then(|e| e.to_str()) == Some("ts") {
            saida.push(caminho);
        }
    }
}
