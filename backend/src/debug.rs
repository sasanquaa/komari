use opencv::core::MatTraitConst;
use opencv::core::Point;
use opencv::core::Point2d;
use opencv::core::Rect;
use opencv::core::Scalar;
use opencv::core::Size;
use opencv::core::ToInputArray;
use opencv::highgui::destroy_window;
use opencv::highgui::{imshow, wait_key};
use opencv::imgproc::arrowed_line;
use opencv::imgproc::rectangle;
use opencv::imgproc::{FONT_HERSHEY_SIMPLEX, put_text_def};
use opencv::imgproc::{LINE_8, circle_def};
use rand::distr::{Alphanumeric, SampleString};
use strum::Display;

use crate::bridge::KeyKind;
use crate::detect::SpinArrow;
use crate::solvers::SolvedArrow;
use crate::tracker::STrack;
use crate::utils::{self, DatasetDir};

#[derive(Debug, Display)]
pub enum TrackDirection {
    Right,
    Left,
    None,
}

#[derive(Debug)]
pub struct ViolettaTrack {
    pub id: u64,
    pub bbox: Rect,
    pub direction: TrackDirection,
    pub is_violetta: bool,
}

#[allow(unused)]
pub fn debug_mobs(
    mat: &impl MatTraitConst,
    minimap: Rect,
    points: Vec<Point>,
    computed_from_player: bool,
) {
    let mut mat = mat.roi(minimap).unwrap().clone_pointee();

    for point in points {
        let color = if computed_from_player {
            Scalar::new(0.0, 255.0, 0.0, 0.0)
        } else {
            Scalar::new(255.0, 0.0, 0.0, 0.0)
        };
        let _ = circle_def(&mut mat, point, 3, color);
    }

    debug_mat("Mobs", &mat, 0, []);
}

pub fn debug_spinning_arrows(mat: &impl MatTraitConst, spin_arrow: SpinArrow) {
    let last_last_arrow_head = spin_arrow.last_last_arrow_head.unwrap();
    let last_arrow_head = spin_arrow.last_arrow_head.unwrap();
    let region_centroid = spin_arrow.centroid;
    let mut mat = mat.try_clone().unwrap();

    let _ = circle_def(
        &mut mat,
        last_arrow_head + region_centroid,
        3,
        Scalar::new(255.0, 0.0, 0.0, 0.0),
    );
    let _ = circle_def(
        &mut mat,
        last_last_arrow_head + region_centroid,
        3,
        Scalar::new(0.0, 255.0, 0.0, 0.0),
    );
    let _ = circle_def(
        &mut mat,
        region_centroid,
        3,
        Scalar::new(0.0, 0.0, 255.0, 0.0),
    );

    debug_mat("Spin Arrow", &mat, 0, []);
}

pub fn debug_shape_tracks(
    mat: &impl MatTraitConst,
    tracks: Vec<STrack>,
    cursor: Point,
    bg_direction: Point2d,
) {
    fn signed_angle_deg(a: Point2d, b: Point2d) -> f64 {
        let dot = a.dot(b);
        let det = a.cross(b);
        det.atan2(dot).to_degrees()
    }

    fn mid_point(rect: Rect) -> Point {
        rect.tl() + Point::new(rect.width / 2, rect.height / 2)
    }

    let arrows = tracks
        .iter()
        .filter_map(|track| {
            if track.tracklet_len() <= 1 {
                return None;
            }

            let center = mid_point(track.rect()).to::<f64>().unwrap();
            let v = track.kalman_velocity();

            if v.norm() < 1e-3 {
                return None;
            }

            let angle = signed_angle_deg(v, bg_direction);

            let end = center + v * 5.0;

            Some((center.to::<i32>().unwrap(), end.to::<i32>().unwrap(), angle))
        })
        .collect::<Vec<_>>();

    let bboxes = tracks
        .into_iter()
        .map(|track| (track.kalman_rect(), format!("Track {}", track.track_id())))
        .collect::<Vec<_>>();

    let mut mat = mat.try_clone().unwrap();
    let arrow_start = Point::new(mat.cols() / 2, mat.rows() / 2);
    let arrow_end = Point::new(
        (arrow_start.x as f64 + bg_direction.x * 60.0) as i32,
        (arrow_start.y as f64 + bg_direction.y * 60.0) as i32,
    );

    let _ = circle_def(&mut mat, cursor, 12, Scalar::new(0.0, 0.0, 255.0, 0.0));
    let _ = arrowed_line(
        &mut mat,
        arrow_start,
        arrow_end,
        Scalar::new(255.0, 0.0, 0.0, 0.0),
        2,
        LINE_8,
        0,
        0.25,
    );

    for (arrow_start, arrow_end, angle) in arrows {
        let abs_angle = angle.abs();

        // green = aligned, yellow = diagonal, red = opposite
        let color = if abs_angle <= 25.0 {
            Scalar::new(0.0, 255.0, 0.0, 0.0)
        } else if abs_angle <= 90.0 {
            Scalar::new(0.0, 255.0, 255.0, 0.0)
        } else {
            Scalar::new(0.0, 0.0, 255.0, 0.0)
        };

        let _ = arrowed_line(&mut mat, arrow_start, arrow_end, color, 2, LINE_8, 0, 0.25);

        let label = format!("{:+.0}", angle);
        let _ = put_text_def(
            &mut mat,
            &label,
            arrow_end + Point::new(3, -3),
            FONT_HERSHEY_SIMPLEX,
            0.45,
            color,
        );
    }

    for (bbox, text) in bboxes {
        let _ = rectangle(
            &mut mat,
            bbox,
            Scalar::new(255.0, 0.0, 0.0, 0.0),
            1,
            LINE_8,
            0,
        );
        let _ = put_text_def(
            &mut mat,
            &text,
            bbox.tl() - Point::new(0, 10),
            FONT_HERSHEY_SIMPLEX,
            0.9,
            Scalar::new(0.0, 255.0, 0.0, 0.0),
        );
    }

    imshow("Shape Tracks", &mat).unwrap();
    wait_key(1).unwrap();
}

#[allow(unused)]
pub fn debug_violetta_tracks(mat: &impl MatTraitConst, tracks: Vec<ViolettaTrack>) {
    let mut mat = mat.try_clone().unwrap();
    for track in tracks {
        let color = if track.is_violetta {
            Scalar::new(0.0, 255.0, 0.0, 0.0)
        } else {
            Scalar::new(0.0, 0.0, 255.0, 0.0)
        };
        let _ = rectangle(&mut mat, track.bbox, color, 1, LINE_8, 0);
        let text = format!("{} {}", track.id, track.direction);
        let _ = put_text_def(
            &mut mat,
            &text,
            track.bbox.tl() - Point::new(0, 10),
            FONT_HERSHEY_SIMPLEX,
            0.9,
            color,
        );
    }

    imshow("Violetta Tracks", &mat).unwrap();
    wait_key(1).unwrap();
}

pub fn debug_mat(
    name: &str,
    mat: &impl MatTraitConst,
    wait_ms: i32,
    bboxes: impl AsRef<[(Rect, String)]>,
) -> i32 {
    let mut mat = mat.try_clone().unwrap();
    for (bbox, text) in bboxes.as_ref() {
        let _ = rectangle(
            &mut mat,
            *bbox,
            Scalar::new(255.0, 0.0, 0.0, 0.0),
            1,
            LINE_8,
            0,
        );
        let _ = put_text_def(
            &mut mat,
            text,
            bbox.tl() - Point::new(0, 10),
            FONT_HERSHEY_SIMPLEX,
            0.9,
            Scalar::new(0.0, 255.0, 0.0, 0.0),
        );
    }
    imshow(name, &mat).unwrap();
    let result = wait_key(wait_ms).unwrap();
    if result == 81 || wait_ms == 0 {
        destroy_window(name).unwrap();
    }
    result
}

pub fn save_rune_for_training<T: MatTraitConst + ToInputArray>(mat: &T, arrows: [SolvedArrow; 4]) {
    let has_spin_arrow = arrows.iter().any(|arrow| arrow.is_spin);
    let mut name = Alphanumeric.sample_string(&mut rand::rng(), 8);
    if has_spin_arrow {
        name = format!("{name}_spin");
    }
    let size = mat.size().unwrap();

    let labels = if has_spin_arrow {
        arrows
            .into_iter()
            .filter(|arrow| arrow.is_spin)
            .map(|arrow| to_yolo_format(0, size, arrow.bbox))
            .collect::<Vec<String>>()
            .join("\n")
    } else {
        arrows
            .into_iter()
            .map(|arrow| {
                let label = match arrow.key {
                    KeyKind::Up => 0,
                    KeyKind::Down => 1,
                    KeyKind::Left => 2,
                    KeyKind::Right => 3,
                    _ => unreachable!(),
                };
                to_yolo_format(label, size, arrow.bbox)
            })
            .collect::<Vec<String>>()
            .join("\n")
    };

    utils::save_image_to(mat, DatasetDir::Rune, format!("{name}.png"));
    utils::save_file_to(labels, DatasetDir::Rune, format!("{name}.txt"));
}

fn to_yolo_format(label: u32, size: Size, bbox: Rect) -> String {
    let x_center = bbox.x + bbox.width / 2;
    let y_center = bbox.y + bbox.height / 2;
    let x_center = x_center as f32 / size.width as f32;
    let y_center = y_center as f32 / size.height as f32;
    let width = bbox.width as f32 / size.width as f32;
    let height = bbox.height as f32 / size.height as f32;
    format!("{label} {x_center} {y_center} {width} {height}")
}
