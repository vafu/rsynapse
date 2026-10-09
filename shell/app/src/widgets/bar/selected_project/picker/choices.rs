use shell_core::source::{self, Observable, rx::Observable as _};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CheckoutChoice {
    pub path: String,
    pub branch: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectChoice {
    pub id: String,
    pub name: String,
    pub path: String,
    pub checkouts: Vec<CheckoutChoice>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::widgets::bar) struct ProjectCatalog {
    pub(super) active: Vec<ProjectChoice>,
}
pub(in crate::widgets::bar) fn project_catalog() -> Observable<ProjectCatalog> {
    source::proj::projects()
        .combine_latest(source::proj::checkouts(), |projects, checkouts| {
            let mut projects: Vec<_> = projects
                .into_iter()
                .map(|p| ProjectChoice {
                    checkouts: checkouts
                        .iter()
                        .filter(|c| c.project_id == p.id)
                        .map(|c| CheckoutChoice {
                            path: c.root_path.clone(),
                            branch: c.branch.clone(),
                        })
                        .collect(),
                    id: p.id,
                    name: p.name,
                    path: p.root_path,
                })
                .collect();
            projects.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
            projects
        })
        .map(|active| ProjectCatalog { active })
        .distinct_until_changed()
        .box_it()
}
impl ProjectChoice {
    pub fn matching(&self, query: &str) -> Vec<CheckoutChoice> {
        let query = query.to_lowercase();
        self.checkouts
            .iter()
            .filter(|c| {
                let text =
                    format!("{} {} {} {}", self.name, self.path, c.path, c.branch).to_lowercase();
                query.split_whitespace().all(|word| text.contains(word))
            })
            .cloned()
            .collect()
    }
    pub fn matches(&self, query: &str) -> bool {
        let text = format!("{} {}", self.name, self.path).to_lowercase();
        query
            .to_lowercase()
            .split_whitespace()
            .all(|word| text.contains(word))
            || !self.matching(query).is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_groups_checkouts_by_project_and_matches_branch_and_path() {
        let p = ProjectChoice {
            id: "p".into(),
            name: "Mobile".into(),
            path: "/repo/mobile".into(),
            checkouts: vec![
                CheckoutChoice {
                    path: "/work/coro-conf".into(),
                    branch: "feature/coroutines".into(),
                },
                CheckoutChoice {
                    path: "/work/main".into(),
                    branch: "main".into(),
                },
            ],
        };
        assert_eq!(p.matching("MOBILE coro-conf"), p.checkouts[..1]);
        assert_eq!(p.matching("COROUTINES"), p.checkouts[..1]);
        assert!(!p.matches("not-existing"));
    }
}
