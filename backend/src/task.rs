use std::{
    fmt::{self, Debug, Formatter},
    sync::Arc,
    time::Duration,
};

use anyhow::{Error, Result, anyhow};
use tokio::{
    spawn,
    sync::oneshot::{self, Receiver},
    task::{JoinHandle, spawn_blocking},
    time::sleep,
};

use crate::{detect::Detector, ecs::Resources};

/// An asynchronous task.
///
/// This is a simple wrapper around [`tokio::task::spawn`] and [`tokio::sync::oneshot`] mainly
/// for using inside synchronous code to do blocking or expensive operation.
#[derive(Debug)]
pub struct Task<T> {
    rx: Receiver<T>,
    handle: JoinHandle<()>,
}

impl<T: Debug> Task<T> {
    pub fn spawn<F>(f: F) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let handle = spawn(async move {
            let _ = tx.send(f.await);
        });
        Task { rx, handle }
    }

    pub fn spawn_blocking<F>(f: F) -> Task<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let handle = spawn_blocking(move || {
            let _ = tx.send(f());
        });
        Task { rx, handle }
    }
}

impl<T> Task<T> {
    pub fn completed(&self) -> bool {
        self.rx.is_terminated()
    }

    pub fn abort(&mut self) {
        self.rx.close();
        self.handle.abort();
    }

    fn poll_inner(&mut self) -> Option<T> {
        if self.rx.is_terminated() {
            return None;
        }

        self.rx.try_recv().ok()
    }
}

#[derive(Debug)]
struct DelayComplete;

impl fmt::Display for DelayComplete {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DelayComplete")
    }
}

impl std::error::Error for DelayComplete {}

#[derive(Debug)]
pub enum Update<T> {
    Ok(T),
    Err(#[allow(unused)] Error),
    Pending,
}

#[inline]
pub fn update_task<F, T, A>(
    repeat_delay_millis: u64,
    task: &mut Option<Task<Result<T>>>,
    task_fn_args: impl FnOnce() -> A,
    task_fn: F,
) -> Update<T>
where
    F: FnOnce(A) -> Result<T> + Send + 'static,
    T: Debug + Send + 'static,
    A: Send + 'static,
{
    let update = match task.as_mut().and_then(|task| task.poll_inner()) {
        Some(Ok(value)) => Update::Ok(value),
        Some(Err(err)) => {
            if err.downcast_ref::<DelayComplete>().is_some() {
                *task = None;

                Update::Pending
            } else {
                Update::Err(err)
            }
        }
        None => Update::Pending,
    };

    if matches!(update, Update::Pending) && task.as_ref().is_none_or(|task| task.completed()) {
        let should_delay = task.as_ref().is_some_and(|task| task.completed());
        let spawned = if should_delay && repeat_delay_millis > 0 {
            Task::spawn(async move {
                sleep(Duration::from_millis(repeat_delay_millis)).await;

                Err(anyhow!(DelayComplete))
            })
        } else {
            let args = task_fn_args();
            let fut = spawn_blocking(move || task_fn(args));

            Task::spawn(async move { fut.await.unwrap() })
        };

        *task = Some(spawned);
    }

    update
}

#[inline]
pub fn update_detection_task<F, T>(
    resources: &Resources,
    repeat_delay_millis: u64,
    task: &mut Option<Task<Result<T>>>,
    task_fn: F,
) -> Update<T>
where
    F: FnOnce(Arc<dyn Detector>) -> Result<T> + Send + 'static,
    T: Debug + Send + 'static,
{
    update_task(
        repeat_delay_millis,
        task,
        || resources.detector_cloned(),
        task_fn,
    )
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use anyhow::Result;
    use tokio::task::yield_now;

    use crate::task::{Task, Update, update_task};

    #[tokio::test(start_paused = true)]
    async fn spawn_state() {
        let mut task = Task::spawn(async move { 0 });
        assert!(!task.completed());

        while !task.completed() {
            match task.poll_inner() {
                Some(value) => assert_eq!(value, 0),
                None => yield_now().await,
            }
        }
        assert_matches!(task.poll_inner(), None);
        assert!(task.completed());
    }

    #[tokio::test(start_paused = true)]
    async fn update_task_repeatable_state() {
        let mut task = None::<Task<Result<u32>>>;
        assert!(task.is_none());

        assert_matches!(
            update_task(1000, &mut task, || (), |_| Ok(0)),
            Update::Pending
        );
        assert!(task.is_some());

        while !task.as_ref().unwrap().completed() {
            match update_task(1000, &mut task, || (), |_| Ok(0)) {
                Update::Ok(value) => assert!(value == 0),
                Update::Pending => yield_now().await,
                Update::Err(_) => unreachable!(),
            }
        }
        assert_matches!(task.as_mut().unwrap().poll_inner(), None);
        assert!(task.as_ref().unwrap().completed());

        assert_matches!(
            update_task(1000, &mut task, || (), |_| Ok(0)),
            Update::Pending
        );
        assert!(!task.as_ref().unwrap().completed());
    }
}
