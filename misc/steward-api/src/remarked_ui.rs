//! Typed client for the remarked UI notebook-selection service.
use zbus::proxy;

#[proxy(
    interface = "io.github.remarked.Remarked.UI.App1",
    default_service = "io.github.remarked.Remarked.UI",
    default_path = "/io/github/remarked/Remarked/UI"
)]
pub trait RemarkedUi {
    fn open_notebook(&self, scope: (String, Vec<String>)) -> zbus::Result<()>;
    fn get_notebooks(&self) -> zbus::Result<Vec<(String, (String, Vec<String>))>>;
}
