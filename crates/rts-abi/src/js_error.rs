/// JavaScript error class kind — zero-cost u8, matches JS spec error hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum JsErrorKind {
    #[default]
    Error = 0,
    TypeError = 1,
    RangeError = 2,
    ReferenceError = 3,
    SyntaxError = 4,
    EvalError = 5,
    UriError = 6,
}

impl JsErrorKind {
    pub fn from_name(name: &str) -> Self {
        match name {
            "TypeError" => Self::TypeError,
            "RangeError" => Self::RangeError,
            "ReferenceError" => Self::ReferenceError,
            "SyntaxError" => Self::SyntaxError,
            "EvalError" => Self::EvalError,
            "URIError" | "UriError" => Self::UriError,
            _ => Self::Error,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::TypeError => "TypeError",
            Self::RangeError => "RangeError",
            Self::ReferenceError => "ReferenceError",
            Self::SyntaxError => "SyntaxError",
            Self::EvalError => "EvalError",
            Self::UriError => "URIError",
        }
    }
}
