/// Carrega uma fonte de UI de QUALIDADE no contexto egui (substitui a fonte default
/// do egui, que destoa de um browser). Tenta o sistema (Segoe UI no Windows — a mesma
/// que o Chrome usa, p/ paridade visual); cai na fonte default se não achar. A fonte
/// vira a Proportional padrão; uma mono do sistema vira a Monospace.
pub(in crate::app) fn install_ui_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // candidatos de fonte proporcional (1ª que existir vence).
    let proportional = [
        "C:/Windows/Fonts/segoeui.ttf", // Windows — igual ao Chrome
        "/System/Library/Fonts/SFNS.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    let monospace = [
        "C:/Windows/Fonts/consola.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ];
    // fonte BOLD do sistema → família NOMEADA "bold" (font-weight:700). Medir/pintar
    // bold com a fonte regular faz o texto estourar a linha (a bold é mais larga).
    let bold = [
        "C:/Windows/Fonts/segoeuib.ttf", // Segoe UI Bold (Windows)
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ];
    // ITÁLICO e BOLD-ITÁLICO: ficheiros de fonte PRÓPRIOS, e é essa a razão de
    // existirem aqui. A alternativa rejeitada foi inclinar os glifos da regular
    // por transformação — um oblique sintético não é o que o Chrome pinta (a
    // Segoe UI Italic tem desenhos de letra diferentes, não a mesma letra
    // torcida), e teria a aparência de estar certo enquanto a paridade dizia o
    // contrário. Sem estes ficheiros, `font-style: italic` não tem glifos e a
    // resposta honesta é pintar regular (ver `familia_ou`).
    let italic = [
        "C:/Windows/Fonts/segoeuii.ttf", // Segoe UI Italic (Windows)
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
    ];
    let bold_italic = [
        "C:/Windows/Fonts/segoeuiz.ttf", // Segoe UI Bold Italic (Windows)
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf",
    ];
    let mut load = |paths: &[&str], name: &str, family: egui::FontFamily| -> bool {
        for p in paths {
            if let Ok(bytes) = std::fs::read(p) {
                fonts.font_data.insert(name.to_string(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
                // insere no INÍCIO da família (vira a preferida).
                fonts.families.entry(family.clone()).or_default().insert(0, name.to_string());
                return true;
            }
        }
        false
    };
    load(&proportional, "ui-sans", egui::FontFamily::Proportional);
    load(&monospace, "ui-mono", egui::FontFamily::Monospace);
    load(&bold, "ui-bold", egui::FontFamily::Name("bold".into()));
    load(&italic, "ui-italic", egui::FontFamily::Name("italic".into()));
    load(&bold_italic, "ui-bold-italic", egui::FontFamily::Name("bold-italic".into()));
    drop(load);
    // Uma família NOMEADA que o render peça e não exista faz o egui entrar em
    // pânico ao montar a fonte — e o render pede "italic"/"bold-italic" sempre
    // que o CSS o diz, num sistema que pode não ter os ficheiros. Então garante-se
    // que as três existem, caindo na proporcional (regular) quando o ficheiro
    // falta: o texto sai DIREITO, que é a recusa honesta, e não inclinado à
    // força nem a rebentar o processo.
    for nome in ["bold", "italic", "bold-italic"] {
        let familia = egui::FontFamily::Name(nome.into());
        let vazia = fonts.families.get(&familia).map_or(true, |v| v.is_empty());
        if vazia {
            let base = fonts
                .families
                .get(&egui::FontFamily::Proportional)
                .cloned()
                .unwrap_or_default();
            fonts.families.insert(familia, base);
        }
    }
    // `set_fonts` deixou de estar preso ao `got_prop`: as famílias nomeadas acima
    // só chegam ao contexto por aqui, e sem elas um sistema sem Segoe UI pedia
    // "italic" a um contexto que nunca o ouviu — o pânico que o bloco anterior
    // evita. Quando nada carregou, isto instala as fontes default do egui, que é
    // exatamente o que o contexto já tinha.
    ctx.set_fonts(fonts);
}
