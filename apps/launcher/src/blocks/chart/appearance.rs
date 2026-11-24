use super::{
    color,
    images::{Picture, Pictures},
    scene::Scene,
};
use crate::ui::{scale::px, theme::Palette};
use gpui_kit::base::plot::{PlotLabel, label::Text as PlotText};
use gpui_kit::*;
use swofty_core::view::*;

pub(super) struct Appearance {
    pub(super) options: Option<Box<ChartOptions>>,
    pub(super) pictures: Pictures,
    pub(super) locations: Vec<ChartImageLocation>,
}
impl Appearance {
    pub(super) fn new(c: &Chart, pictures: Pictures) -> Self {
        Self {
            options: c.options.clone(),
            pictures,
            locations: c.images.iter().map(|i| i.location.clone()).collect(),
        }
    }
    pub(super) fn text(&self) -> ChartTextSizes {
        self.options.as_ref().map(|o| o.text.clone()).unwrap_or_default()
    }
    pub(super) fn picture(&self, location: &ChartImageLocation) -> Option<&Picture> {
        self.locations
            .iter()
            .position(|v| v == location)
            .and_then(|i| self.pictures.get(i)?.as_ref())
    }
    pub(super) fn plot(
        &self,
        scene: &Scene,
        axes: &[ChartAxis; 2],
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) -> Bounds<Pixels> {
        let text = self.text();
        let mut left = 88_f32;
        let mut top = 10_f32;
        let mut bottom = 48_f32;
        if let Some(sz) = text.y_title {
            top = top.max(sz + 6.);
        }
        let y_image = self
            .locations
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                matches!(l, ChartImageLocation::YTick { .. })
                    .then(|| self.pictures.get(i)?.as_ref())
                    .flatten()
            })
            .map(|p| p.width + 4.)
            .fold(0_f32, f32::max);
        if text.y_ticks.is_some() || y_image > 0. {
            let labels = if axes[1].ticks.is_empty() {
                (0..=4)
                    .map(|i| {
                        axes[1]
                            .format
                            .format(scene.y.low + (scene.y.high - scene.y.low) * i as f64 / 4.)
                    })
                    .collect::<Vec<_>>()
            } else {
                axes[1].ticks.iter().map(|t| t.label.clone()).collect()
            };
            for label in labels {
                left = left.max(
                    measure(&label, text.y_ticks.unwrap_or(10.), window) / crate::ui::scale::px(1.).as_f32()
                        + y_image
                        + 16.,
                );
            }
        }
        let x_image = self
            .locations
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                matches!(l, ChartImageLocation::XTick { .. })
                    .then(|| self.pictures.get(i)?.as_ref())
                    .flatten()
            })
            .map(|p| p.height)
            .fold(0_f32, f32::max);
        if text.x_ticks.is_some() || text.x_title.is_some() || x_image > 0. {
            bottom = bottom.max(
                8. + (text.x_ticks.unwrap_or(10.) * 1.4).max(x_image)
                    + text.x_title.unwrap_or(11.) * 1.4
                    + 12.,
            );
        }
        Bounds::new(
            bounds.origin + point(px(left), px(top)),
            size(
                (bounds.size.width - px(left + 18.)).max(px(1.)),
                (bounds.size.height - px(top + bottom)).max(px(1.)),
            ),
        )
    }
    pub(super) fn grid(&self, vertical: bool) -> Option<&ChartGridStyle> {
        self.options.as_ref().and_then(|o| {
            if vertical {
                o.y_grid.as_ref()
            } else {
                o.x_grid.as_ref()
            }
        })
    }
    pub(super) fn plot_images(
        &self,
        scene: &Scene,
        plot: Bounds<Pixels>,
        p: Palette,
        window: &mut Window,
        cx: &mut App,
    ) {
        for (i, location) in self.locations.iter().enumerate() {
            if let ChartImageLocation::Plot { x, y } = *location
                && let Some(Some(image)) = self.pictures.get(i)
            {
                let center = plot.origin
                    + point(
                        plot.size.width * scene.x.map(x),
                        plot.size.height * (1. - scene.y.map(y)),
                    );
                let bounds = Bounds::new(
                    center - point(px(image.width / 2.), px(image.height / 2.)),
                    size(px(image.width), px(image.height)),
                );
                if !bounds.intersect(&plot).is_empty() {
                    image.paint(bounds, p, window, cx);
                }
            }
        }
    }
}
pub(super) fn measure(text: &str, size: f32, window: &mut Window) -> f32 {
    gpui_kit::base::plot::label::measure_text_width(&text.to_owned().into(), px(size), window)
}
pub(super) struct Block<'a> {
    pub bounds: Bounds<Pixels>,
    pub style: Option<&'a ChartBlockStyle>,
    pub fallback: Hsla,
    pub label: &'a str,
    pub image: Option<&'a Picture>,
    pub label_size: f32,
}
pub(super) fn block(block: Block<'_>, p: Palette, window: &mut Window, cx: &mut App) {
    let Block {
        bounds,
        style,
        fallback,
        label,
        image,
        label_size,
    } = block;
    let radius = style.map_or(0., |s| s.radius);
    let fill_color = color(style.and_then(|s| s.fill.as_deref()), fallback);
    let border = style.and_then(|s| s.border.as_ref());
    window.paint_quad(quad(
        bounds,
        px(radius)
            .min(bounds.size.width / 2.)
            .min(bounds.size.height / 2.),
        fill_color,
        px(border.map_or(0., |b| b.width)),
        color(border.map(|b| b.color.as_str()), fill_color),
        BorderStyle::Solid,
    ));
    let text_width = measure(label, label_size, window);
    let image_width = image.map_or(0., |i| {
        px(i.width).min((bounds.size.width - px(8.)).max(px(0.))).as_f32()
    });
    let gap = if image.is_some() && !label.is_empty() {
        px(5.).as_f32()
    } else {
        0.
    };
    let fits = text_width + image_width + gap + px(8.).as_f32() <= bounds.size.width.as_f32();
    let width = if fits {
        text_width + image_width + gap
    } else {
        image_width
    };
    let x = bounds.center().x - Pixels::from(width / 2.);
    if let Some(image) = image {
        let h = px(image.height).min((bounds.size.height - px(4.)).max(px(0.)));
        if image_width > 0. && h > px(0.) {
            image.paint(
                Bounds::new(
                    point(x, bounds.center().y - h / 2.),
                    size(Pixels::from(image_width), h),
                ),
                p,
                window,
                cx,
            );
        }
    }
    if fits && !label.is_empty() {
        let text_color = if fill_color.l > 0.55 {
            rgb(0x10141a).into()
        } else {
            rgb(0xffffff).into()
        };
        let origin = point(
            x + Pixels::from(image_width + gap),
            bounds.center().y - px(label_size * 0.6),
        );
        PlotLabel::new(vec![
            PlotText::new(label.to_owned(), origin - bounds.origin, text_color).font_size(px(label_size)),
        ])
        .paint(&bounds, window, cx);
    }
}
