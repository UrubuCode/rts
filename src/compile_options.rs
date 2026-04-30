use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompilationProfile {
    #[default]
    Development,
    Production,
}

impl CompilationProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    pub fn includes_trace_data(self, debug: bool) -> bool {
        matches!(self, Self::Development) || debug
    }

    pub fn is_development(self) -> bool {
        matches!(self, Self::Development)
    }

    /// Detecta o modo a partir do arquivo `.env` no diretório do projeto.
    ///
    /// Prioridade: `RTS_MODE` > `NODE_ENV` > `APP_ENV`.
    /// Valores aceitos: `"development"` ou `"production"`.
    /// Se nenhuma variável for encontrada, retorna `None` (cabe ao caller definir o padrão).
    pub fn from_env(project_root: &Path) -> Option<Self> {
        // Tenta ler o .env do projeto
        let env_path = project_root.join(".env");
        if let Ok(content) = std::fs::read_to_string(&env_path) {
            if let Some(profile) = parse_env_vars(&content) {
                return Some(profile);
            }
        }

        // Fallback: variáveis de ambiente do processo
        for var in &["RTS_MODE", "NODE_ENV", "APP_ENV"] {
            if let Ok(val) = std::env::var(var) {
                if let Some(profile) = mode_from_str(&val) {
                    return Some(profile);
                }
            }
        }

        None
    }
}

fn parse_env_vars(content: &str) -> Option<CompilationProfile> {
    // Processa em ordem de prioridade: RTS_MODE > NODE_ENV > APP_ENV
    let mut rts_mode: Option<CompilationProfile> = None;
    let mut node_env: Option<CompilationProfile> = None;
    let mut app_env: Option<CompilationProfile> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key {
                "RTS_MODE" => rts_mode = mode_from_str(value),
                "NODE_ENV" => node_env = mode_from_str(value),
                "APP_ENV" => app_env = mode_from_str(value),
                _ => {}
            }
        }
    }

    rts_mode.or(node_env).or(app_env)
}

fn mode_from_str(value: &str) -> Option<CompilationProfile> {
    match value {
        "development" | "dev" => Some(CompilationProfile::Development),
        "production" | "prod" => Some(CompilationProfile::Production),
        _ => None,
    }
}

impl fmt::Display for CompilationProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrontendMode {
    #[default]
    Native,
    Compat,
}

impl FrontendMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Compat => "compat",
        }
    }
}

impl fmt::Display for FrontendMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CompileOptions {
    pub profile: CompilationProfile,
    pub debug: bool,
    pub frontend_mode: FrontendMode,
    pub emit_module_progress: bool,
    /// Link all namespace symbols regardless of usage (disables linker DCE on
    /// the runtime archive). Required when the binary uses `import(variable)`.
    pub all_namespaces: bool,
}

impl CompileOptions {
    pub fn development() -> Self {
        Self::default()
    }

    pub fn include_trace_data(self) -> bool {
        self.profile.includes_trace_data(self.debug)
    }
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            profile: CompilationProfile::Development,
            debug: false,
            frontend_mode: FrontendMode::Native,
            emit_module_progress: false,
            all_namespaces: false,
        }
    }
}

/// Le `RTS_OPT_LEVEL` e devolve um valor aceito por Cranelift.
/// Aceita: `none` | `speed` | `speed_and_size`. Default `speed`.
/// `none` reduz tempo de compilacao em troca de codigo mais lento — util
/// pra debug rapido de codegen e iteracoes em testes.
pub fn opt_level() -> &'static str {
    match std::env::var("RTS_OPT_LEVEL").as_deref() {
        Ok("none") => "none",
        Ok("speed_and_size") => "speed_and_size",
        Ok("speed") => "speed",
        _ => "speed",
    }
}
