use std::{
    cell::RefCell,
    mem,
    ops::{Index, Not},
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Error, bail};
use bit_vec::BitVec;
use log::{debug, error};
use opencv::{
    core::{ToInputArray, Vector, VectorToVec},
    imgcodecs::imencode_def,
};
use reqwest::{
    Client, Url,
    multipart::{Form, Part},
};
use serde_json::{Value, json};
use tokio::{
    spawn,
    time::{Instant, sleep},
};

use crate::{Settings, WebhookProvider};

static TRUE: bool = true;
static FALSE: bool = false;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[repr(usize)]
pub enum NotificationKind {
    FailOrMapChange,
    RuneAppear,
    EliteBossAppear,
    PlayerGuildieAppear,
    PlayerStrangerAppear,
    PlayerFriendAppear,
    PlayerIsDead,
    LieDetectorShapeAppear,
    LieDetectorViolettaAppear,
    RunTimerEnded,
}

impl NotificationKind {
    fn enabled(&self, settings: &Settings) -> bool {
        match self {
            NotificationKind::FailOrMapChange => {
                settings.notifications.notify_on_fail_or_change_map
            }
            NotificationKind::RuneAppear => settings.notifications.notify_on_rune_appear,
            NotificationKind::EliteBossAppear => settings.notifications.notify_on_elite_boss_appear,
            NotificationKind::PlayerIsDead => settings.notifications.notify_on_player_die,
            NotificationKind::PlayerGuildieAppear => {
                settings.notifications.notify_on_player_guildie_appear
            }
            NotificationKind::PlayerStrangerAppear => {
                settings.notifications.notify_on_player_stranger_appear
            }
            NotificationKind::PlayerFriendAppear => {
                settings.notifications.notify_on_player_friend_appear
            }
            NotificationKind::LieDetectorViolettaAppear
            | NotificationKind::LieDetectorShapeAppear => {
                settings.notifications.notify_on_lie_detector_appear
            }
            NotificationKind::RunTimerEnded => settings.notifications.notify_on_run_timer_end,
        }
    }

    fn content(&self, settings: &Settings) -> String {
        let user_id = settings
            .notifications
            .discord_user_id
            .is_empty()
            .not()
            .then_some(format!("<@{}> ", settings.notifications.discord_user_id))
            .unwrap_or_default();

        match self {
            NotificationKind::FailOrMapChange => {
                if settings.stop_on_fail_or_change_map {
                    format!(
                        "{user_id}Bot stopped because it has failed to detect or the map has changed"
                    )
                } else {
                    format!("{user_id}Bot has failed to detect or the map has changed")
                }
            }
            NotificationKind::RuneAppear => {
                format!("{user_id}Bot has detected a rune on map")
            }
            NotificationKind::EliteBossAppear => {
                format!("{user_id}Elite boss spawned")
            }
            NotificationKind::PlayerIsDead => {
                format!("{user_id}The player is dead")
            }
            NotificationKind::PlayerGuildieAppear => {
                format!("{user_id}Bot has detected guildie player(s)")
            }
            NotificationKind::PlayerStrangerAppear => {
                format!("{user_id}Bot has detected stranger player(s)")
            }
            NotificationKind::PlayerFriendAppear => {
                format!("{user_id}Bot has detected friend player(s)")
            }
            NotificationKind::LieDetectorViolettaAppear
            | NotificationKind::LieDetectorShapeAppear => {
                format!("{user_id}Bot has detected the lie detector")
            }
            NotificationKind::RunTimerEnded => {
                format!("{user_id}Bot run timer has ended.")
            }
        }
    }

    fn scheduled_frames(&self) -> Vec<ScheduledFrame> {
        match self {
            NotificationKind::FailOrMapChange => vec![
                ScheduledFrame::new_deadline(2),
                ScheduledFrame::new_deadline(4),
            ],
            NotificationKind::RunTimerEnded
            | NotificationKind::EliteBossAppear
            | NotificationKind::PlayerIsDead
            | NotificationKind::PlayerGuildieAppear
            | NotificationKind::PlayerStrangerAppear
            | NotificationKind::PlayerFriendAppear => vec![ScheduledFrame::new_deadline(2)],
            NotificationKind::RuneAppear
            | NotificationKind::LieDetectorShapeAppear
            | NotificationKind::LieDetectorViolettaAppear => {
                vec![ScheduledFrame::new_deadline(1)]
            }
        }
    }

    fn schedule_delay_duration(&self) -> Duration {
        let secs = match self {
            NotificationKind::FailOrMapChange => 5,
            NotificationKind::RunTimerEnded
            | NotificationKind::EliteBossAppear
            | NotificationKind::PlayerIsDead
            | NotificationKind::PlayerGuildieAppear
            | NotificationKind::PlayerStrangerAppear
            | NotificationKind::PlayerFriendAppear
            | NotificationKind::RuneAppear => 3,
            NotificationKind::LieDetectorShapeAppear
            | NotificationKind::LieDetectorViolettaAppear => 2,
        };

        Duration::from_secs(secs)
    }
}

impl From<NotificationKind> for usize {
    fn from(kind: NotificationKind) -> Self {
        kind as usize
    }
}

impl Index<NotificationKind> for BitVec {
    type Output = bool;

    fn index(&self, index: NotificationKind) -> &Self::Output {
        if self.get(index.into()).expect("index out of bound") {
            &TRUE
        } else {
            &FALSE
        }
    }
}

/// A notification scheduled to be sending.
#[derive(Debug)]
struct ScheduledNotification {
    /// The instant it was scheduled.
    instant: Instant,
    /// The kind of notification.
    kind: NotificationKind,
    /// The webhook url.
    url: Url,
    /// The provider of the webhook service.
    provider: WebhookProvider,
    /// The content of the message.
    content: String,
    /// The username of the message's owner.
    username: &'static str,
    /// Stores fixed size tuples of frame and frame deadline in seconds.
    ///
    /// During each [`Notification::update_schedule`], the first frame not passing the
    /// deadline will try to capture the image from current game state. This is useful for showing
    /// `before and after` when map changes. So frame that cannot capture when the deadline is
    /// reached will be skipped.
    frames: Vec<ScheduledFrame>,
}

#[derive(Debug)]
struct ScheduledFrame {
    inner: Option<Vec<u8>>,
    deadline_secs: u32,
}

impl ScheduledFrame {
    fn new_deadline(deadline_secs: u32) -> Self {
        Self {
            inner: None,
            deadline_secs,
        }
    }
}

#[derive(Debug)]
pub struct Notification {
    client: Client,
    /// A reference to [`Settings`] for checking if a notification is enabled.
    settings: Rc<RefCell<Settings>>,
    /// Stores pending notifications.
    scheduled: Arc<Mutex<Vec<ScheduledNotification>>>,
    /// Storing currently incomplete / pending notification kinds.
    ///
    /// There can only be one unique [`NotificationKind`] scheduled at a time.
    pending: Arc<Mutex<BitVec>>,
}

impl Notification {
    pub fn new(settings: Rc<RefCell<Settings>>) -> Self {
        Self {
            client: Client::new(),
            settings,
            scheduled: Arc::new(Mutex::new(vec![])),
            pending: Arc::new(Mutex::new(BitVec::from_elem(
                mem::variant_count::<NotificationKind>(),
                false,
            ))),
        }
    }

    pub fn schedule_notification(&self, kind: NotificationKind) {
        let _ = self.schedule_notification_inner(kind);
    }

    fn schedule_notification_inner(&self, kind: NotificationKind) -> Result<(), Error> {
        let settings = self.settings.borrow();
        if !kind.enabled(&settings) {
            bail!("notification not enabled");
        }

        {
            let mut pending = self.pending.lock().unwrap();
            if pending[kind] {
                bail!("notification is already sending");
            }

            let provider = settings.notifications.webhook_provider;
            let Ok(url) = Url::try_from(settings.notifications.webhook_url.as_str()) else {
                bail!("failed to parse webhook url");
            };

            let content = kind.content(&settings);
            let frames = kind.scheduled_frames();
            let mut scheduled = self.scheduled.lock().unwrap();
            scheduled.push(ScheduledNotification {
                instant: Instant::now(),
                kind,
                url,
                provider,
                content,
                username: "Komari",
                frames,
            });
            pending.set(kind.into(), true);
        }

        let client = self.client.clone();
        let delay = kind.schedule_delay_duration();
        let pending = self.pending.clone();
        let scheduled = self.scheduled.clone();
        spawn(async move {
            sleep(delay).await;

            let notification = {
                let mut scheduled = scheduled.lock().unwrap();
                let (index, _) = scheduled
                    .iter()
                    .enumerate()
                    .find(|(_, item)| item.kind == kind)
                    .unwrap();
                let notification = scheduled.remove(index);
                let kind = notification.kind;

                let mut pending = pending.lock().unwrap();
                assert!(pending.get(kind.into()).unwrap());
                pending.set(kind.into(), false);

                notification
            };

            let _ = post_notification(client, notification).await;
        });

        Ok(())
    }

    pub fn update(&self, frame: Option<impl ToInputArray>) {
        #[inline]
        fn to_png(frame: Option<&impl ToInputArray>) -> Option<Vec<u8>> {
            let frame = frame?;
            let mut bytes = Vector::new();
            imencode_def(".png", frame, &mut bytes).ok()?;
            Some(bytes.to_vec())
        }

        let mut scheduled = self.scheduled.lock().unwrap();
        if scheduled.is_empty() {
            return;
        }

        for item in scheduled.iter_mut() {
            let elapsed_secs = item.instant.elapsed().as_secs() as u32;
            for scheduled_frame in item.frames.iter_mut() {
                if elapsed_secs <= scheduled_frame.deadline_secs {
                    if scheduled_frame.inner.is_none() {
                        scheduled_frame.inner = to_png(frame.as_ref());
                    }
                    break;
                }
            }
        }
    }
}

async fn post_notification(
    client: Client,
    notification: ScheduledNotification,
) -> Result<(), Error> {
    match notification.provider {
        WebhookProvider::Discord => post_discord_notification(client, notification).await,
        WebhookProvider::Feishu => post_feishu_notification(client, notification).await,
    }
}

async fn post_discord_notification(
    client: Client,
    notification: ScheduledNotification,
) -> Result<(), Error> {
    let attachments = notification
        .frames
        .iter()
        .filter(|frame| frame.inner.is_some())
        .enumerate()
        .map(|(i, _)| {
            json!({
                "id": i,
                "description": format!("Game snapshot #{i}"),
                "filename": format!("image_{i}.png"),
            })
        })
        .collect::<Vec<Value>>();
    let body = json!({
        "content": notification.content,
        "username": notification.username,
        "attachments": attachments,
    });
    let mut form = Form::new().text("payload_json", serde_json::to_string(&body).unwrap());
    for (i, frame) in notification
        .frames
        .into_iter()
        .filter_map(|frame| frame.inner)
        .enumerate()
    {
        form = form.part(
            format!("files[{i}]"),
            Part::bytes(frame)
                .mime_str("image/png")
                .unwrap()
                .file_name(format!("image_{i}.png")),
        );
    }

    let _ = client
        .post(notification.url)
        .multipart(form)
        .send()
        .await
        .inspect(|_| {
            debug!(target: "backend/notification", "calling Webhook API {:?} succeeded", notification.kind);
        })
        .inspect_err(|err| {
            error!(target: "backend/notification", "calling Webhook API failed {err}");
        });

    Ok(())
}

async fn post_feishu_notification(
    client: Client,
    notification: ScheduledNotification,
) -> Result<(), Error> {
    let body: Value = json!({
        "msg_type": "text",
        "content": {
            "text": format!("{} from {}", notification.content, notification.username),
        },
    });
    let _ = client
        .post(notification.url)
        .json(&body)
        .send()
        .await
        .inspect(|_| {
            debug!(target: "notification", "calling Webhook API {:?} succeeded", notification.kind);
        })
        .inspect_err(|err| {
            error!(target: "notification", "calling Webhook API failed {err}");
        });

    Ok(())
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc, time::Duration};

    use opencv::core::{CV_8UC4, Mat, MatExprTraitConst};
    use reqwest::Url;
    use tokio::time::{Instant, advance};

    use super::{Notification, NotificationKind, ScheduledNotification};
    use crate::{
        Notifications, Settings, WebhookProvider, mat::OwnedMat, notification::ScheduledFrame,
    };

    #[tokio::test(start_paused = true)]
    async fn schedule_kind_unique() {
        let noti = Notification::new(Rc::new(RefCell::new(Settings {
            notifications: Notifications {
                webhook_url: "https://discord.com/api/webhooks/foo/bar".to_string(),
                notify_on_fail_or_change_map: true,
                notify_on_rune_appear: true,
                ..Default::default()
            },
            ..Default::default()
        })));

        assert!(
            noti.schedule_notification_inner(NotificationKind::FailOrMapChange)
                .is_ok()
        );
        assert!(noti.scheduled.lock().unwrap().len() == 1);
        assert!(
            noti.pending
                .lock()
                .unwrap()
                .get(NotificationKind::FailOrMapChange.into())
                .unwrap()
        );
        assert!(
            noti.schedule_notification_inner(NotificationKind::FailOrMapChange)
                .is_err()
        );
        assert!(
            noti.schedule_notification_inner(NotificationKind::RuneAppear)
                .is_ok()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn schedule_invalid_url() {
        let noti = Notification::new(Rc::new(RefCell::new(Settings {
            notifications: Notifications {
                notify_on_fail_or_change_map: true,
                ..Default::default()
            },
            ..Default::default()
        })));

        assert!(
            noti.schedule_notification_inner(NotificationKind::FailOrMapChange)
                .is_err()
        );
    }

    #[tokio::test(start_paused = true)]
    #[allow(clippy::await_holding_lock)]
    async fn update_scheduled_frames_deadline() {
        let noti = Notification::new(Rc::new(RefCell::new(Settings::default())));
        noti.scheduled.lock().unwrap().push(ScheduledNotification {
            instant: Instant::now(),
            kind: NotificationKind::FailOrMapChange,
            url: Url::parse("https://example.com").unwrap(),
            provider: WebhookProvider::Discord,
            content: "content".into(),
            username: "username",
            frames: vec![
                ScheduledFrame::new_deadline(3),
                ScheduledFrame::new_deadline(6),
                ScheduledFrame::new_deadline(9),
            ],
        });

        advance(Duration::from_secs(4)).await;
        // Skip frame 1 because deadline passed to frame 2
        noti.update(Some(
            &OwnedMat::from(Mat::zeros(1, 1, CV_8UC4).unwrap().to_mat().unwrap()).as_mat(),
        ));
        let scheduled_guard = noti.scheduled.lock().unwrap();
        let scheduled = scheduled_guard.first().unwrap();
        assert!(scheduled.frames[0].inner.is_none());
        assert!(scheduled.frames[1].inner.is_some());
        assert!(scheduled.frames[2].inner.is_none());
        drop(scheduled_guard);

        // Frame 3
        advance(Duration::from_secs(4)).await;
        noti.update(Some(
            &OwnedMat::from(Mat::zeros(1, 1, CV_8UC4).unwrap().to_mat().unwrap()).as_mat(),
        ));
        let scheduled = noti.scheduled.lock().unwrap();
        let scheduled = scheduled.first().unwrap();
        assert!(scheduled.frames[0].inner.is_none());
        assert!(scheduled.frames[1].inner.is_some());
        assert!(scheduled.frames[2].inner.is_some());
    }
}
