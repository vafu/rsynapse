use rsynapse_plugin::{Plugin, ResultItem};

struct CalcPlugin;

impl Plugin for CalcPlugin {
    fn name(&self) -> &'static str {
        "Calculator"
    }

    fn query(&self, query: &str) -> Vec<ResultItem> {
        match meval::eval_str(query) {
            Ok(result) => {
                if !result.is_finite() {
                    return Vec::new();
                }

                vec![ResultItem {
                    id: format!("{}::{}", self.name(), query),
                    title: result.to_string(),
                    description: Some("Result".to_string()),
                    icon: Some("accessories-calculator".to_string()),
                    command: None,
                    score: 100.0,
                }]
            }
            Err(_) => Vec::new(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    Box::into_raw(Box::new(CalcPlugin))
}
