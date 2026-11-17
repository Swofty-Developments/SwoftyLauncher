use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::view::table::{is_yes, yes};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Form {
    pub id: String,
    pub fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Number(NumberField),
    Slider(NumberField),
    Text(TextField),
    Select(SelectField),
    Toggle(ToggleField),
    Colour(ColourField),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NumberField {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

impl NumberField {
    pub fn step(&self) -> f64 {
        self.step.filter(|s| *s > 0. && s.is_finite()).unwrap_or(1.)
    }

    pub fn clamp(&self, value: f64) -> f64 {
        let step = self.step();
        let base = self.min.unwrap_or(0.);
        let mut v = base + ((value - base) / step).round() * step;
        if let Some(min) = self.min {
            v = v.max(min);
        }
        if let Some(max) = self.max {
            v = v.min(max);
        }
        let digits = decimals(step);
        let scale = 10f64.powi(digits as i32);
        (v * scale).round() / scale
    }

    pub fn json(&self, value: f64) -> serde_json::Value {
        if value.fract() == 0. && self.step().fract() == 0. && value.abs() < 9.0e15 {
            serde_json::Value::from(value as i64)
        } else {
            serde_json::Number::from_f64(value).map_or(serde_json::Value::Null, serde_json::Value::Number)
        }
    }

    pub fn text(&self, value: f64) -> String {
        format!("{value:.*}", decimals(self.step()))
    }
}

fn decimals(step: f64) -> usize {
    let text = format!("{step}");
    text.split_once('.').map_or(0, |(_, f)| f.len().min(6))
}

pub fn number(v: f64) -> String {
    if v.fract() == 0. && v.abs() < 9.0e15 {
        format!("{}", v as i64)
    } else {
        let t = format!("{v:.2}");
        t.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextField {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_len: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SelectField {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub options: Vec<Choice>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub search: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Choice {
    Text(String),
    Spec {
        value: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<PathBuf>,
    },
    Heading {
        heading: String,
    },
}

impl Choice {
    pub fn value(&self) -> &str {
        match self {
            Self::Text(t) => t,
            Self::Spec { value, .. } => value,
            Self::Heading { .. } => "",
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Text(t) => t,
            Self::Spec { label, .. } => label,
            Self::Heading { heading } => heading,
        }
    }

    pub fn is_heading(&self) -> bool {
        matches!(self, Self::Heading { .. })
    }

    pub fn image(&self) -> Option<&std::path::Path> {
        match self {
            Self::Spec { image, .. } => image.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToggleField {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(default)]
    pub value: bool,
}

/// A colour, sent as `#RRGGBB`. `value` takes `#RRGGBB` or `#RGB`, with or without `#`, in
/// either case; `hex: false` shows only the colour's dot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColourField {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub hex: bool,
}

impl ColourField {
    pub fn rgb(&self) -> Option<[u8; 3]> {
        self.value.as_deref().and_then(parse_colour)
    }
}

pub fn parse_colour(text: &str) -> Option<[u8; 3]> {
    let text = text.trim();
    let digits = text.strip_prefix('#').unwrap_or(text);
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&digits[i..i + 1], 16).ok();
    match digits.len() {
        3 => Some([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17]),
        6 => {
            let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
            Some([byte(0)?, byte(2)?, byte(4)?])
        }
        _ => None,
    }
}

pub fn colour_hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

impl Field {
    pub fn id(&self) -> &str {
        match self {
            Self::Number(f) | Self::Slider(f) => &f.id,
            Self::Text(f) => &f.id,
            Self::Select(f) => &f.id,
            Self::Toggle(f) => &f.id,
            Self::Colour(f) => &f.id,
        }
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Number(f) | Self::Slider(f) => f.label.as_deref(),
            Self::Text(f) => f.label.as_deref(),
            Self::Select(f) => f.label.as_deref(),
            Self::Toggle(f) => f.label.as_deref(),
            Self::Colour(f) => f.label.as_deref(),
        }
    }

    pub fn help(&self) -> Option<&str> {
        match self {
            Self::Number(f) | Self::Slider(f) => f.help.as_deref(),
            Self::Text(f) => f.help.as_deref(),
            Self::Select(f) => f.help.as_deref(),
            Self::Toggle(f) => f.help.as_deref(),
            Self::Colour(f) => f.help.as_deref(),
        }
    }

    pub fn disabled(&self) -> bool {
        match self {
            Self::Number(f) | Self::Slider(f) => f.disabled,
            Self::Text(f) => f.disabled,
            Self::Select(f) => f.disabled,
            Self::Toggle(f) => f.disabled,
            Self::Colour(f) => f.disabled,
        }
    }

    pub fn value(&self) -> serde_json::Value {
        use serde_json::Value;
        match self {
            Self::Number(f) => f.value.map_or(Value::Null, |v| f.json(v)),
            Self::Slider(f) => f.json(f.value.unwrap_or(f.min.unwrap_or(0.))),
            Self::Text(f) => Value::String(f.value.clone().unwrap_or_default()),
            Self::Select(f) => f
                .value
                .clone()
                .or_else(|| f.options.first().map(|o| o.value().to_owned()))
                .map_or(Value::Null, Value::String),
            Self::Toggle(f) => Value::Bool(f.value),
            Self::Colour(f) => f.rgb().map_or(Value::Null, |c| Value::String(colour_hex(c))),
        }
    }
}
