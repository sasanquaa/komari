use std::ops::Div;

use log::debug;
use opencv::core::{Point, Point_, Point2d, Rect};

use crate::{
    detect::Detector,
    run::FPS,
    tracker::{ByteTracker, Detection, IouGating, STrack},
};

#[derive(Debug)]
pub struct TransparentShapeSolver {
    tracker: ByteTracker,
    current_track_id: Option<u64>,
    candidate_track_id: Option<u64>,
    candidate_track_count: u32,
    last_cursor: Option<Point>,
    last_velocity: Option<Point2d>,
    bg_direction: Point2d,
    #[cfg(debug_assertions)]
    is_debugging: bool,
}

impl Default for TransparentShapeSolver {
    fn default() -> Self {
        Self {
            tracker: ByteTracker::new(FPS as u64, 0.25, 0.1, 0.25, IouGating::None),
            current_track_id: None,
            candidate_track_id: None,
            candidate_track_count: 0,
            last_cursor: None,
            last_velocity: None,
            bg_direction: Point2d::default(),
            #[cfg(debug_assertions)]
            is_debugging: false,
        }
    }
}

impl TransparentShapeSolver {
    #[cfg(debug_assertions)]
    pub fn debug() -> Self {
        let mut default = Self::default();
        default.is_debugging = true;
        default
    }

    pub fn solve(&mut self, detector: &dyn Detector, region: Rect) -> Option<Point> {
        let shapes = detector.detect_transparent_shapes(region);
        let tracks = self.tracker.update(
            shapes
                .into_iter()
                .map(|(bbox, score)| Detection::new(bbox, score))
                .collect(),
        );

        self.update_initial_track_if_needed(region, &tracks);
        self.update_background_direction(&tracks);

        match self.update_and_find_best_track(&tracks, region) {
            Some(track) => {
                let next_cursor = predicted_center(track);
                if self.current_track_id != Some(track.track_id()) {
                    debug!(target: "backend/player", "shape id switches from {:?} to {}", self.current_track_id, track.track_id());
                }
                self.current_track_id = Some(track.track_id());
                self.last_cursor = Some(next_cursor);
                self.last_velocity = Some(track.kalman_velocity());

                #[cfg(debug_assertions)]
                if self.is_debugging {
                    debug_transparent_shapes(
                        detector,
                        &tracks,
                        region,
                        next_cursor,
                        self.bg_direction,
                    );
                }

                Some(region.tl() + next_cursor)
            }
            None => {
                let last_cursor = self.last_cursor?;
                let last_velocity = self.last_velocity.expect("set if last_cursor set") * 1.5;
                let next_cursor = last_cursor
                    + Point::new(
                        last_velocity.x.round() as i32,
                        last_velocity.y.round() as i32,
                    );
                let absolute_next_cursor = region.tl() + next_cursor;
                if !region.contains(absolute_next_cursor) {
                    return None;
                }

                self.last_cursor = Some(next_cursor);

                #[cfg(debug_assertions)]
                if self.is_debugging {
                    debug_transparent_shapes(
                        detector,
                        &tracks,
                        region,
                        next_cursor,
                        self.bg_direction,
                    );
                }

                Some(absolute_next_cursor)
            }
        }
    }

    fn update_initial_track_if_needed(&mut self, region: Rect, tracks: &[STrack]) {
        if self.current_track_id.is_none() {
            let region_mid = mid_point(Rect::new(0, 0, region.width, region.height));
            if let Some(track) = find_track_closest_to(region_mid, tracks) {
                self.current_track_id = Some(track.track_id());
                self.last_cursor = Some(mid_point(track.rect()));
                self.last_velocity = Some(track.kalman_velocity());
            }
        }
    }

    fn update_background_direction(&mut self, tracks: &[STrack]) {
        if let Some(direction) = estimate_background_direction(self.last_cursor, tracks)
            .and_then(|direction| unit(self.bg_direction * 0.5 + direction * 0.5))
        {
            self.bg_direction = direction;
        }
    }

    fn update_and_find_best_track<'a>(
        &mut self,
        tracks: &'a [STrack],
        region: Rect,
    ) -> Option<&'a STrack> {
        let current_track_id = self.current_track_id?;
        let last_cursor = self.last_cursor?;
        let bg_direction = self.bg_direction;
        let match_track = tracks
            .iter()
            .filter(|track| track.track_id() == current_track_id || track.tracklet_len() >= 1)
            .filter_map(|track| {
                let score = track_background_score(track, last_cursor, bg_direction, region)?;
                Some((track, score))
            })
            .max_by(|(_, a_score), (_, b_score)| a_score.partial_cmp(b_score).unwrap())
            .map(|(track, _)| track);

        if let Some(track) = match_track {
            if track.track_id() == current_track_id {
                self.candidate_track_id = None;
                self.candidate_track_count = 0;
            }

            if self.candidate_track_id == Some(track.track_id()) {
                self.candidate_track_count += 1;
            } else {
                self.candidate_track_id = Some(track.track_id());
                self.candidate_track_count = 0;
            }

            if self.candidate_track_count >= 1 {
                self.candidate_track_id = None;
                self.candidate_track_count = 0;
                return Some(track);
            }
        }

        tracks
            .iter()
            .find(|track| track.track_id() == current_track_id)
    }
}

impl Drop for TransparentShapeSolver {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if self.is_debugging {
            use opencv::highgui::destroy_all_windows;

            let _ = destroy_all_windows();
        }
    }
}

#[cfg(debug_assertions)]
fn debug_transparent_shapes(
    detector: &dyn Detector,
    tracks: &[STrack],
    region: Rect,
    last_cursor: Point,
    bg_direction: Point2d,
) {
    use opencv::core::MatTraitConst;

    use crate::debug::debug_shape_tracks;

    debug_shape_tracks(
        &detector.mat().roi(region).unwrap(),
        tracks.to_vec(),
        last_cursor,
        bg_direction,
    );
}

fn find_track_closest_to(point: Point, tracks: &[STrack]) -> Option<&STrack> {
    tracks.iter().min_by_key(|track| {
        let track_region = track.rect();
        let track_mid =
            track_region.tl() + Point::new(track_region.width / 2, track_region.height / 2);

        (point - track_mid).norm() as i32
    })
}

fn mid_point(rect: Rect) -> Point {
    rect.tl() + Point::new(rect.width / 2, rect.height / 2)
}

fn predicted_center(track: &STrack) -> Point {
    let v = track.kalman_velocity();
    let point = mid_point(track.kalman_rect());

    Point::new(
        (point.x as f64 + v.x).round() as i32,
        (point.y as f64 + v.y).round() as i32,
    )
}

fn track_background_score(
    track: &STrack,
    last_cursor: Point,
    bg_direction: Point2d,
    region: Rect,
) -> Option<f64> {
    let angle = track_background_degree(track, bg_direction)?;
    if angle <= 45.0 {
        return None;
    }
    let score = angle / 180.0;

    let distance_penalty = if angle >= 60.0 {
        1.0
    } else {
        let cursor_dir = mid_point(track.rect()) - last_cursor;
        let cursor_squared = (cursor_dir.x.pow(2) + cursor_dir.y.pow(2)) as f64;
        let sigma = 0.25 * diag(region);
        (-cursor_squared / (2.0 * sigma.powi(2))).exp()
    };
    if distance_penalty <= 0.3 {
        return None;
    }

    Some(score * distance_penalty)
}

fn track_background_degree(track: &STrack, bg_direction: Point2d) -> Option<f64> {
    let dir = unit(track.kalman_velocity())?;
    let dot = dir.dot(bg_direction);
    let det = dir.cross(bg_direction);
    Some(det.atan2(dot).to_degrees().abs())
}

fn estimate_background_direction(last_cursor: Option<Point>, tracks: &[STrack]) -> Option<Point2d> {
    let mut last_rect_contains_cursor = None;
    let filtered = tracks
        .iter()
        .filter(|track| {
            if track.tracklet_len() < 5 {
                return false;
            }

            if last_rect_contains_cursor.is_some_and(|rect: Rect| (rect & track.rect()).area() > 0)
            {
                return false;
            }

            let Some(last_cursor) = last_cursor else {
                return true;
            };

            let rect = track.rect();
            if rect.contains(last_cursor) {
                if last_rect_contains_cursor.is_none() {
                    last_rect_contains_cursor = Some(rect);
                }

                return false;
            }

            let norm = (mid_point(track.rect()) - last_cursor).norm();
            norm >= diag(track.rect())
        })
        .map(STrack::kalman_velocity)
        .collect::<Vec<Point2d>>();
    if filtered.len() < 3 {
        return None;
    }

    let velocity_sum = filtered
        .into_iter()
        .fold(Point2d::default(), |acc, v| acc + v);
    let velocity_unit = unit(velocity_sum)?;

    Some(velocity_unit)
}

fn diag(rect: Rect) -> f64 {
    ((rect.width.pow(2) + rect.height.pow(2)) as f64).sqrt()
}

fn unit<T>(point: Point_<T>) -> Option<Point_<T>>
where
    T: Copy,
    Point_<T>: Div<f64, Output = Point_<T>>,
    f64: From<T>,
{
    let norm = point.norm();
    if norm < 1e-3 {
        return None;
    }

    Some(point / norm)
}
