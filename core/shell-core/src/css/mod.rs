mod compiler;
mod error;
mod source;
mod stylesheet;
mod watcher;

pub(crate) use compiler::SassConfig;
pub(crate) use error::StylesheetError;
pub(crate) use source::StylesheetSource;
pub(crate) use stylesheet::{Stylesheet, StylesheetWatcher};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CssPriority {
    Application,
    User,
}

impl CssPriority {
    pub(crate) const fn gtk_priority(self) -> u32 {
        match self {
            Self::Application => gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            Self::User => gtk::STYLE_PROVIDER_PRIORITY_USER,
        }
    }
}

#[cfg(test)]
mod test;
