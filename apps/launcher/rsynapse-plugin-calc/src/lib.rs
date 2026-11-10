use rsynapse_plugin::{
    LauncherActivity, Plugin, PluginGroup, PluginResponse, QueryContext, ResultItem,
};

struct CalcPlugin;

impl Plugin for CalcPlugin {
    fn name(&self) -> &'static str {
        "Calculator"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![PluginGroup {
            id: "calc".to_owned(),
            title: "Calculator".to_owned(),
            description: "Evaluate a calculator expression".to_owned(),
            aliases: vec!["calculator".to_owned(), "=".to_owned()],
            plugin: String::new(),
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
                        description: "Result".to_string(),
                        icon: "accessories-calculator".to_string(),
                        data: result.to_string(),
                        score: 100.0,
                        activity: LauncherActivity::None,
                        current: false,
                        keep_open: false,
                    }],
                }
            }
            Err(_) => PluginResponse::default(),
        }
    }
}

pub fn plugin() -> Box<dyn Plugin> {
    Box::new(CalcPlugin)
}
