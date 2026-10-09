use super::{ProjectDetails, cwd_label, non_empty};
use shell_core::source::{self, Observable, rx::Observable as _};
pub(super) fn resolve(details: Observable<ProjectDetails>) -> Observable<ProjectDetails> {
    details
        .combine_latest(
            source::proj::checkouts().start_with(vec![Vec::<source::proj::CheckoutInfo>::new()]),
            |details, checkouts| (details, checkouts),
        )
        .combine_latest(
            source::proj::projects().start_with(vec![Vec::<source::proj::ProjectInfo>::new()]),
            |(details, checkouts), projects| (details, checkouts, projects),
        )
        .combine_latest(
            source::proj::contexts().start_with(vec![Vec::<source::proj::ContextInfo>::new()]),
            |(mut details, checkouts, projects), contexts| {
                if let Some(checkout) = checkouts
                    .iter()
                    .find(|c| Some(c.root_path.as_str()) == details.path.as_deref())
                {
                    if let Some(project) = projects.iter().find(|p| p.id == checkout.project_id) {
                        details.name = Some(project.name.clone());
                        details.display_main = Some(project.name.clone());
                    }
                    details.branch = non_empty(Some(checkout.branch.clone()));
                    details.display_secondary = details.branch.clone();
                    if let Some(context) = contexts
                        .iter()
                        .find(|c| Some(c.id.as_str()) == details.context_id.as_deref())
                    {
                        details.cwd_label = cwd_label(
                            Some(&context.relative_cwd),
                            Some(&context.cwd),
                            details.path.as_deref(),
                        );
                    }
                }
                details
            },
        )
        .distinct_until_changed()
        .box_it()
}
