use super::*;
use serde_json::json;
fn chart() -> Chart {
    serde_json::from_value(json!({"id":"legacy","series":[{"name":"driver","type":"line","points":[{"x":0,"y":1},null,{"x":2,"y":3}]}]})).unwrap()
}

#[test]
fn omitted_options_keep_the_exact_legacy_wire_shape() {
    let c = chart();
    assert_eq!(
        serde_json::to_value(&c).unwrap(),
        json!({
            "id":"legacy","title":"","caption":"","height":300.0,
            "x":{"label":"","domain":null,"reversed":false,"format":"number","ticks":[]},
            "y":{"label":"","domain":null,"reversed":false,"format":"number","ticks":[]},
            "series":[{"name":"driver","color":null,"type":"line","points":[{"x":0.0,"y":1.0},null,{"x":2.0,"y":3.0}],"dashed":false,"interpolation":"linear","dots":false}]
        })
    );
    assert!(c.validate().is_ok());
}
#[test]
fn all_seven_demo_options_round_trip_and_validate() {
    let charts: Vec<Chart> = serde_json::from_str(include_str!(
        "../../../../../tools/native-charts/fixtures/options.json"
    ))
    .unwrap();
    assert_eq!(charts.len(), 3);
    for c in charts {
        c.validate().unwrap();
        assert_eq!(
            c,
            serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap()
        );
    }
}
#[test]
fn sizes_colors_widths_and_dashes_are_bounded() {
    for size in [f32::NAN, f32::INFINITY, 0., 5., 65.] {
        let mut c = chart();
        c.options = Some(Box::new(ChartOptions {
            text: ChartTextSizes {
                title: Some(size),
                ..Default::default()
            },
            ..Default::default()
        }));
        assert!(c.validate().is_err());
    }
    for width in [f32::NAN, -1., 0., 0.1, 17.] {
        let mut c = chart();
        c.series[0].line_width = Some(width);
        assert!(c.validate().is_err());
    }
    for dash in [
        vec![1.],
        vec![0., 2.],
        vec![f32::NAN, 2.],
        vec![101., 2.],
        vec![1.; 18],
    ] {
        assert!(
            !ChartGridStyle {
                dash,
                ..Default::default()
            }
            .validate()
        );
    }
    assert!(
        ChartGridStyle {
            width: Some(0.),
            dash: vec![4., 2., 1., 2.],
            ..Default::default()
        }
        .validate()
    );
    assert!(
        !ChartOptions {
            background: Some("purple".into()),
            ..Default::default()
        }
        .validate()
    );
    assert!(
        !ChartBlockStyle {
            radius: 65.,
            ..Default::default()
        }
        .validate()
    );
    assert!(
        !ChartBorder {
            color: "#FFFFFF".into(),
            width: f32::INFINITY
        }
        .validate()
    );
}
#[test]
fn image_placement_checks_indices_duplicates_sizes_and_count() {
    let mut c = chart();
    let image = |location| ChartImage {
        source: ChartImageSource::default(),
        location,
    };
    c.images.push(image(ChartImageLocation::Legend { series: 1 }));
    assert!(c.validate().is_err());
    c.images = vec![image(ChartImageLocation::XTick { tick: 0 })];
    assert!(c.validate().is_err());
    c.images = vec![image(ChartImageLocation::Cell { series: 0, cell: 0 })];
    assert!(c.validate().is_err());
    c.images = vec![image(ChartImageLocation::Plot { x: f64::NAN, y: 0. })];
    assert!(c.validate().is_err());
    c.images = vec![image(ChartImageLocation::Legend { series: 0 }); 2];
    assert!(c.validate().is_err());
    c.images = vec![image(ChartImageLocation::Plot { x: 0., y: 0. }); 65];
    assert!(c.validate().is_err());
    c.images.truncate(64);
    assert!(c.validate().is_ok());
    c.images[0].source.image_data = Some(crate::view::CellImageData {
        png: "invalid PNG is a renderer fallback, not a bad view".into(),
        width: Some(0.),
        height: None,
        fit: crate::view::Fit::Contain,
        transparent: true,
    });
    assert!(c.validate().is_err());
    c.images[0].source.image_data.as_mut().unwrap().width = Some(16.);
    assert!(c.validate().is_ok());
}
#[test]
fn text_elements_can_be_sized_independently() {
    let sizes = ChartTextSizes {
        title: Some(24.),
        caption: Some(12.),
        x_title: Some(14.),
        y_title: Some(16.),
        x_ticks: Some(10.),
        y_ticks: Some(11.),
        legend: Some(13.),
        labels: Some(15.),
    };
    assert!(sizes.validate());
    let c: Chart = serde_json::from_value(json!({"id":"independent","options":{"text":sizes}})).unwrap();
    assert_eq!(c.options.unwrap().text, sizes);
}
