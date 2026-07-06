use std::{fs, path::PathBuf};

use super::{
    StylesheetError,
    compiler::{SassConfig, compile_scss},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct StylesheetSource {
    path: PathBuf,
}

impl StylesheetSource {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub(crate) fn load(&self, sass_config: &SassConfig) -> Result<String, StylesheetError> {
        match self.kind()? {
            StylesheetKind::Css => {
                fs::read_to_string(&self.path).map_err(|source| StylesheetError::Read {
                    path: self.path.clone(),
                    source,
                })
            }
            StylesheetKind::Sass => compile_scss(&self.path, sass_config),
        }
    }

    pub(crate) fn watch_roots(
        &self,
        sass_config: &SassConfig,
    ) -> Result<Vec<PathBuf>, StylesheetError> {
        match self.kind()? {
            StylesheetKind::Css => Ok(vec![self.path.clone()]),
            StylesheetKind::Sass => {
                let mut roots = vec![
                    self.path
                        .parent()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from(".")),
                ];
                roots.extend(sass_config.load_paths().iter().cloned());
                roots.sort();
                roots.dedup();
                Ok(roots)
            }
        }
    }

    fn kind(&self) -> Result<StylesheetKind, StylesheetError> {
        let Some(extension) = self
            .path
            .extension()
            .and_then(|extension| extension.to_str())
        else {
            return Err(StylesheetError::UnsupportedExtension {
                path: self.path.clone(),
            });
        };

        if extension.eq_ignore_ascii_case("css") {
            Ok(StylesheetKind::Css)
        } else if extension.eq_ignore_ascii_case("scss") || extension.eq_ignore_ascii_case("sass") {
            Ok(StylesheetKind::Sass)
        } else {
            Err(StylesheetError::UnsupportedExtension {
                path: self.path.clone(),
            })
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum StylesheetKind {
    Css,
    Sass,
}
