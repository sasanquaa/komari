use std::future::Future;

use anyhow::Error;
use bit_vec::BitVec;
use futures::FutureExt;
use log::info;
use strum::Display;
use tokio::{
    runtime::Handle,
    spawn,
    task::{JoinHandle, block_in_place},
};
use tonic::{
    Request, Status,
    transport::{Channel, Endpoint},
};

use crate::grpc::input::{
    Coordinate, Key, KeyDownRequest, KeyInitRequest, KeyInitResponse, KeyRequest, KeyState,
    KeyStateRequest, KeyUpRequest, MouseAction, MouseRequest, key_input_client::KeyInputClient,
};

type RpcConnectingFuture = JoinHandle<Option<(KeyInputClient<Channel>, KeyInitResponse)>>;

#[derive(Debug, Display)]
enum State {
    Disconnected,
    Connecting(RpcConnectingFuture),
    Connected(KeyInputClient<Channel>),
}

#[derive(Debug)]
pub struct InputService {
    state: State,
    endpoint: Endpoint,
    seed: Vec<u8>,
    key_down: BitVec,
    mouse_coordinate: Coordinate,
}

impl InputService {
    pub fn new<D>(dest: D, seed: Vec<u8>) -> Result<Self, Error>
    where
        D: TryInto<Endpoint>,
        D: AsRef<str>,
        D::Error: std::error::Error + Send + Sync + 'static,
    {
        Ok(Self {
            state: State::Disconnected,
            endpoint: TryInto::<Endpoint>::try_into(dest.as_ref().to_string())?,
            seed,
            key_down: BitVec::from_elem(128, false),
            mouse_coordinate: Coordinate::Screen,
        })
    }

    pub fn state(&self) -> String {
        self.state.to_string()
    }

    pub fn mouse_coordinate(&self) -> Coordinate {
        self.mouse_coordinate
    }

    pub fn key_state(&mut self, key: Key) -> Option<KeyState> {
        let mut state = None;

        self.with_client(|client| {
            let response = block_future(async {
                client
                    .key_state(Request::new(KeyStateRequest { key: key.into() }))
                    .await
            })?;

            state = KeyState::try_from(response.into_inner().state).ok();
            Ok(())
        });

        state
    }

    pub fn send_mouse(&mut self, width: i32, height: i32, x: i32, y: i32, action: MouseAction) {
        self.with_client(|client| {
            block_future(async {
                client
                    .send_mouse(Request::new(MouseRequest {
                        width,
                        height,
                        x,
                        y,
                        action: action.into(),
                    }))
                    .await
            })?;
            Ok(())
        });
    }

    pub fn send_key(&mut self, key: Key, down_ms: f32) {
        self.with_client(|client| {
            block_future(async {
                client
                    .send(Request::new(KeyRequest {
                        key: key.into(),
                        down_ms,
                    }))
                    .await
            })?;
            Ok(())
        });

        self.key_down.set(i32::from(key) as usize, false);
    }

    pub fn send_key_down(&mut self, key: Key) {
        if !self.can_send_key(key, true) {
            return;
        }

        self.with_client(|client| {
            block_future(async {
                client
                    .send_down(Request::new(KeyDownRequest { key: key.into() }))
                    .await
            })?;
            Ok(())
        });

        self.key_down.set(i32::from(key) as usize, true);
    }

    pub fn send_key_up(&mut self, key: Key) {
        if !self.can_send_key(key, false) {
            return;
        }

        self.with_client(|client| {
            block_future(async {
                client
                    .send_up(Request::new(KeyUpRequest { key: key.into() }))
                    .await
            })?;
            Ok(())
        });

        self.key_down.set(i32::from(key) as usize, false);
    }

    #[inline]
    fn can_send_key(&self, key: Key, is_down: bool) -> bool {
        let idx = i32::from(key) as usize;
        let was_down = self.key_down.get(idx).unwrap();
        !matches!((was_down, is_down), (true, true) | (false, false))
    }

    fn with_client<F>(&mut self, f: F)
    where
        F: FnOnce(&mut KeyInputClient<Channel>) -> Result<(), Status>,
    {
        if !self.ensure_connected() {
            return;
        }

        let State::Connected(client) = &mut self.state else {
            return;
        };

        if let Err(status) = f(client) {
            info!(target: "backend/rpc", "rpc call failed: {status}");
            self.state = State::Disconnected;
        }
    }

    fn ensure_connected(&mut self) -> bool {
        match &mut self.state {
            State::Connected(_) => return true,
            State::Connecting(handle) => {
                if !handle.is_finished() {
                    return false;
                }

                if let Some((client, response)) = handle
                    .now_or_never()
                    .and_then(|result| result.ok().flatten())
                {
                    self.mouse_coordinate = response.mouse_coordinate();
                    self.state = State::Connected(client);
                    return true;
                }
            }
            State::Disconnected => (),
        }

        let endpoint = self.endpoint.clone();
        let seed = self.seed.clone();
        info!(target: "backend/rpc", "connecting to input server {}", endpoint.uri());

        let task = spawn(async move {
            let mut client = KeyInputClient::connect(endpoint).await.ok()?;
            let response = client.init(KeyInitRequest { seed }).await.ok()?;
            Some((client, response.into_inner()))
        });

        self.state = State::Connecting(task);
        false
    }
}

impl Drop for InputService {
    fn drop(&mut self) {
        for i in 0..self.key_down.len() {
            if Key::try_from(i as i32).is_ok() {
                self.with_client(|client| {
                    block_future(async {
                        client
                            .send_up(Request::new(KeyUpRequest { key: i as i32 }))
                            .await
                    })?;
                    Ok(())
                });
            }
        }
    }
}

#[inline]
fn block_future<F: Future>(f: F) -> F::Output {
    block_in_place(|| Handle::current().block_on(f))
}

#[cfg(test)]
mod test {
    // TODO
}
