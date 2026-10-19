mod commands;
mod protocol;
mod socket;
mod state;
pub(crate) use commands::command;

use crate::model::Projection;
use socket::Socket;
use state::State;
use std::path::Path;

pub(crate) struct Umbriel {
    stream: Socket,
    state: State,
}
impl Umbriel {
    pub async fn connect(path: Option<&Path>) -> anyhow::Result<Self> {
        Ok(Self {
            stream: Socket::connect(path).await?,
            state: State::default(),
        })
    }
    pub async fn next(&mut self) -> anyhow::Result<Projection> {
        loop {
            if let Some(projection) = self.state.apply(self.stream.event().await?)? {
                return Ok(projection);
            }
        }
    }
}
