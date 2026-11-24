//! Add-on-controlled presentation only. Omission keeps the original chart defaults.
use super::{Chart, ChartData, coordinate, valid_color};
use crate::view::table::{CellImage, CellImageData};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartTextSizes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_title: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_title: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_ticks: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_ticks: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartBorder {
    pub color: String,
    pub width: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartGridStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Even number of alternating on/off lengths in logical pixels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartOptions {
    #[serde(default)]
    pub text: ChartTextSizes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plot_background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<ChartBorder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f32>,
    /// X controls vertical grid lines, Y controls horizontal grid lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_grid: Option<ChartGridStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_grid: Option<ChartGridStyle>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartBlockStyle {
    #[serde(default)]
    pub radius: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<ChartBorder>,
}

/// Uses exactly the file-backed and in-memory image descriptions used by tables.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartImageSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<CellImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_data: Option<CellImageData>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum ChartImageLocation {
    Plot { x: f64, y: f64 },
    Cell { series: usize, cell: usize },
    XTick { tick: usize },
    YTick { tick: usize },
    Legend { series: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartImage {
    #[serde(flatten)]
    pub source: ChartImageSource,
    #[serde(flatten)]
    pub location: ChartImageLocation,
}

fn bounded(v: f32, max: f32) -> bool {
    v.is_finite() && v >= 0. && v <= max
}
impl ChartBorder {
    pub fn validate(&self) -> bool {
        valid_color(&Some(self.color.clone())) && bounded(self.width, 16.)
    }
}
impl ChartBlockStyle {
    pub fn validate(&self) -> bool {
        bounded(self.radius, 64.)
            && valid_color(&self.fill)
            && self.border.as_ref().is_none_or(ChartBorder::validate)
    }
}
impl ChartGridStyle {
    pub fn validate(&self) -> bool {
        valid_color(&self.color)
            && self.width.is_none_or(|v| bounded(v, 16.))
            && self.dash.len() <= 16
            && self.dash.len().is_multiple_of(2)
            && self
                .dash
                .iter()
                .all(|v| v.is_finite() && (1. ..=100.).contains(v))
    }
}
impl ChartTextSizes {
    pub fn validate(&self) -> bool {
        [
            self.title,
            self.caption,
            self.x_title,
            self.y_title,
            self.x_ticks,
            self.y_ticks,
            self.legend,
            self.labels,
        ]
        .into_iter()
        .flatten()
        .all(|v| v.is_finite() && (6. ..=64.).contains(&v))
    }
}
impl ChartOptions {
    pub fn validate(&self) -> bool {
        self.text.validate()
            && valid_color(&self.background)
            && valid_color(&self.plot_background)
            && self.border.as_ref().is_none_or(ChartBorder::validate)
            && self.corner_radius.is_none_or(|v| bounded(v, 64.))
            && self.x_grid.as_ref().is_none_or(ChartGridStyle::validate)
            && self.y_grid.as_ref().is_none_or(ChartGridStyle::validate)
    }
}
impl ChartImageSource {
    fn validate_size(&self) -> bool {
        let valid = |v: Option<f32>| v.is_none_or(|v| v.is_finite() && (1. ..=512.).contains(&v));
        self.image.as_ref().is_none_or(|image| {
            let (w, h) = image.size(16.);
            valid(Some(w)) && valid(Some(h))
        }) && self
            .image_data
            .as_ref()
            .is_none_or(|image| valid(image.width) && valid(image.height))
    }
}
impl Chart {
    pub(super) fn validate_options(&self) -> Result<(), String> {
        if self.options.as_ref().is_some_and(|v| !v.validate()) {
            return Err("Invalid chart options.".into());
        }
        for s in &self.series {
            if s.line_width
                .is_some_and(|v| !v.is_finite() || !(0.5..=16.).contains(&v))
                || s.blocks.as_ref().is_some_and(|v| !v.validate())
            {
                return Err("Invalid chart series style.".into());
            }
            if let ChartData::Ranges { cells } | ChartData::Heatmap { cells } = &s.data
                && cells
                    .iter()
                    .any(|c| c.style.as_ref().is_some_and(|v| !v.validate()))
            {
                return Err("Invalid chart block style.".into());
            }
        }
        if self.images.len() > 64 {
            return Err("Chart supports at most 64 images.".into());
        }
        let mut occupied = std::collections::HashSet::new();
        for image in &self.images {
            if !image.source.validate_size() {
                return Err("Invalid chart image size.".into());
            }
            let slot = match image.location {
                ChartImageLocation::Plot { x, y } => {
                    if !coordinate(x) || !coordinate(y) {
                        return Err("Invalid chart image position.".into());
                    }
                    continue;
                }
                ChartImageLocation::Cell { series, cell } => {
                    let Some(s) = self.series.get(series) else {
                        return Err("Invalid chart image series.".into());
                    };
                    let (ChartData::Ranges { cells } | ChartData::Heatmap { cells }) = &s.data else {
                        return Err("Cell images need range or heatmap cells.".into());
                    };
                    if cell >= cells.len() {
                        return Err("Invalid chart image cell.".into());
                    }
                    (0, series, cell)
                }
                ChartImageLocation::XTick { tick } => {
                    if tick >= self.x.ticks.len() {
                        return Err("Invalid chart image tick.".into());
                    }
                    (1, 0, tick)
                }
                ChartImageLocation::YTick { tick } => {
                    if tick >= self.y.ticks.len() {
                        return Err("Invalid chart image tick.".into());
                    }
                    (2, 0, tick)
                }
                ChartImageLocation::Legend { series } => {
                    if series >= self.series.len() {
                        return Err("Invalid chart image legend.".into());
                    }
                    (3, series, 0)
                }
            };
            if !occupied.insert(slot) {
                return Err("Chart image positions must be unique.".into());
            }
        }
        Ok(())
    }
}
