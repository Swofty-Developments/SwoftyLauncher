mod blocks;
mod chart;
mod event;
mod form;
mod style;
mod table;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub use blocks::{Card, Columns, DropZone, Fit, Grid, Hero, Image, Meter, Pill, Search, TabItem, Tabs, Tile};
pub use chart::*;
pub use event::{Button, ButtonKind, ButtonSize, Control, Effect, Event, Shown};
pub use form::{
    Choice, ColourField, Field, Form, NumberField, SelectField, TextField, ToggleField, colour_hex, number,
    parse_colour,
};
pub use style::{Art, Fact, Icon, Notice, Style, Tag, Tone};
pub use table::{
    Align, Cell, CellImage, CellImageData, CellSpec, Column, ColumnSpec, RowSpec, Sort, SortKey, Span, Table,
    TableRow,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionSpec {
    pub id: String,
    pub title: String,
    pub icon: Icon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub subtitle: String,
    #[serde(default)]
    pub actions: Vec<Button>,
    #[serde(default)]
    pub notices: Vec<Notice>,
    pub body: Body,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tick: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    List {
        rows: Vec<Row>,
        reorder: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        search: Option<Search>,
    },
    Empty {
        title: String,
        text: String,
        action: Option<Button>,
    },
    Blocks(Vec<Block>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(default)]
    pub art: Art,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub trailing: Option<Control>,
    #[serde(default)]
    pub detail: Vec<Block>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu: Vec<Button>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Block {
    Stats(Vec<Fact>),
    Facts(Vec<Fact>),
    Label(String),
    Text {
        text: String,
        #[serde(default)]
        tone: Tone,
    },
    Names {
        label: String,
        items: Vec<String>,
    },
    Table(Table),
    Buttons(Vec<Button>),
    Hero(Hero),
    Pills(Vec<Pill>),
    Tiles(Vec<Tile>),
    Meters(Vec<Meter>),
    Card(Card),
    Grid(Grid),
    Form(Form),
    Tabs(Tabs),
    Image(Image),
    Chart(Chart),
    DropZone(DropZone),
}
