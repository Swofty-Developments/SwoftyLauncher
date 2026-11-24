//! A chart laid out once into marks on its two scales.

use gpui_kit::*;
use swofty_core::view::*;

use super::{COLORS, color};

#[derive(Clone, Copy, Debug)]
pub(super) struct Scale {
    pub(super) low: f64,
    pub(super) high: f64,
    pub(super) reversed: bool,
}
impl Scale {
    pub(super) fn map(self, v: f64) -> f32 {
        let n = ((v - self.low) / (self.high - self.low)).clamp(-4096., 4097.) as f32;
        if self.reversed { 1. - n } else { n }
    }
}

#[derive(Clone)]
pub(super) enum Mark {
    StyledPath {
        points: Vec<[f32; 2]>,
        color: Hsla,
        dashed: bool,
        width: f32,
    },
    Block {
        block: Box<BlockMark>,
    },
    Path {
        points: Vec<[f32; 2]>,
        color: Hsla,
        fill: bool,
        dashed: bool,
    },
    Dot {
        at: [f32; 2],
        color: Hsla,
        hollow: bool,
    },
    Label {
        at: [f32; 2],
        text: String,
        width: f32,
        color: Hsla,
    },
}
#[derive(Clone)]
pub(super) struct BlockMark {
    pub(super) x: [f32; 2],
    pub(super) y: [f32; 2],
    pub(super) color: Hsla,
    pub(super) style: Option<Box<ChartBlockStyle>>,
    pub(super) label: String,
    pub(super) image: Option<usize>,
}
pub(crate) struct Scene {
    pub(super) x: Scale,
    pub(super) y: Scale,
    pub(super) marks: Vec<Mark>,
    pub(super) has_data: bool,
}

pub(super) fn domains(c: &Chart) -> (Scale, Scale) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut add = |x: f64, y: f64| {
        lo[0] = lo[0].min(x);
        hi[0] = hi[0].max(x);
        lo[1] = lo[1].min(y);
        hi[1] = hi[1].max(y);
    };
    for s in &c.series {
        match &s.data {
            ChartData::Line { points, .. } => {
                for p in points.iter().flatten() {
                    add(p.x, p.y);
                }
            }
            ChartData::Scatter { points, .. } => {
                for p in points {
                    add(p.x, p.y);
                }
            }
            ChartData::Band { points } => {
                for p in points.iter().flatten() {
                    add(p.x, p.low);
                    add(p.x, p.high);
                }
            }
            ChartData::Bars {
                values,
                width,
                horizontal,
            } => {
                for b in values {
                    if *horizontal {
                        add(b.base, b.position - width / 2.);
                        add(b.value, b.position + width / 2.);
                    } else {
                        add(b.position - width / 2., b.base);
                        add(b.position + width / 2., b.value);
                    }
                }
            }
            ChartData::Ranges { cells } | ChartData::Heatmap { cells } => {
                for r in cells {
                    add(r.x[0], r.y[0]);
                    add(r.x[1], r.y[1]);
                }
            }
            ChartData::Boxes { values, width } => {
                for b in values {
                    add(b.position - width / 2., b.min);
                    add(b.position + width / 2., b.max);
                }
            }
        }
    }
    let scales = std::array::from_fn::<_, 2, _>(|i| {
        let axis = if i == 0 { &c.x } else { &c.y };
        let [mut a, mut b] = axis.domain.unwrap_or([lo[i], hi[i]]);
        if !a.is_finite() || !b.is_finite() {
            a = 0.;
            b = 1.;
        }
        if b - a < 1e-9 {
            let d = (a.abs() * 0.05).max(1.);
            a -= d;
            b += d;
        }
        Scale {
            low: a,
            high: b,
            reversed: axis.reversed,
        }
    });
    (scales[0], scales[1])
}

impl Scene {
    pub(super) fn new(c: &Chart) -> Self {
        let (x, y) = domains(c);
        let mut scene = Self {
            x,
            y,
            marks: Vec::new(),
            has_data: false,
        };
        for (i, s) in c.series.iter().enumerate() {
            let begin = scene.marks.len();
            let col = color(s.color.as_deref(), rgb(COLORS[i % COLORS.len()]).into());
            match &s.data {
                ChartData::Line {
                    points,
                    dashed,
                    interpolation,
                    dots,
                } => {
                    let mut run = Vec::new();
                    for p in points.iter().chain(std::iter::once(&None)) {
                        if let Some(p) = p {
                            let at = scene.at(p.x, p.y);
                            if *interpolation == ChartInterpolation::StepAfter
                                && let Some(last) = run.last()
                            {
                                let last: &[f32; 2] = last;
                                run.push([at[0], last[1]]);
                            }
                            run.push(at);
                            if *dots {
                                scene.marks.push(Mark::Dot {
                                    at,
                                    color: col,
                                    hollow: false,
                                });
                            }
                            scene.has_data = true;
                        } else if !run.is_empty() {
                            if run.len() == 1 {
                                scene.marks.push(Mark::Dot {
                                    at: run[0],
                                    color: col,
                                    hollow: false,
                                });
                            } else {
                                scene.marks.push(Mark::Path {
                                    points: std::mem::take(&mut run),
                                    color: col,
                                    fill: false,
                                    dashed: *dashed,
                                });
                            }
                            run.clear();
                        }
                    }
                }
                ChartData::Scatter { points, hollow } => {
                    for p in points {
                        scene.has_data = true;
                        scene.marks.push(Mark::Dot {
                            at: scene.at(p.x, p.y),
                            color: col,
                            hollow: *hollow,
                        });
                    }
                }
                ChartData::Band { points } => {
                    let mut run: Vec<&ChartBandPoint> = Vec::new();
                    for p in points.iter().chain(std::iter::once(&None)) {
                        if let Some(p) = p {
                            run.push(p);
                            scene.has_data = true;
                        } else if !run.is_empty() {
                            let pts = run
                                .iter()
                                .map(|p| scene.at(p.x, p.high))
                                .chain(run.iter().rev().map(|p| scene.at(p.x, p.low)))
                                .collect();
                            scene.marks.push(Mark::Path {
                                points: pts,
                                color: col,
                                fill: true,
                                dashed: false,
                            });
                            run.clear();
                        }
                    }
                }
                ChartData::Bars {
                    values,
                    width,
                    horizontal,
                } => {
                    for b in values {
                        if *horizontal {
                            scene.block(
                                [b.base, b.value],
                                [b.position - width / 2., b.position + width / 2.],
                                col,
                                s.blocks.as_deref(),
                                "",
                                None,
                            );
                        } else {
                            scene.block(
                                [b.position - width / 2., b.position + width / 2.],
                                [b.base, b.value],
                                col,
                                s.blocks.as_deref(),
                                "",
                                None,
                            );
                        }
                    }
                }
                ChartData::Ranges { cells } | ChartData::Heatmap { cells } => {
                    for (cell, r) in cells.iter().enumerate() {
                        let style = r.style.as_deref().or(s.blocks.as_deref());
                        let image=c.images.iter().position(|image| matches!(image.location,ChartImageLocation::Cell {series,cell:slot} if series==i && slot==cell));
                        if style.is_some() || image.is_some() {
                            scene.block(r.x, r.y, color(r.color.as_deref(), col), style, &r.label, image);
                            continue;
                        }
                        scene.rect(r.x, r.y, color(r.color.as_deref(), col));
                        if !r.label.is_empty() {
                            scene.marks.push(Mark::Label {
                                at: scene.at((r.x[0] + r.x[1]) / 2., (r.y[0] + r.y[1]) / 2.),
                                text: r.label.clone(),
                                width: (scene.x.map(r.x[1]) - scene.x.map(r.x[0])).abs(),
                                color: {
                                    let c = color(r.color.as_deref(), col);
                                    if c.l > 0.55 {
                                        rgb(0x10141a).into()
                                    } else {
                                        rgb(0xffffff).into()
                                    }
                                },
                            });
                        }
                    }
                }
                ChartData::Boxes { values, width } => {
                    for b in values {
                        scene.block(
                            [b.position - width / 2., b.position + width / 2.],
                            [b.q1, b.q3],
                            col,
                            s.blocks.as_deref(),
                            "",
                            None,
                        );
                        for (a, z) in [
                            ((b.position, b.min), (b.position, b.q1)),
                            ((b.position, b.q3), (b.position, b.max)),
                            ((b.position - width / 4., b.min), (b.position + width / 4., b.min)),
                            ((b.position - width / 4., b.max), (b.position + width / 4., b.max)),
                        ] {
                            scene.line(a, z, col);
                        }
                        scene.line(
                            (b.position - width / 2., b.median),
                            (b.position + width / 2., b.median),
                            rgb(0x10141a).into(),
                        );
                    }
                }
            }
            if let Some(width) = s.line_width {
                for mark in &mut scene.marks[begin..] {
                    if let Mark::Path {
                        points,
                        color,
                        fill: false,
                        dashed,
                    } = mark
                    {
                        *mark = Mark::StyledPath {
                            points: std::mem::take(points),
                            color: *color,
                            dashed: *dashed,
                            width,
                        };
                    }
                }
            }
        }
        scene.has_data |= c
            .images
            .iter()
            .any(|i| matches!(i.location, ChartImageLocation::Plot { .. }));
        scene
    }
    fn block(
        &mut self,
        x: [f64; 2],
        y: [f64; 2],
        color: Hsla,
        style: Option<&ChartBlockStyle>,
        label: &str,
        image: Option<usize>,
    ) {
        if style.is_none() && image.is_none() {
            self.rect(x, y, color);
            return;
        }
        self.has_data = true;
        self.marks.push(Mark::Block {
            block: Box::new(BlockMark {
                x: [self.x.map(x[0]), self.x.map(x[1])],
                y: [1. - self.y.map(y[0]), 1. - self.y.map(y[1])],
                color,
                style: style.cloned().map(Box::new),
                label: label.into(),
                image,
            }),
        });
    }
    fn at(&self, x: f64, y: f64) -> [f32; 2] {
        [self.x.map(x), 1. - self.y.map(y)]
    }
    fn line(&mut self, a: (f64, f64), b: (f64, f64), color: Hsla) {
        self.marks.push(Mark::Path {
            points: vec![self.at(a.0, a.1), self.at(b.0, b.1)],
            color,
            fill: false,
            dashed: false,
        });
    }
    fn rect(&mut self, x: [f64; 2], y: [f64; 2], color: Hsla) {
        self.has_data = true;
        self.marks.push(Mark::Path {
            points: vec![
                self.at(x[0], y[0]),
                self.at(x[1], y[0]),
                self.at(x[1], y[1]),
                self.at(x[0], y[1]),
            ],
            color,
            fill: true,
            dashed: false,
        });
    }
}
