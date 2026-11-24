//! Reuse table image decoding/cache and file roots; no generated image files.
use super::super::{image_data, select::ImageRoots};
use crate::ui::scale::px;
use crate::ui::theme::Palette;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::cell::RefCell;
use swofty_core::view::*;

#[derive(Clone)]
pub(crate) struct Picture {
    source: ImageSource,
    pub(super) width: f32,
    pub(super) height: f32,
    fit: Fit,
    transparent: bool,
}
pub(crate) type Pictures = Vec<Option<Picture>>;

pub(super) fn resolve(
    c: &Chart,
    cache: &RefCell<image_data::Images>,
    roots: Option<&ImageRoots>,
) -> Pictures {
    if c.images.len() > 64 {
        return Vec::new();
    }
    c.images
        .iter()
        .map(|i| {
            if let Some(data) = &i.source.image_data
                && let Some(image) = cache.borrow_mut().get(&data.png)
            {
                let (width, height) = data.size(16.);
                return Some(Picture {
                    source: image.into(),
                    width,
                    height,
                    fit: data.fit,
                    transparent: data.transparent,
                });
            }
            let file = i.source.image.as_ref()?;
            let path = roots?.allow(file.path())?;
            let (width, height) = file.size(16.);
            Some(Picture {
                source: path.into(),
                width,
                height,
                fit: file.fit(),
                transparent: file.transparent(),
            })
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct DemoImages(RefCell<image_data::Images>);
impl DemoImages {
    pub(crate) fn pictures(&self, c: &Chart, window: &mut Window) -> Pictures {
        for image in self.0.borrow_mut().retired() {
            window.drop_image(image).ok();
        }
        resolve(c, &self.0, None)
    }
}

impl Picture {
    pub(super) fn element(&self, p: Palette) -> AnyElement {
        div()
            .w(px(self.width))
            .h(px(self.height))
            .flex_shrink_0()
            .overflow_hidden()
            .when(!self.transparent, |d| d.bg(p.elevated))
            .child(
                img(self.source.clone())
                    .w_full()
                    .h_full()
                    .object_fit(match self.fit {
                        Fit::Contain => ObjectFit::Contain,
                        Fit::Cover => ObjectFit::Cover,
                    }),
            )
            .into_any_element()
    }
    pub(super) fn paint(&self, bounds: Bounds<Pixels>, p: Palette, window: &mut Window, cx: &mut App) {
        let data = match &self.source {
            ImageSource::Render(data) => Some(data.clone()),
            ImageSource::Resource(resource) => window
                .use_asset::<ImgResourceLoader>(resource, cx)
                .and_then(Result::ok),
            _ => None,
        };
        let Some(data) = data else {
            return;
        };
        if !self.transparent {
            window.paint_quad(fill(bounds, p.elevated));
        }
        let actual = data.size(0);
        let image = fit_bounds(bounds, actual.width.0 as f32, actual.height.0 as f32, self.fit);
        window
            .paint_image(bounds, image, Corners::all(px(0.)), data, 0, false)
            .ok();
    }
}
pub(super) fn fit_bounds(bounds: Bounds<Pixels>, width: f32, height: f32, fit: Fit) -> Bounds<Pixels> {
    let sx = bounds.size.width.as_f32() / width.max(1.);
    let sy = bounds.size.height.as_f32() / height.max(1.);
    let scale = match fit {
        Fit::Contain => sx.min(sy),
        Fit::Cover => sx.max(sy),
    };
    let size = size(Pixels::from(width * scale), Pixels::from(height * scale));
    Bounds::new(bounds.center() - point(size.width / 2., size.height / 2.), size)
}
