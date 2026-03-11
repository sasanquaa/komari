use std::{cell::RefCell, rc::Rc, sync::Arc};

use anyhow::Result;
use log::debug;
use opencv::core::{Point, Rect};
use tokio::sync::mpsc::{self, error::TryRecvError};

use crate::{
    bridge::MouseKind,
    detect::Detector,
    ecs::Resources,
    player::{
        Player, PlayerAction, PlayerEntity, next_action,
        timeout::{Lifecycle, Timeout, next_timeout_lifecycle},
    },
    solvers::TransparentShapeSolver,
    task::{Task, Update, update_detection_task},
};

#[derive(Debug)]
struct Solving {
    task: Task<()>,
    cursor_rx: mpsc::Receiver<Point>,
    detector_tx: mpsc::Sender<Arc<dyn Detector>>,
}

/// Representing the current state of transparent shape (e.g. lie detector) solving.
#[derive(Debug, Clone, Copy, Default)]
pub enum State {
    #[default]
    Waiting,
    Solving(Timeout),
    Completed,
}

#[derive(Clone, Debug, Default)]
pub struct SolvingShape {
    state: State,
    solving: Option<Rc<RefCell<Solving>>>,
    lie_detector_task: Rc<RefCell<Option<Task<Result<bool>>>>>,
}

impl Drop for SolvingShape {
    fn drop(&mut self) {
        if let Some(solving) = self.solving.as_mut() {
            solving.borrow_mut().task.abort();
        }
    }
}

/// Updates the [`Player::SolvingShape`] contextual state.
///
/// Note: This state does not use any [`Task`], so all detections are blocking. But this should be
/// acceptable for this state.
pub fn update_solving_shape_state(resources: &Resources, player: &mut PlayerEntity) {
    let Player::SolvingShape(mut solving_shape) = player.state.clone() else {
        panic!("state is not solving shape");
    };

    match solving_shape.state {
        State::Waiting => update_waiting(resources, &mut solving_shape),
        State::Solving(_) => update_solving(resources, &mut solving_shape),
        State::Completed => unreachable!(),
    }

    let player_next_state = if matches!(solving_shape.state, State::Completed) {
        Player::Idle
    } else {
        Player::SolvingShape(solving_shape)
    };

    match next_action(&player.context) {
        Some(PlayerAction::SolveShape) => {
            if matches!(player_next_state, Player::Idle) {
                player.context.clear_action_completed();
            }

            player.state = player_next_state;
        }
        Some(_) => unreachable!(),
        None => player.state = Player::Idle, // Force cancel if not from action
    }
}

fn update_waiting(resources: &Resources, solving_shape: &mut SolvingShape) {
    const CHECK_INTERVAL: u64 = 30;

    let State::Waiting = solving_shape.state else {
        panic!("solving shape state is not waiting")
    };

    if !resources.tick.is_multiple_of(CHECK_INTERVAL) {
        return;
    }
    if resources.detector().detect_lie_detector_shape_preparing() {
        return;
    }

    let title = match resources.detector().detect_lie_detector_shape() {
        Ok(val) => val,
        Err(_) => {
            solving_shape.state = State::Completed;
            return;
        }
    };

    let tl = title.tl() + Point::new(0, 20);
    let br = tl + Point::new(755, 505);
    let region = Rect::from_points(tl, br);
    solving_shape.solving = Some(Rc::new(RefCell::new(start_solving_task(region))));
    debug!(target:"backend/player","lie detector transparent shape region: {region:?}");
    solving_shape.state = State::Solving(Timeout::default());
}

fn update_solving(resources: &Resources, solving_shape: &mut SolvingShape) {
    let State::Solving(timeout) = solving_shape.state else {
        panic!("solving shape state is not solving")
    };

    // Avoids throttling the detection by using task
    let update = update_detection_task(
        resources,
        1000,
        &mut *solving_shape.lie_detector_task.borrow_mut(),
        |detector| Ok(detector.detect_lie_detector_shape().is_ok()),
    );
    if let Update::Ok(has_lie_detector) = update
        && !has_lie_detector
    {
        solving_shape.state = State::Completed;
        return;
    }

    match next_timeout_lifecycle(timeout, 545) {
        Lifecycle::Ended => {
            solving_shape.state = State::Completed;
        }
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            let mut solving = solving_shape.solving.as_mut().unwrap().borrow_mut();
            let _ = solving.detector_tx.try_send(resources.detector_cloned());
            if let Ok(cursor) = solving.cursor_rx.try_recv() {
                resources
                    .input
                    .send_mouse(cursor.x, cursor.y, MouseKind::Move);
            }
            solving_shape.state = State::Solving(timeout);
        }
    }
}

fn start_solving_task(region: Rect) -> Solving {
    let (cursor_tx, cursor_rx) = mpsc::channel(1);
    let (detector_tx, mut detector_rx) = mpsc::channel::<Arc<dyn Detector>>(2);

    let task = Task::spawn_blocking(move || {
        let mut solver = TransparentShapeSolver::default();

        loop {
            let detector = match detector_rx.try_recv() {
                Ok(detector) => detector,
                Err(err) => match err {
                    TryRecvError::Empty => continue,
                    TryRecvError::Disconnected => break,
                },
            };
            if let Some(cursor) = solver.solve(&*detector, region) {
                let _ = cursor_tx.try_send(cursor);
            }
        }
    });

    Solving {
        task,
        cursor_rx,
        detector_tx,
    }
}
