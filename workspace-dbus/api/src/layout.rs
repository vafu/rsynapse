use serde::{Deserialize, Serialize};
use zbus::zvariant::{OwnedValue, Type, Value};

/// Effective workspace tiling mode, independent of compositor and configuration
/// defaults. Unknown modes remain observable without pretending to be scrolling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
#[zvariant(signature = "s")]
pub enum Layout {
    Scrolling,
    Master,
    Dwindle,
    #[default]
    #[serde(other)]
    Unknown,
}

impl Layout {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scrolling => "scrolling",
            Self::Master => "master",
            Self::Dwindle => "dwindle",
            Self::Unknown => "unknown",
        }
    }
}

impl From<&str> for Layout {
    fn from(value: &str) -> Self {
        match value {
            "scrolling" => Self::Scrolling,
            "master" => Self::Master,
            "dwindle" => Self::Dwindle,
            _ => Self::Unknown,
        }
    }
}

impl From<Layout> for Value<'static> {
    fn from(value: Layout) -> Self {
        Self::from(value.as_str())
    }
}

impl TryFrom<OwnedValue> for Layout {
    type Error = zbus::zvariant::Error;

    fn try_from(value: OwnedValue) -> Result<Self, Self::Error> {
        String::try_from(value).map(|value| Self::from(value.as_str()))
    }
}
