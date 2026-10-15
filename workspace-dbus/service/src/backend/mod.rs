//! Backend protocols are private to the producer, not the API or its consumers.
mod niri;
mod selection;
mod umbriel;
pub use selection::BackendKind;

use crate::model::Projection;
use std::path::Path;

pub(crate) enum Backend {
    Niri(niri::Niri),
    Umbriel(umbriel::Umbriel),
}

impl Backend {
    pub async fn connect(kind: BackendKind, socket: Option<&Path>) -> anyhow::Result<Self> {
        match kind {
            BackendKind::Niri => Ok(Self::Niri(niri::Niri::connect(socket).await?)),
            BackendKind::Umbriel => Ok(Self::Umbriel(umbriel::Umbriel::connect(socket).await?)),
            BackendKind::Auto => anyhow::bail!("backend must be resolved before connecting"),
        }
    }
    pub async fn next(&mut self) -> anyhow::Result<Projection> {
        match self {
            Self::Niri(backend) => backend.next().await,
            Self::Umbriel(backend) => backend.next().await,
        }
    }
}
