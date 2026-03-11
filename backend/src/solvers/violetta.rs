use std::time::Instant;

use opencv::core::{Point, Point2d, Rect};

use crate::{
    detect::Detector,
    run::FPS,
    tracker::{ByteTracker, Detection, IouGating, STrack},
};

const MUSHROOM_COUNT: usize = 4;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
enum Direction {
    Left,
    Right,
    #[default]
    None,
}

#[derive(Debug)]
pub struct ViolettaSolver {
    tracker: ByteTracker,
    numbers: Vec<Rect>,
    mushrooms: [Mushroom; MUSHROOM_COUNT],
    last_moving_time: Instant,
    is_moving: bool,
    is_initialized: bool,
    #[cfg(debug_assertions)]
    is_debugging: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct Mushroom {
    last_track_id: u64,
    last_rect: Rect,
    last_velocity: Point2d,
    last_direction: Direction,
    last_candidate_direction: Direction,
    last_candidate_direction_count: u32,
    is_violetta: bool,
}

impl Default for ViolettaSolver {
    fn default() -> Self {
        Self {
            tracker: ByteTracker::new(FPS as u64, 0.25, 0.1, 0.25, IouGating::None),
            numbers: vec![],
            mushrooms: [Mushroom::default(); MUSHROOM_COUNT],
            last_moving_time: Instant::now(),
            is_moving: false,
            is_initialized: false,
            #[cfg(debug_assertions)]
            is_debugging: false,
        }
    }
}

impl ViolettaSolver {
    #[cfg(debug_assertions)]
    #[allow(unused)]
    pub fn debug() -> Self {
        let mut solver = Self::default();
        solver.is_debugging = true;
        solver
    }

    pub fn solve(&mut self, detector: &dyn Detector, region: Rect) -> Option<Point> {
        let mushrooms = detector.detect_violetta_mushrooms(region);
        let tracks = self.tracker.update(
            mushrooms
                .into_iter()
                .map(|(bbox, score)| Detection::new(bbox, score))
                .collect(),
        );

        self.update_initial_state_if_needed(detector, region, &tracks);
        self.update_tracking(&tracks);
        self.is_moving = self
            .mushrooms
            .iter()
            .any(|mushroom| mushroom.last_direction != Direction::None);
        if self.is_moving {
            self.last_moving_time = Instant::now();
        }

        #[cfg(debug_assertions)]
        if self.is_debugging {
            use opencv::core::MatTraitConst;

            use crate::debug::debug_violetta_tracks;
            use crate::debug::{TrackDirection, ViolettaTrack};

            debug_violetta_tracks(
                &detector.mat().roi(region).unwrap(),
                self.mushrooms
                    .iter()
                    .map(|mushroom| ViolettaTrack {
                        id: mushroom.last_track_id,
                        bbox: mushroom.last_rect,
                        direction: match mushroom.last_direction {
                            Direction::Left => TrackDirection::Left,
                            Direction::Right => TrackDirection::Right,
                            Direction::None => TrackDirection::None,
                        },
                        is_violetta: mushroom.is_violetta,
                    })
                    .collect(),
            );
        }

        if !self.is_initialized || self.numbers.is_empty() || self.is_moving {
            return None;
        }

        let stationary_secs = Instant::now()
            .duration_since(self.last_moving_time)
            .as_secs();
        if stationary_secs == 0 {
            return None;
        }

        let mushroom = self
            .mushrooms
            .iter()
            .find(|mushroom| mushroom.is_violetta)
            .expect("has mushroom");
        self.numbers
            .iter()
            .find(|number| {
                let range = number.x..(number.x + number.width);
                range.contains(&x_mid(mushroom.last_rect))
            })
            .copied()
            .map(|number| mid(number) + region.tl())
    }

    fn update_initial_state_if_needed(
        &mut self,
        detector: &dyn Detector,
        region: Rect,
        tracks: &[STrack],
    ) {
        if self.numbers.is_empty() {
            self.numbers = detector.detect_violetta_numbers(region);
        }

        if self.is_initialized || tracks.len() != MUSHROOM_COUNT {
            return;
        }

        let Ok(face) = detector.detect_violetta_face(region) else {
            return;
        };

        for (index, track) in tracks.iter().enumerate() {
            let mushroom = &mut self.mushrooms[index];
            let rect = track.rect();
            let is_violetta = (rect & face).area() > 0;

            mushroom.is_violetta = is_violetta;
            update_mushroom_from_track(mushroom, track);
        }

        self.is_initialized = true;
    }

    fn update_tracking(&mut self, tracks: &[STrack]) {
        let mut tracks = tracks.iter().collect::<Vec<&STrack>>();
        let mut unprocessed_indexes = vec![];

        for (i, mushroom) in self.mushrooms.iter_mut().enumerate() {
            let Some((j, track)) = tracks
                .iter()
                .copied()
                .enumerate()
                .find(|(_, track)| track.track_id() == mushroom.last_track_id)
            else {
                unprocessed_indexes.push(i);
                continue;
            };

            tracks.remove(j);
            update_mushroom_from_track(mushroom, track);
        }

        if tracks.is_empty()
            || unprocessed_indexes.is_empty()
            || unprocessed_indexes.len() > tracks.len()
        {
            return;
        }

        let gate = tracks.len() != unprocessed_indexes.len();
        for i in unprocessed_indexes {
            let mushroom = &mut self.mushrooms[i];
            let Some((j, track)) = tracks.iter().copied().enumerate().min_by_key(|(_, track)| {
                track_score(track, mushroom.last_direction, mushroom.last_rect)
            }) else {
                return;
            };

            if gate {
                let rect = track.rect();
                let y_score = y_score(rect, mushroom.last_rect);
                if y_score >= 12 {
                    continue;
                }

                let x_score = x_score(rect, mushroom.last_direction, mushroom.last_rect) as f64;
                let threshold = mushroom.last_velocity.x * 2.5 + mushroom.last_rect.width as f64;
                if x_score >= threshold {
                    continue;
                }
            }

            tracks.remove(j);
            update_mushroom_from_track(mushroom, track);
        }
    }
}

fn update_mushroom_from_track(mushroom: &mut Mushroom, track: &STrack) {
    mushroom.last_track_id = track.track_id();
    mushroom.last_rect = track.rect();
    mushroom.last_velocity = track.kalman_velocity();

    let direction = velocity_direction(mushroom.last_velocity);
    if mushroom.last_candidate_direction == direction {
        mushroom.last_candidate_direction_count += 1;
    } else {
        mushroom.last_candidate_direction = direction;
        mushroom.last_candidate_direction_count = 0;
    }

    if mushroom.last_candidate_direction_count >= 2 {
        mushroom.last_candidate_direction_count = 0;
        mushroom.last_direction = mushroom.last_candidate_direction;
    }
}

impl Drop for ViolettaSolver {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if self.is_debugging {
            use opencv::highgui::destroy_all_windows;

            let _ = destroy_all_windows();
        }
    }
}

fn mid(rect: Rect) -> Point {
    Point::new(x_mid(rect), y_mid(rect))
}

fn x_right(rect: Rect) -> i32 {
    rect.x + rect.width
}

fn x_mid(rect: Rect) -> i32 {
    rect.x + rect.width / 2
}

fn y_mid(rect: Rect) -> i32 {
    rect.y + rect.height / 2
}

fn track_score(track: &STrack, last_direction: Direction, last_rect: Rect) -> u32 {
    let rect = track.rect();
    let x_score = x_score(rect, last_direction, last_rect);
    let y_score = y_score(rect, last_rect);

    x_score + y_score
}

fn x_score(rect: Rect, last_direction: Direction, last_rect: Rect) -> u32 {
    match last_direction {
        Direction::Left => rect.x.abs_diff(last_rect.x),
        Direction::Right => x_right(rect).abs_diff(x_right(last_rect)),
        Direction::None => x_mid(rect).abs_diff(x_mid(last_rect)),
    }
}

fn y_score(rect: Rect, last_rect: Rect) -> u32 {
    rect.y.abs_diff(last_rect.y)
}

fn velocity_direction(v: Point2d) -> Direction {
    const LEFT: Point2d = Point2d::new(-1.0, 0.0);
    const RIGHT: Point2d = Point2d::new(1.0, 0.0);
    const THRESHOLD: f64 = 0.7;
    let norm = v.norm();
    if norm < 0.8 {
        return Direction::None;
    }

    let direction = v / norm;
    if direction.dot(RIGHT) >= THRESHOLD {
        Direction::Right
    } else if direction.dot(LEFT) >= THRESHOLD {
        Direction::Left
    } else {
        Direction::None
    }
}
