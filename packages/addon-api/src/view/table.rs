use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::view::Block;
use crate::view::blocks::{Fit, Search};
use crate::view::form::Field;
use crate::view::style::Tone;

pub(crate) fn yes() -> bool {
    true
}

pub(crate) fn is_yes(v: &bool) -> bool {
    *v
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub columns: Vec<Column>,
    pub rows: Vec<TableRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<Search>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub compact: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty: Option<String>,
    /// Taller rows, such as for pictures in cells; never shorter than the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_height: Option<f32>,
}

impl Table {
    pub fn plain(columns: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            id: None,
            columns: columns.into_iter().map(Column::Name).collect(),
            rows: rows
                .into_iter()
                .map(|r| TableRow::Cells(r.into_iter().map(Cell::Text).collect()))
                .collect(),
            sort: None,
            search: None,
            compact: false,
            empty: None,
            row_height: None,
        }
    }

    /// Whether clicking a row opens its detail panel.
    pub fn has_detail(&self) -> bool {
        self.rows.iter().any(|r| !r.detail().is_empty())
    }

    pub fn id(&self) -> &str {
        self.id.as_deref().unwrap_or("table")
    }

    pub fn is_plain(&self) -> bool {
        self.id.is_none()
            && self.sort.is_none()
            && self.search.is_none()
            && !self.compact
            && self.row_height.is_none()
            && self.columns.iter().all(|c| matches!(c, Column::Name(_)))
            && self.rows.iter().all(
                |r| matches!(r, TableRow::Cells(cells) if cells.iter().all(|c| matches!(c, Cell::Text(_)))),
            )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Column {
    Name(String),
    Spec(ColumnSpec),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ColumnSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub align: Align,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sort: bool,
}

impl Column {
    pub fn title(&self) -> &str {
        match self {
            Self::Name(t) => t,
            Self::Spec(s) => &s.title,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            Self::Spec(ColumnSpec { key: Some(k), .. }) => k,
            _ => self.title(),
        }
    }

    pub fn spec(&self) -> ColumnSpec {
        match self {
            Self::Name(t) => ColumnSpec {
                title: t.clone(),
                ..ColumnSpec::default()
            },
            Self::Spec(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sort {
    pub column: String,
    #[serde(default)]
    pub desc: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TableRow {
    Cells(Vec<Cell>),
    Spec(RowSpec),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub cells: Vec<Cell>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
    /// The detail panel's title and subtitle; the title defaults to the text of the first cell
    /// that isn't a switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Blocks shown in a panel beside the table while the row is selected; a row with
    /// detail is selected by clicking it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail: Vec<Block>,
}

impl TableRow {
    pub fn cells(&self) -> &[Cell] {
        match self {
            Self::Cells(c) => c,
            Self::Spec(r) => &r.cells,
        }
    }

    pub fn id(&self, index: usize) -> String {
        match self {
            Self::Spec(RowSpec { id: Some(id), .. }) => id.clone(),
            _ => index.to_string(),
        }
    }

    pub fn tone(&self) -> Option<Tone> {
        match self {
            Self::Cells(_) => None,
            Self::Spec(r) => r.tone,
        }
    }

    pub fn detail(&self) -> &[Block] {
        match self {
            Self::Cells(_) => &[],
            Self::Spec(r) => &r.detail,
        }
    }

    /// The detail panel's title: the row's own, else the text of its first cell that isn't a
    /// switch.
    pub fn title(&self) -> String {
        match self {
            Self::Spec(RowSpec { title: Some(t), .. }) => t.clone(),
            _ => self
                .cells()
                .iter()
                .find(|c| !matches!(c, Cell::Spec(s) if matches!(s.edit, Some(Field::Toggle(_)))))
                .map(|c| c.text().to_owned())
                .unwrap_or_default(),
        }
    }

    pub fn subtitle(&self) -> Option<&str> {
        match self {
            Self::Spec(r) => r.subtitle.as_deref(),
            Self::Cells(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cell {
    Text(String),
    Spec(Box<CellSpec>),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellSpec {
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<SortKey>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pill: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<Field>,
    /// A picture before the text, such as a portrait or a flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<CellImage>,
    /// Small helper-supplied PNG, kept only in memory. Requires `view.cell.image_data`.
    /// A valid image_data takes precedence over the optional file-backed image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_data: Option<CellImageData>,
    /// The text in pieces with their own weight and colour, drawn instead of `text`;
    /// `text` still searches, sorts and shows on launchers without spans.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
}

/// An in-memory cell picture: base64 PNG, at most 64 KiB encoded and 256 x 256 pixels.
/// Invalid images leave the cell's text and optional file-backed fallback available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellImageData {
    pub png: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(default)]
    pub fit: Fit,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub transparent: bool,
}

impl CellImageData {
    pub fn size(&self, side: f32) -> (f32, f32) {
        match (self.width, self.height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w),
            (None, Some(h)) => (h, h),
            _ => (side, side),
        }
    }
}

/// A cell's picture: the absolute path of a PNG or JPEG under the addon's cache or data
/// folder, or that with a size in pixels. Without a size it is a square as tall as the row
/// allows; with one it fills that box, cropped to it unless `fit` is `contain`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CellImage {
    Path(PathBuf),
    Sized {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fit: Option<Fit>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        transparent: bool,
    },
}

impl CellImage {
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Path(p) | Self::Sized { path: p, .. } => p,
        }
    }

    /// The box's width and height, given the side of the default square.
    pub fn size(&self, side: f32) -> (f32, f32) {
        match self {
            Self::Path(_) => (side, side),
            Self::Sized { width, height, .. } => match (width, height) {
                (Some(w), Some(h)) => (*w, *h),
                (Some(w), None) => (*w, *w),
                (None, Some(h)) => (*h, *h),
                (None, None) => (side, side),
            },
        }
    }

    pub fn fit(&self) -> Fit {
        match self {
            Self::Sized { fit: Some(f), .. } => *f,
            _ => Fit::Cover,
        }
    }

    pub fn transparent(&self) -> bool {
        matches!(
            self,
            Self::Sized {
                transparent: true,
                ..
            }
        )
    }
}

/// A piece of a cell's text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
}

impl Cell {
    pub fn text(&self) -> &str {
        match self {
            Self::Text(t) => t,
            Self::Spec(c) => &c.text,
        }
    }

    pub fn spec(&self) -> CellSpec {
        match self {
            Self::Text(t) => CellSpec {
                text: t.clone(),
                ..CellSpec::default()
            },
            Self::Spec(c) => (**c).clone(),
        }
    }

    pub fn sort_key(&self) -> SortKey {
        if let Self::Spec(c) = self
            && let Some(k) = &c.sort
        {
            return k.clone();
        }
        let text = self.text().trim();
        match text.replace(',', "").parse::<f64>() {
            Ok(n) if !text.is_empty() => SortKey::Number(n),
            _ => SortKey::Text(text.to_lowercase()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SortKey {
    Number(f64),
    Text(String),
}

impl SortKey {
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Self::Number(_), Self::Text(_)) => Ordering::Less,
            (Self::Text(_), Self::Number(_)) => Ordering::Greater,
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
        }
    }
}
