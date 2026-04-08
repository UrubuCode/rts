use std::fmt;

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
        }
    }
}
