use crate::text::term_frequency;
use crate::walk::normalized_path;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const CHUNK_CHARS: usize = 6_000;
const CHUNK_OVERLAP: usize = 400;

pub(crate) struct VaultDocument {
    pub(crate) source_path: String,
    pub(crate) title: String,
    pub(crate) heading_path: String,
    pub(crate) body: String,
    pub(crate) fields: [String; 7],
    pub(crate) terms: [HashMap<String, usize>; 7],
    pub(crate) lengths: [usize; 7],
}

fn make_document(
    source_path: &str,
    path_text: &str,
    title: &str,
    heading_path: &str,
    body: String,
    keys: &str,
    tags: &str,
    metadata: &str,
) -> VaultDocument {
    let fields = [
        path_text.to_owned(),
        title.to_owned(),
        heading_path.to_owned(),
        keys.to_owned(),
        tags.to_owned(),
        body.clone(),
        metadata.to_owned(),
    ];
    let terms = std::array::from_fn(|index| term_frequency(&fields[index]));
    let lengths = std::array::from_fn(|index| terms[index].values().sum());
    VaultDocument {
        source_path: source_path.to_owned(),
        title: title.to_owned(),
        heading_path: heading_path.to_owned(),
        body,
        fields,
        terms,
        lengths,
    }
}
fn split_body(body: &str) -> Vec<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.chars().count() <= CHUNK_CHARS {
        return vec![trimmed.to_owned()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    loop {
        let end = trimmed[start..]
            .char_indices()
            .nth(CHUNK_CHARS)
            .map_or(trimmed.len(), |(offset, _)| start + offset);
        let chunk = trimmed[start..end].trim();
        if !chunk.is_empty() {
            chunks.push(chunk.to_owned());
        }
        if end == trimmed.len() {
            break;
        }
        start = trimmed[..end]
            .char_indices()
            .rev()
            .nth(CHUNK_OVERLAP.saturating_sub(1))
            .map_or(end, |(offset, _)| offset);
    }
    chunks
}
fn markdown_documents(source_path: &str, path_text: &str, content: &str) -> Vec<VaultDocument> {
    let title = Path::new(source_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let normalized = content.replace("\r\n", "\n");
    let mut documents = Vec::new();
    let mut body = normalized.as_str();
    if let Some(frontmatter) = body.strip_prefix("---\n") {
        if let Some(end) = frontmatter.find("\n---\n") {
            let value = &frontmatter[..end];
            let keys = value
                .lines()
                .filter_map(|line| line.split_once(':').map(|(key, _)| key))
                .collect::<Vec<_>>()
                .join(" ");
            documents.push(make_document(
                source_path,
                path_text,
                title,
                "__frontmatter__",
                value.to_owned(),
                &keys,
                "",
                "",
            ));
            body = &frontmatter[end + 5..];
        }
    }
    let mut headings: Vec<String> = Vec::new();
    let mut current_heading = "__preamble__".to_owned();
    let mut current = Vec::new();
    let flush = |documents: &mut Vec<VaultDocument>,
                 current: &mut Vec<&str>,
                 headings: &[String],
                 current_heading: &str| {
        let heading_path = if current_heading == "__preamble__" {
            current_heading.to_owned()
        } else {
            headings
                .iter()
                .filter(|value| !value.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" > ")
        };
        for (index, chunk) in split_body(&current.join("\n")).into_iter().enumerate() {
            let heading = if index == 0 {
                heading_path.clone()
            } else {
                format!("{heading_path} [{}]", index + 1)
            };
            documents.push(make_document(
                source_path,
                path_text,
                title,
                &heading,
                chunk,
                "",
                "",
                "",
            ));
        }
        current.clear();
    };
    for line in body.lines() {
        let hashes = line
            .chars()
            .take_while(|character| *character == '#')
            .count();
        let heading = if (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
            Some(line[hashes + 1..].trim())
        } else {
            None
        };
        if let Some(heading) = heading {
            flush(&mut documents, &mut current, &headings, &current_heading);
            headings.resize(hashes, String::new());
            headings[hashes - 1] = heading.to_owned();
            current_heading = heading.to_owned();
        } else {
            current.push(line);
        }
    }
    flush(&mut documents, &mut current, &headings, &current_heading);
    documents
}
fn escaped_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
fn flatten_json(value: &Value, pointer: &str) -> (String, String) {
    fn visit(value: &Value, pointer: &str, keys: &mut Vec<String>, values: &mut Vec<String>) {
        match value {
            Value::Null => values.push(format!("{pointer}: null")),
            Value::Bool(value) => values.push(format!("{pointer}: {value}")),
            Value::Number(value) => values.push(format!("{pointer}: {value}")),
            Value::String(value) => values.push(format!("{pointer}: {value}")),
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    visit(item, &format!("{pointer}/{index}"), keys, values);
                }
            }
            Value::Object(object) => {
                for (key, child) in object {
                    keys.push(key.clone());
                    visit(
                        child,
                        &format!("{pointer}/{}", escaped_pointer(key)),
                        keys,
                        values,
                    );
                }
            }
        }
    }
    let mut keys = Vec::new();
    let mut values = Vec::new();
    visit(value, pointer, &mut keys, &mut values);
    (keys.join(" "), values.join("\n"))
}
fn json_documents(
    source_path: &str,
    path_text: &str,
    value: &Value,
    heading_prefix: &str,
) -> Vec<VaultDocument> {
    let title = Path::new(source_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let records: Vec<(String, &Value)> = match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("{heading_prefix}/{index}"), item))
            .collect(),
        Value::Object(object) => object
            .iter()
            .map(|(key, item)| (format!("{heading_prefix}/{}", escaped_pointer(key)), item))
            .collect(),
        _ => vec![(
            if heading_prefix.is_empty() {
                "$".into()
            } else {
                heading_prefix.into()
            },
            value,
        )],
    };
    records
        .into_iter()
        .flat_map(|(heading, record)| {
            let (keys, body) =
                flatten_json(record, if heading.is_empty() { "$" } else { &heading });
            split_body(&body)
                .into_iter()
                .enumerate()
                .map(|(index, body)| {
                    let chunk_heading = if index == 0 {
                        heading.clone()
                    } else {
                        format!("{heading} [{}]", index + 1)
                    };
                    make_document(
                        source_path,
                        path_text,
                        title,
                        &chunk_heading,
                        body,
                        &keys,
                        "",
                        &heading,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
pub(crate) fn parse_file(
    root: &Path,
    absolute: &Path,
    max_file_bytes: u64,
    warnings: &mut Vec<String>,
) -> Vec<VaultDocument> {
    let source_path = normalized_path(absolute);
    let metadata = match fs::metadata(absolute) {
        Ok(metadata) => metadata,
        Err(_) => {
            warnings.push(format!("{source_path}: skipped unreadable text file."));
            return Vec::new();
        }
    };
    if metadata.len() > max_file_bytes {
        warnings.push(format!(
            "{source_path}: skipped file larger than {max_file_bytes} bytes."
        ));
        return Vec::new();
    }
    let content = match fs::read_to_string(absolute) {
        Ok(content) => content,
        Err(_) => {
            warnings.push(format!("{source_path}: skipped unreadable text file."));
            return Vec::new();
        }
    };
    let path_text = absolute
        .strip_prefix(root)
        .map(normalized_path)
        .unwrap_or_else(|_| source_path.clone());
    let extension = absolute
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    match extension.as_str() {
        "md" | "markdown" => markdown_documents(&source_path, &path_text, &content),
        "json" => match serde_json::from_str(&content) {
            Ok(value) => json_documents(&source_path, &path_text, &value, ""),
            Err(_) => {
                warnings.push(format!("{source_path}: skipped malformed JSON."));
                Vec::new()
            }
        },
        "jsonl" => {
            let mut documents = Vec::new();
            let mut malformed = 0;
            for (index, line) in content.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str(line) {
                    Ok(value) => documents.extend(json_documents(
                        &source_path,
                        &path_text,
                        &value,
                        &format!("line:{}", index + 1),
                    )),
                    Err(_) => malformed += 1,
                }
            }
            if malformed > 0 {
                warnings.push(format!(
                    "{source_path}: skipped {malformed} malformed JSONL record{}.",
                    if malformed == 1 { "" } else { "s" }
                ));
            }
            documents
        }
        _ => {
            let title = absolute
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            split_body(&content)
                .into_iter()
                .enumerate()
                .map(|(index, body)| {
                    let heading = if index == 0 {
                        "__document__".into()
                    } else {
                        format!("__document__ [{}]", index + 1)
                    };
                    make_document(&source_path, &path_text, title, &heading, body, "", "", "")
                })
                .collect()
        }
    }
}
