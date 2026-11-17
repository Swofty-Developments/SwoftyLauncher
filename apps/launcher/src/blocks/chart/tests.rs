use super::scene::{Mark, Scene, domains};
use swofty_core::view::*;
#[test]
fn dense_straights_collapse_but_braking_spikes_and_steps_survive() {
    let mut points: Vec<_> = (0..100_000).map(|i| [i as f32 / 99_999., 0.5]).collect();
    assert_eq!(
        super::paint::simplify_path(&points, 1000., 200.),
        vec![points[0], points[99_999]]
    );
    points[50_000][1] = 0.1;
    let simple = super::paint::simplify_path(&points, 1000., 200.);
    assert!(simple.contains(&points[50_000]));
    assert!(simple.len() < 12);
    let step = [[0., 1.], [0.5, 1.], [0.5, 0.], [1., 0.]];
    assert_eq!(super::paint::simplify_path(&step, 1000., 200.), step);
}

#[test]
fn pixel_envelope_keeps_extrema_in_sample_order_and_handles_backtracking() {
    let points = [
        [0., 0.5],
        [0.0001, 0.1],
        [0.0002, 0.9],
        [0.0003, 0.5],
        [0.8, 0.2],
        [0., 0.7],
    ];
    assert_eq!(super::paint::pixel_envelope(&points, 100.), points);
    assert!(super::paint::simplify_path(&[], 100., 100.).is_empty());
    assert_eq!(super::paint::simplify_path(&[[0., 0.]], 100., 100.), [[0., 0.]]);
}
#[test]
fn dashed_work_is_clipped_to_the_visible_plot() {
    assert_eq!(
        super::paint::clip_segment([-100., 0.5], [100., 0.5]),
        Some((0.5, 0.505))
    );
    assert_eq!(super::paint::clip_segment([-2., -1.], [2., -1.]), None);
    assert_eq!(super::paint::clip_segment([0., 0.], [1., 1.]), Some((0., 1.)));
}
fn chart(data: ChartData) -> Chart {
    Chart {
        id: "test".into(),
        title: String::new(),
        caption: String::new(),
        height: 300.,
        x: ChartAxis::default(),
        y: ChartAxis::default(),
        series: vec![ChartSeries {
            name: "driver".into(),
            color: None,
            data,
        }],
    }
}
#[test]
fn missing_points_split_paths_and_step_reaches_final_value() {
    let c = chart(ChartData::Line {
        points: vec![
            Some(ChartPoint { x: 0., y: 0. }),
            Some(ChartPoint { x: 1., y: 1. }),
            None,
            Some(ChartPoint { x: 3., y: 2. }),
            Some(ChartPoint { x: 9., y: 4. }),
        ],
        dashed: false,
        interpolation: ChartInterpolation::StepAfter,
        dots: false,
    });
    let scene = Scene::new(&c);
    let paths: Vec<_> = scene
        .marks
        .iter()
        .filter_map(|m| {
            if let Mark::Path { points, .. } = m {
                Some(points)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0].len(), 3);
    assert_eq!(*paths[1].last().unwrap(), [1., 0.]);
    // Numeric distance spacing: x=3 is one third, not the third category.
    assert!((paths[1][0][0] - 1. / 3.).abs() < 0.0001);
}
#[test]
fn bars_include_baseline_and_reversed_axis_is_respected() {
    let mut c = chart(ChartData::Bars {
        values: vec![ChartBar {
            position: 1.,
            value: 25.,
            base: 0.,
        }],
        width: 0.8,
        horizontal: true,
    });
    c.y.reversed = true;
    let (x, y) = domains(&c);
    assert_eq!(x.low, 0.);
    assert_eq!(x.high, 25.);
    assert_eq!(y.map(y.low), 1.);
}
