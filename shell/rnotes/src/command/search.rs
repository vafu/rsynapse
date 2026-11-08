use anyhow::Result;
use notebook_api::search::LexicalSearchQuery;

use crate::Session;
use crate::args::SearchArgs;
use crate::output::{Output, PageInfo};

use super::common::validate_limit;

pub async fn execute(args: SearchArgs, session: &Session) -> Result<Output> {
    let limit = validate_limit(args.limit)?;
    let page = session
        .notes()
        .await?
        .search(LexicalSearchQuery {
            text: args.query.join(" "),
            required_tags: args.tags,
            limit,
            snippet_chars: args.snippet_chars,
        })
        .await?;
    let text = page
        .results
        .iter()
        .map(|result| {
            format!(
                "{}\t{}\t{}",
                result.note.id,
                result.note.title,
                result.snippet.replace('\n', " ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output {
        command: "search",
        data: serde_json::to_value(&page.results)?,
        page: Some(PageInfo {
            limit: page.limit,
            truncated: page.truncated,
        }),
        text,
    })
}
