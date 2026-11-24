use super::{
    appearance, images,
    scene::{self, Scene},
};
use gpui_kit::{Bounds, point, px, size};
use serde_json::json;
use std::cell::RefCell;
use swofty_core::view::*;
fn chart(value: serde_json::Value) -> Chart {
    serde_json::from_value(value).unwrap()
}
#[test]
fn original_marks_stay_on_the_original_renderer_path() {
    let c = chart(
        json!({"id":"legacy","series":[{"name":"line","type":"line","points":[{"x":0,"y":0},{"x":1,"y":1}]},{"name":"cells","type":"ranges","cells":[{"x":[0,1],"y":[0,1],"label":"cell"}]}]}),
    );
    let scene = Scene::new(&c);
    assert!(scene.marks.iter().all(|m| matches!(
        m,
        scene::Mark::Path { .. } | scene::Mark::Label { .. } | scene::Mark::Dot { .. }
    )));
    let mut empty = c.clone();
    empty.options = Some(Box::default());
    let other = Scene::new(&empty);
    assert_eq!(scene.marks.len(), other.marks.len());
    assert_eq!(
        appearance::Appearance::new(&c, vec![]).text(),
        ChartTextSizes::default()
    );
}
#[test]
fn styles_apply_to_bodies_and_strokes_without_joining_missing_data() {
    let c = chart(json!({"id":"styled","series":[
        {"name":"line","type":"line","line_width":5,"points":[{"x":0,"y":0},{"x":1,"y":1},null,{"x":2,"y":2},{"x":3,"y":3}]},
        {"name":"bar","type":"bars","width":0.8,"blocks":{"radius":7},"values":[{"position":0,"base":0,"value":-2}]},
        {"name":"cell","type":"ranges","cells":[{"x":[0,1],"y":[0,1],"label":"one","style":{"radius":4}}]}
    ]}));
    c.validate().unwrap();
    let scene = Scene::new(&c);
    assert_eq!(
        scene
            .marks
            .iter()
            .filter(|m| matches!(m, scene::Mark::StyledPath { width: 5.0, .. }))
            .count(),
        2
    );
    assert_eq!(
        scene
            .marks
            .iter()
            .filter(|m| matches!(m, scene::Mark::Block { .. }))
            .count(),
        2
    );
}
#[test]
fn image_fit_preserves_aspect_ratio_for_contain_and_cover() {
    let bounds = Bounds::new(point(px(0.), px(0.)), size(px(20.), px(20.)));
    let contain = images::fit_bounds(bounds, 40., 20., Fit::Contain);
    assert_eq!(contain.size, size(px(20.), px(10.)));
    assert_eq!(contain.top(), px(5.));
    let cover = images::fit_bounds(bounds, 40., 20., Fit::Cover);
    assert_eq!(cover.size, size(px(40.), px(20.)));
    assert_eq!(cover.left(), px(-10.));
}
#[test]
fn image_decoder_and_roots_match_table_fallback_behaviour() {
    let fixtures: Vec<Chart> = serde_json::from_str(include_str!(
        "../../../../../tools/native-charts/fixtures/options.json"
    ))
    .unwrap();
    let mut c = fixtures[0].clone();
    c.images.truncate(1);
    let cache = RefCell::default();
    let root = std::env::temp_dir().join("swofty-chart-options-test");
    let roots = super::super::select::ImageRoots([root.clone(), root.join("data")]);
    c.images[0].source.image = Some(CellImage::Sized {
        path: root.join("fallback.png"),
        width: Some(28.),
        height: Some(28.),
        fit: Some(Fit::Contain),
        transparent: true,
    });
    let picture = images::resolve(&c, &cache, Some(&roots));
    assert_eq!(picture[0].as_ref().unwrap().width, 20.);
    c.images[0].source.image_data.as_mut().unwrap().png = "bad PNG".into();
    let picture = images::resolve(&c, &cache, Some(&roots));
    assert_eq!(picture[0].as_ref().unwrap().width, 28.);
    c.images[0].source.image = Some(CellImage::Path(root.join("../outside.png")));
    assert!(images::resolve(&c, &cache, Some(&roots))[0].is_none());
    c.images[0].source.image = Some(CellImage::Path(std::env::temp_dir().join("outside.png")));
    assert!(images::resolve(&c, &cache, Some(&roots))[0].is_none());
}
#[test]
fn plot_images_do_not_change_the_data_domain() {
    let mut c = chart(
        json!({"id":"domain","series":[{"name":"line","type":"line","points":[{"x":0,"y":0},{"x":1,"y":1}]}]}),
    );
    c.images.push(ChartImage {
        source: ChartImageSource::default(),
        location: ChartImageLocation::Plot { x: 100., y: 100. },
    });
    let scene = Scene::new(&c);
    assert_eq!(scene.x.high, 1.);
    assert_eq!(scene.y.high, 1.);
}
