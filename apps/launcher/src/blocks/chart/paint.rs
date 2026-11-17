//! Painting a laid-out chart: its paths and dashes, thinned to what a pixel column can show.

use gpui_kit::base::plot::{PlotLabel, label::Text as PlotText};
use gpui_kit::*;
use swofty_core::view::*;

use crate::ui::scale::px;
use crate::ui::theme::Palette;

use super::scene::{Mark, Scene};

pub(super) fn paint(
    scene: &Scene,
    axes: &[ChartAxis; 2],
    bounds: Bounds<Pixels>,
    p: Palette,
    window: &mut Window,
    cx: &mut App,
) {
    // Scroll containers still paint canvas children outside their viewport. Reject
    // them before allocating projected points or tessellating any native paths.
    if bounds.intersect(&window.content_mask().bounds).is_empty() {
        return;
    }
    if std::env::var_os("SWOFTY_CHART_METRICS").is_some() {
        static PAINTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if PAINTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 16 {
            eprintln!(
                "native chart painted: {} marks, {} x {} logical pixels",
                scene.marks.len(),
                bounds.size.width.as_f32(),
                bounds.size.height.as_f32()
            );
        }
    }
    let gutter = px(88.);
    let top = px(10.);
    let bottom = px(48.);
    let plot = Bounds::new(
        bounds.origin + point(gutter, top),
        size(
            (bounds.size.width - gutter - px(18.)).max(px(1.)),
            (bounds.size.height - top - bottom).max(px(1.)),
        ),
    );
    let at = |v: [f32; 2]| plot.origin + point(plot.size.width * v[0], plot.size.height * v[1]);
    let mut labels = Vec::new();
    for (axis, scale, vertical) in [(&axes[0], scene.x, false), (&axes[1], scene.y, true)] {
        let ticks = if axis.ticks.is_empty() {
            (0..=4)
                .map(|i| {
                    let v = scale.low + (scale.high - scale.low) * f64::from(i) / 4.;
                    ChartTick {
                        value: v,
                        label: axis.format.format(v),
                    }
                })
                .collect()
        } else {
            axis.ticks.clone()
        };
        let stride = if vertical {
            (ticks.len() as f32 / (plot.size.height.as_f32() / px(20.).as_f32()).max(1.)).ceil() as usize
        } else {
            (ticks.len() as f32 / (plot.size.width.as_f32() / px(65.).as_f32()).max(1.)).ceil() as usize
        }
        .max(1);
        for t in ticks.iter().step_by(stride) {
            let n = scale.map(t.value);
            if !(0. ..=1.).contains(&n) {
                continue;
            }
            let (a, b, origin, align) = if vertical {
                (
                    [0., 1. - n],
                    [1., 1. - n],
                    point(gutter - px(8.), top + plot.size.height * (1. - n) - px(6.)),
                    TextAlign::Right,
                )
            } else {
                (
                    [n, 0.],
                    [n, 1.],
                    point(gutter + plot.size.width * n, top + plot.size.height + px(8.)),
                    TextAlign::Center,
                )
            };
            path(&[at(a), at(b)], p.border_variant, false, window);
            labels.push(
                PlotText::new(t.label.clone(), origin, p.text_muted)
                    .font_size(px(10.))
                    .align(align),
            );
        }
    }
    labels.push(
        PlotText::new(
            axes[0].label.clone(),
            point(gutter + plot.size.width / 2., bounds.size.height - px(16.)),
            p.text_muted,
        )
        .font_size(px(11.))
        .align(TextAlign::Center),
    );
    labels
        .push(PlotText::new(axes[1].label.clone(), point(gutter, px(-3.)), p.text_muted).font_size(px(10.)));
    window.with_content_mask(Some(ContentMask { bounds: plot }), |window| {
        for mark in &scene.marks {
            match mark {
                Mark::Path {
                    points,
                    color,
                    fill,
                    dashed,
                } => {
                    let points = if *fill {
                        points.clone()
                    } else {
                        simplify_path(points, plot.size.width.as_f32(), plot.size.height.as_f32())
                    };
                    let points: Vec<_> = points.into_iter().map(at).collect();
                    if *dashed {
                        dashed_path(&points, *color, plot, window);
                    } else {
                        path(&points, *color, *fill, window);
                    }
                }
                Mark::Dot { at: v, color, hollow } => {
                    let center = at(*v);
                    let radius = px(3.);
                    let points: Vec<_> = (0..=16)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::TAU / 16.;
                            center + point(radius * a.cos(), radius * a.sin())
                        })
                        .collect();
                    path(&points, *color, !hollow, window);
                }
                Mark::Label {
                    at: v,
                    text,
                    width,
                    color,
                } => {
                    let origin = at(*v) - bounds.origin - point(px(0.), px(6.));
                    let text_width = gpui_kit::base::plot::label::measure_text_width(
                        &text.clone().into(),
                        px(10.),
                        window,
                    );
                    if text_width + px(8.).as_f32() > width * plot.size.width.as_f32() {
                        continue;
                    }
                    PlotLabel::new(vec![
                        PlotText::new(text.clone(), origin, *color)
                            .font_size(px(10.))
                            .align(TextAlign::Center),
                    ])
                    .paint(&bounds, window, cx);
                }
            }
        }
    });
    PlotLabel::new(labels).paint(&bounds, window, cx);
}

pub(super) fn path(points: &[Point<Pixels>], color: Hsla, fill: bool, window: &mut Window) {
    if points.len() < 2 {
        return;
    }
    if !fill && points.len() > 4096 {
        // Lyon's GPUI adapter uses u16 vertex indices. Bound each stroke even
        // when an unusually noisy series cannot be simplified further.
        for start in (0..points.len() - 1).step_by(4095) {
            path(
                &points[start..points.len().min(start + 4096)],
                color,
                false,
                window,
            );
        }
        return;
    }
    let mut builder = if fill {
        PathBuilder::fill()
    } else {
        PathBuilder::stroke(px(1.6))
    };
    builder.move_to(points[0]);
    for p in &points[1..] {
        builder.line_to(*p);
    }
    if fill {
        builder.close();
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Fixed screen-space dashes, continuous through corners and irregular samples.
pub(super) fn dashed_path(
    points: &[Point<Pixels>],
    color: Hsla,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let mut builder = PathBuilder::stroke(px(1.6));
    let mut segments = 0;
    let mut phase = 0.;
    let dash = px(7.).as_f32();
    let period = dash + px(5.).as_f32();
    for pair in points.windows(2) {
        let delta = pair[1] - pair[0];
        let len = delta.x.as_f32().hypot(delta.y.as_f32());
        if !len.is_finite() || len == 0. {
            continue;
        }
        let a = [
            (pair[0].x - bounds.origin.x).as_f32() / bounds.size.width.as_f32(),
            (pair[0].y - bounds.origin.y).as_f32() / bounds.size.height.as_f32(),
        ];
        let b = [
            (pair[1].x - bounds.origin.x).as_f32() / bounds.size.width.as_f32(),
            (pair[1].y - bounds.origin.y).as_f32() / bounds.size.height.as_f32(),
        ];
        let Some((enter, leave)) = clip_segment(a, b) else {
            phase = (phase + len) % period;
            continue;
        };
        let previous_phase = phase;
        phase = (phase + enter * len) % period;
        let visible_start = pair[0] + delta * enter;
        let unit = delta * (1. / len);
        let mut done = 0.;
        let end = (leave - enter) * len;
        while done < end {
            let step = (if phase < dash {
                dash - phase
            } else {
                period - phase
            })
            .min(end - done)
            .max(0.001);
            if phase < dash {
                // All dash segments share one native path, rather than one
                // scene allocation and GPU draw entry for every tiny segment.
                builder.move_to(visible_start + unit * done);
                builder.line_to(visible_start + unit * (done + step));
                segments += 1;
                if segments == 1024 {
                    let batch = std::mem::replace(&mut builder, PathBuilder::stroke(px(1.6)));
                    if let Ok(path) = batch.build() {
                        window.paint_path(path, color);
                    }
                    segments = 0;
                }
            }
            done += step;
            phase = (phase + step) % period;
        }
        phase = (previous_phase + len) % period;
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Preserve the first, last, minimum and maximum in each consecutive pixel
/// column. No smoothing: short braking spikes and vertical gear steps survive.
/// Runs are already separated at missing samples, and non-monotonic x is safe.
pub(super) fn simplify_path(points: &[[f32; 2]], width: f32, height: f32) -> Vec<[f32; 2]> {
    let points = pixel_envelope(points, width);
    if points.len() < 3 {
        return points;
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut pending = vec![(0, points.len() - 1)];
    let mut budget = points.len().saturating_mul(32);
    while let Some((start, end)) = pending.pop() {
        // Adversarial zigzags must not turn simplification into quadratic work.
        // Keeping the remaining samples preserves accuracy when the budget ends.
        let work = end - start;
        if work > budget {
            keep[start..=end].fill(true);
            continue;
        }
        budget -= work;
        let a = points[start];
        let b = points[end];
        let dx = (b[0] - a[0]) * width;
        let dy = (b[1] - a[1]) * height;
        let length2 = dx * dx + dy * dy;
        let mut farthest = None;
        let mut max_error = 0.35_f32.powi(2);
        for (i, p) in points.iter().enumerate().take(end).skip(start + 1) {
            let x = (p[0] - a[0]) * width;
            let y = (p[1] - a[1]) * height;
            let t = if length2 == 0. {
                0.
            } else {
                ((x * dx + y * dy) / length2).clamp(0., 1.)
            };
            let error = (x - t * dx).powi(2) + (y - t * dy).powi(2);
            if error > max_error {
                max_error = error;
                farthest = Some(i);
            }
        }
        if let Some(i) = farthest {
            keep[i] = true;
            pending.push((start, i));
            pending.push((i, end));
        }
    }
    points
        .into_iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(p))
        .collect()
}

pub(super) fn pixel_envelope(points: &[[f32; 2]], width: f32) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(points.len().min((width.max(1.) as usize + 2) * 4));
    let mut start = 0;
    while start < points.len() {
        let column = (points[start][0] * width).floor();
        let mut end = start + 1;
        let mut lo = start;
        let mut hi = start;
        while end < points.len() && (points[end][0] * width).floor() == column {
            if points[end][1] < points[lo][1] {
                lo = end;
            }
            if points[end][1] > points[hi][1] {
                hi = end;
            }
            end += 1;
        }
        let mut indices = [start, lo, hi, end - 1];
        indices.sort_unstable();
        let mut previous = usize::MAX;
        for i in indices {
            if i != previous && out.last() != Some(&points[i]) {
                out.push(points[i]);
            }
            previous = i;
        }
        start = end;
    }
    out
}

/// Liang–Barsky clipping keeps dashed work proportional to visible geometry.
pub(super) fn clip_segment(a: [f32; 2], b: [f32; 2]) -> Option<(f32, f32)> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let mut enter = 0_f32;
    let mut leave = 1_f32;
    for (p, q) in [(-d[0], a[0]), (d[0], 1. - a[0]), (-d[1], a[1]), (d[1], 1. - a[1])] {
        if p == 0. {
            if q < 0. {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0. {
                enter = enter.max(r);
            } else {
                leave = leave.min(r);
            }
            if enter > leave {
                return None;
            }
        }
    }
    Some((enter, leave))
}
