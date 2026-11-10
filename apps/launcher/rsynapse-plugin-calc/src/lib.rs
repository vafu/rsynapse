use rsynapse_plugin::{
    ActionHint, LauncherActivity, Plugin, PluginResponse, PluginScope, PluginSpec, QueryContext,
    ResultItem,
};

struct CalcPlugin;

impl Plugin for CalcPlugin {
    fn spec(&self) -> PluginSpec {
        PluginSpec {
            id: "calc",
            name: "Calculator",
            aliases: &["calculator", "="],
            scope: PluginScope::Both,
        }
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        match meval::eval_str(context.query) {
            Ok(result) => {
                if !result.is_finite() {
                    return PluginResponse::default();
                }

                PluginResponse {
                    summary: String::new(),
                    actions: vec![ActionHint::Apply],
                    results: vec![ResultItem {
                        id: format!("{}::{}", "Calculator", context.query),
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
