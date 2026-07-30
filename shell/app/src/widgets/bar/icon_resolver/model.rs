use std::collections::BTreeSet;

use crate::widgets::nerd_icon::NerdIcon;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct IconChoice {
    pub(in crate::widgets::bar) icon: String,
    pub(in crate::widgets::bar) glyph: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct IconCandidate {
    pub(in crate::widgets::bar) icon: String,
    pub(in crate::widgets::bar) glyph: Option<String>,
    pub(in crate::widgets::bar) score_millis: u16,
    pub(in crate::widgets::bar) source: IconCandidateSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) enum IconCandidateSource {
    Alias,
    Picker,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct IconEvidence {
    pub(in crate::widgets::bar) kind: IconEvidenceKind,
    pub(in crate::widgets::bar) value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) enum IconEvidenceKind {
    AppId,
    DesktopName,
    DesktopIcon,
    ProjectName,
    ProjectDisplayMain,
    ProjectDisplaySecondary,
    ProjectBranch,
    ProjectCwd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct IconPolicy {
    min_picker_inputs: usize,
    min_picker_score_millis: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct IconRequest {
    namespace: String,
    fallback: IconChoice,
    override_icon: Option<IconChoice>,
    evidence: Vec<IconEvidence>,
    policy: IconPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct IconResolution {
    pub(in crate::widgets::bar) selected: IconChoice,
    pub(in crate::widgets::bar) candidates: Vec<IconCandidate>,
    pub(in crate::widgets::bar) picker_input: String,
    pub(in crate::widgets::bar) overridden: bool,
}

impl IconChoice {
    pub(in crate::widgets::bar) fn new(icon: String, glyph: Option<String>) -> Option<Self> {
        let icon = non_empty(icon)?;
        Some(Self {
            icon,
            glyph: glyph.and_then(non_empty),
        })
    }

    #[cfg(test)]
    pub(in crate::widgets::bar) fn named(icon: &str) -> Self {
        Self {
            icon: icon.to_owned(),
            glyph: None,
        }
    }

    pub(in crate::widgets::bar) fn from_nerd_icon(icon: NerdIcon) -> Self {
        Self {
            icon: icon.key().to_owned(),
            glyph: Some(icon.glyph().to_owned()),
        }
    }

    pub(in crate::widgets::bar) fn to_nerd_icon(&self, fallback: NerdIcon) -> NerdIcon {
        NerdIcon::from_parts(self.icon.clone(), self.glyph.clone(), fallback)
    }
}

impl From<&IconCandidate> for IconChoice {
    fn from(candidate: &IconCandidate) -> Self {
        Self {
            icon: candidate.icon.clone(),
            glyph: candidate.glyph.clone(),
        }
    }
}

impl IconCandidate {
    pub(in crate::widgets::bar) fn new(
        choice: IconChoice,
        score_millis: u16,
        source: IconCandidateSource,
    ) -> Self {
        Self {
            icon: choice.icon,
            glyph: choice.glyph,
            score_millis,
            source,
        }
    }

    pub(in crate::widgets::bar::icon_resolver) fn identity(&self) -> String {
        format!(
            "{}:{}",
            self.icon,
            self.glyph.as_deref().unwrap_or_default()
        )
    }
}

impl IconEvidence {
    pub(in crate::widgets::bar) fn new(
        kind: IconEvidenceKind,
        value: impl Into<String>,
    ) -> Option<Self> {
        non_empty(value.into()).map(|value| Self { kind, value })
    }

    pub(in crate::widgets::bar::icon_resolver) fn key(&self) -> &'static str {
        self.kind.key()
    }
}

impl IconEvidenceKind {
    pub(in crate::widgets::bar::icon_resolver) fn key(&self) -> &'static str {
        match self {
            Self::AppId => "app-id",
            Self::DesktopName => "desktop-name",
            Self::DesktopIcon => "desktop-icon",
            Self::ProjectName => "project-name",
            Self::ProjectDisplayMain => "project-display-main",
            Self::ProjectDisplaySecondary => "project-display-secondary",
            Self::ProjectBranch => "project-branch",
            Self::ProjectCwd => "project-cwd",
        }
    }
}

impl IconPolicy {
    pub(in crate::widgets::bar) fn window_app() -> Self {
        Self::new(1, 720)
    }

    pub(in crate::widgets::bar) fn workspace_project() -> Self {
        Self::new(1, 660)
    }

    pub(in crate::widgets::bar) fn workspace_apps() -> Self {
        Self::new(2, 720)
    }

    const fn new(min_picker_inputs: usize, min_picker_score_millis: u16) -> Self {
        Self {
            min_picker_inputs,
            min_picker_score_millis,
        }
    }

    pub(in crate::widgets::bar::icon_resolver) fn min_picker_inputs(&self) -> usize {
        self.min_picker_inputs
    }

    pub(in crate::widgets::bar::icon_resolver) fn min_picker_score_millis(&self) -> u16 {
        self.min_picker_score_millis
    }
}

impl IconRequest {
    pub(in crate::widgets::bar) fn new(
        namespace: impl Into<String>,
        fallback: IconChoice,
        policy: IconPolicy,
        evidence: Vec<IconEvidence>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            fallback,
            override_icon: None,
            evidence,
            policy,
        }
    }

    pub(in crate::widgets::bar) fn with_override(
        mut self,
        override_icon: Option<IconChoice>,
    ) -> Self {
        self.override_icon = override_icon;
        self
    }

    pub(in crate::widgets::bar) fn fallback(&self) -> &IconChoice {
        &self.fallback
    }

    pub(in crate::widgets::bar) fn override_icon(&self) -> Option<&IconChoice> {
        self.override_icon.as_ref()
    }

    pub(in crate::widgets::bar) fn evidence(&self) -> &[IconEvidence] {
        &self.evidence
    }

    pub(in crate::widgets::bar::icon_resolver) fn min_picker_score_millis(&self) -> u16 {
        self.policy.min_picker_score_millis()
    }

    pub(in crate::widgets::bar::icon_resolver) fn picker_strings(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.evidence
            .iter()
            .filter_map(|evidence| non_empty(evidence.value.clone()))
            .filter(|value| seen.insert(value.clone()))
            .collect()
    }

    pub(in crate::widgets::bar::icon_resolver) fn picker_input(&self) -> String {
        self.picker_strings().join("\n")
    }

    pub(in crate::widgets::bar::icon_resolver) fn picker_cache_key(&self) -> Option<String> {
        let strings = self.picker_strings();
        (strings.len() >= self.policy.min_picker_inputs()).then(|| {
            format!(
                "{}:{}:{}",
                self.namespace,
                self.policy.min_picker_score_millis(),
                strings.join("\u{1f}")
            )
        })
    }

    pub(in crate::widgets::bar::icon_resolver) fn key(&self) -> String {
        let evidence = self
            .evidence
            .iter()
            .map(|evidence| format!("{}={}", evidence.key(), evidence.value))
            .collect::<Vec<_>>()
            .join("\u{1e}");
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.namespace,
            self.policy.min_picker_inputs(),
            self.policy.min_picker_score_millis(),
            self.fallback.icon,
            self.fallback.glyph.as_deref().unwrap_or_default(),
            self.override_icon
                .as_ref()
                .map(|icon| format!(
                    "{}:{}",
                    icon.icon,
                    icon.glyph.as_deref().unwrap_or_default()
                ))
                .unwrap_or_default(),
            evidence
        )
    }
}

impl IconResolution {
    pub(in crate::widgets::bar) fn selected_nerd_icon(&self, fallback: NerdIcon) -> NerdIcon {
        self.selected.to_nerd_icon(fallback)
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}
