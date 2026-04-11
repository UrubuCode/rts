use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Number(f64),
    String(String),
    Bool(bool),
    Object(BTreeMap<String, RuntimeValue>),
    NativeFunction(String),
    Null,
    Undefined,
}

impl RuntimeValue {
    pub fn is_nullish(&self) -> bool {
        matches!(self, Self::Null | Self::Undefined)
    }

    pub fn is_string_like(&self) -> bool {
        matches!(self, Self::String(_))
    }

    pub fn get_property(&self, name: &str) -> Option<RuntimeValue> {
        match self {
            Self::Object(map) => map.get(name).cloned(),
            _ => None,
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Number(value) => !value.is_nan() && *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::Object(_) | Self::NativeFunction(_) => true,
            Self::Null | Self::Undefined => false,
        }
    }

    pub fn to_number(&self) -> f64 {
        match self {
            Self::Number(value) => *value,
            Self::Bool(true) => 1.0,
            Self::Bool(false) => 0.0,
            Self::String(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    0.0
                } else {
                    trimmed.parse::<f64>().unwrap_or(f64::NAN)
                }
            }
            Self::Object(_) | Self::NativeFunction(_) => f64::NAN,
            Self::Null => 0.0,
            Self::Undefined => f64::NAN,
        }
    }

    pub fn to_runtime_string(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Number(value) => format_number(*value),
            Self::Bool(value) => value.to_string(),
            Self::Object(_) => "[object Object]".to_string(),
            Self::NativeFunction(name) => {
                format!("function {}() {{ [native code] }}", name)
            }
            Self::Null => "null".to_string(),
            Self::Undefined => "undefined".to_string(),
        }
    }

    pub fn to_js_string(&self) -> String {
        self.to_runtime_string()
    }
}

fn format_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }
    if value.fract() == 0.0 {
        return format!("{}", value as i64);
    }
    value.to_string()
}
