use std::sync::OnceLock;

const NERD_ICON_PREFIXES: &[&str] = &[
    "cod-", "custom-", "dev-", "fa-", "fae-", "iec-", "linux-", "md-", "oct-", "ple-", "pom-",
    "seti-", "weather-",
];

static ICONS: OnceLock<Vec<NerdIcon>> = OnceLock::new();

/// A named Nerd Font glyph that can be displayed or selected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NerdIcon {
    name: String,
    glyph: String,
}

impl NerdIcon {
    /// Construct an icon supplied by a picker consumer, such as a suggestion.
    pub fn specific(name: impl Into<String>, glyph: impl Into<String>) -> Option<Self> {
        let name = non_empty(name.into())?;
        let glyph = non_empty(glyph.into())?;
        Some(Self { name, glyph })
    }

    /// Return the searchable Nerd Font name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Return the glyph text rendered by a Nerd Font.
    pub fn glyph(&self) -> &str {
        self.glyph.as_str()
    }
}

/// Search the bundled Nerd Font catalog, ordered by name relevance.
pub fn search_icons(query: &str, limit: usize) -> Vec<NerdIcon> {
    let terms = search_terms(query);
    if terms.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut matches = icons()
        .iter()
        .filter_map(|icon| search_rank(icon.name(), &terms).map(|rank| (rank, icon)))
        .collect::<Vec<_>>();
    matches.sort_by(|(left_rank, left), (right_rank, right)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| left.name.cmp(&right.name))
    });
    matches
        .into_iter()
        .take(limit)
        .map(|(_, icon)| icon.clone())
        .collect()
}

fn icons() -> &'static [NerdIcon] {
    ICONS.get_or_init(|| {
        nerd_font::load_font()
            .glyphs()
            .iter()
            .filter(|glyph| {
                NERD_ICON_PREFIXES
                    .iter()
                    .any(|prefix| glyph.name().starts_with(prefix))
            })
            .map(|glyph| NerdIcon {
                name: glyph.name().to_owned(),
                glyph: glyph.char().to_string(),
            })
            .collect()
    })
}

fn search_terms(query: &str) -> Vec<String> {
    query
        .split(|character: char| character.is_whitespace() || character == '-' || character == '_')
        .filter_map(|term| non_empty(term.to_lowercase()))
        .collect()
}

fn search_rank(name: &str, terms: &[String]) -> Option<(usize, usize)> {
    let searchable = name.replace(['-', '_'], " ").to_lowercase();
    let positions = terms
        .iter()
        .map(|term| searchable.find(term))
        .collect::<Option<Vec<_>>>()?;
    let first = positions.iter().copied().min().unwrap_or_default();
    let distance = positions.into_iter().sum();
    Some((first, distance))
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod test;
