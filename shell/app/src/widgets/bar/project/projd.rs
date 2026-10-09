use super::{ProjectDetails, non_empty};
use shell_core::source::{self, Observable, rx::Observable as _};
pub(super) fn resolve(details: Observable<ProjectDetails>) -> Observable<ProjectDetails> {
    details
        .combine_latest(source::proj::checkouts(), |d, c| (d, c))
        .combine_latest(source::proj::projects(), |(mut d, checkouts), projects| {
            let checkout = checkouts
                .iter()
                .find(|c| Some(c.root_path.as_str()) == d.path.as_deref());
            let project = checkout
                .and_then(|c| projects.iter().find(|p| p.checkout_id == c.id))
                .or_else(|| {
                    projects
                        .iter()
                        .find(|p| Some(p.id.as_str()) == d.project_id.as_deref())
                })
                .or_else(|| {
                    projects
                        .iter()
                        .find(|p| Some(p.cwd.as_str()) == d.path.as_deref())
                });
            if let Some(p) = project {
                d.project_id = Some(p.id.clone());
                d.name = Some(p.name.clone());
                d.display_main = Some(p.name.clone());
                d.cwd = Some(p.cwd.clone());
                let checkout = checkouts.iter().find(|c| c.id == p.checkout_id);
                d.branch = checkout.and_then(|c| non_empty(Some(c.branch.clone())));
                d.display_secondary = d.branch.clone();
                d.path = Some(
                    checkout
                        .map(|c| c.root_path.clone())
                        .unwrap_or_else(|| p.cwd.clone()),
                );
            }
            d
        })
        .distinct_until_changed()
        .box_it()
}
