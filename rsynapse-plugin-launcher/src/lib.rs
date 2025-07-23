use rsynapse_plugin::{Plugin, ResultItem};

struct LauncherPlugin;

impl Plugin for LauncherPlugin {
    fn name(&self) -> &'static str {
        "Application Launcher"
    }

    fn query(&self, query: &str) -> Vec<ResultItem> {
        let items = vec!["Firefox", "Terminal", "File Manager", "Text Editor"];

        items
            .into_iter()
            .filter(|item| item.to_lowercase().contains(&query.to_lowercase()))
            .map(|item| ResultItem {
                icon: None,
                id: item.to_lowercase(),
                title: item.to_string(),
                description: Some(format!("Execute {}", item)),
            })
            .collect()
    }
}

/// This is the plugin's entry point. The daemon will look for this exact
/// symbol to instantiate the plugin. `#[no_mangle]` prevents name mangling.
/// The return type is a raw pointer to a trait object, which the daemon
/// will then reconstruct into a `Box<dyn Plugin>`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    Box::into_raw(Box::new(LauncherPlugin))
}
