//! Minimal Markdown frontmatter reader/writer for task notes.
//!
//! Mirrors the on-disk format owned by Remarked (`---` fenced YAML) without
//! depending on its implementation crates.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};

pub struct Document {
    pub yaml: Option<String>,
    pub body: String,
}

pub fn split(raw: &str) -> Document {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return Document {
            yaml: None,
            body: raw.to_string(),
        };
    };
    let Some(end) = rest.find("\n---\n") else {
        return Document {
            yaml: None,
            body: raw.to_string(),
        };
    };
    Document {
        yaml: Some(rest[..end].to_string()),
        body: rest[end + "\n---\n".len()..].to_string(),
    }
}

pub fn parse(document: &Document) -> Result<Mapping, String> {
    let Some(yaml) = &document.yaml else {
        return Err("missing frontmatter".to_string());
    };
    serde_yaml::from_str::<Mapping>(yaml).map_err(|error| format!("invalid frontmatter: {error}"))
}

pub fn render(mapping: &Mapping, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(mapping).context("serialize frontmatter")?;
    Ok(format!("---\n{yaml}---\n{body}"))
}

pub fn read(path: &Path) -> Result<Document> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(split(&raw))
}

pub fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("path has no UTF-8 file name: {}", path.display()))?;
    let tmp: PathBuf = parent.join(format!(".{name}.rnotes-tmp-{}", std::process::id()));
    {
        let mut file = fs::File::create(&tmp)
            .with_context(|| format!("create temporary file {}", tmp.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("write {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("sync {}", tmp.display()))?;
    }
    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

pub fn tags(mapping: &Mapping) -> Vec<String> {
    let key = Value::String("tags".to_string());
    match mapping.get(&key) {
        Some(Value::Sequence(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        Some(Value::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    }
}

pub fn set_tags(mapping: &mut Mapping, tags: Vec<String>) {
    let mut deduped: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_string();
        if !tag.is_empty() && !deduped.contains(&tag) {
            deduped.push(tag);
        }
    }
    mapping.insert(
        Value::String("tags".to_string()),
        Value::Sequence(deduped.into_iter().map(Value::String).collect()),
    );
}

pub fn title(mapping: &Mapping) -> Option<String> {
    mapping
        .get(Value::String("title".to_string()))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub fn set_title(mapping: &mut Mapping, title: &str) {
    mapping.insert(
        Value::String("title".to_string()),
        Value::String(title.to_string()),
    );
}

pub fn replace_tag_namespace(mapping: &mut Mapping, namespace: &str, values: &[String]) {
    let prefix = format!("{namespace}/");
    let mut tags = tags(mapping);
    tags.retain(|tag| !tag.starts_with(&prefix));
    tags.extend(values.iter().cloned());
    set_tags(mapping, tags);
}

pub fn tags_with_prefix(mapping: &Mapping, namespace: &str) -> Vec<String> {
    let prefix = format!("{namespace}/");
    tags(mapping)
        .into_iter()
        .filter(|tag| tag.starts_with(&prefix))
        .collect()
}
