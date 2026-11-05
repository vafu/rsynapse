use rsynapse_plugin::{Plugin, PluginGroup, PluginResponse, QueryContext, ResultItem};

struct CalcPlugin;

impl Plugin for CalcPlugin {
    fn name(&self) -> &'static str {
        "Calculator"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![PluginGroup {
            id: "calc".to_owned(),
            title: "Calculator".to_owned(),
            description: Some("Evaluate a calculator expression".to_owned()),
            aliases: vec!["calculator".to_owned(), "=".to_owned()],
        }]
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        match meval::eval_str(context.query) {
            Ok(result) => {
                if !result.is_finite() {
                    return PluginResponse::default();
                }

                PluginResponse {
                    view: None,
                    results: vec![ResultItem {
                        id: format!("{}::{}", self.name(), context.query),
                        title: result.to_string(),
                        description: Some("Result".to_string()),
                        icon: Some("accessories-calculator".to_string()),
                        data: Some(result.to_string()),
                        score: 100.0,
                        current: false,
                    }],
                }
            }
            Err(_) => PluginResponse::default(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    Box::into_raw(Box::new(CalcPlugin))
}
