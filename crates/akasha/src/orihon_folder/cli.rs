use super::{Book, Result, fold_page, model::FolderConfig};
use crate::Config;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

const HELP: &str = "orihon-fold --room ROOM --turn-no N --model TAG [--env-file PATH | --book-dir PATH] [--endpoint URL] [--num-ctx N]\n\nFold one page snapshot with a local Ollama model. The book directory is <room>/orihon.\nThis command stores derived provenance. It does not change capture mode or enable wake injection.\nThe database must already have the current migrations. Run deployment through the normal guarded path.";

fn arguments(args: &[String]) -> Result<BTreeMap<&str, &str>> {
    if args.len() % 2 != 0 {
        return Err("orihon-fold: every option requires a value".into());
    }
    let mut values = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        let key = pair[0].as_str();
        if !matches!(
            key,
            "--room"
                | "--turn-no"
                | "--model"
                | "--env-file"
                | "--book-dir"
                | "--endpoint"
                | "--num-ctx"
        ) {
            return Err(format!("orihon-fold: unknown option {key}").into());
        }
        if pair[1].is_empty() || values.insert(key, pair[1].as_str()).is_some() {
            return Err(format!("orihon-fold: empty or repeated option {key}").into());
        }
    }
    for key in ["--room", "--turn-no", "--model"] {
        if !values.contains_key(key) {
            return Err(format!("orihon-fold: missing {key}").into());
        }
    }
    if values.contains_key("--book-dir") && values.contains_key("--env-file") {
        return Err("orihon-fold: select either --book-dir or --env-file".into());
    }
    Ok(values)
}

/// Run one operator-selected fold without starting a background worker or applying migrations.
pub async fn run(args: &[String]) -> Result<()> {
    if args == ["--help"] {
        println!("{HELP}");
        return Ok(());
    }
    let values = arguments(args)?;
    let turn_no: i64 = values["--turn-no"]
        .parse()
        .map_err(|_| "orihon-fold: invalid page ordinal")?;
    let num_ctx: u32 = values
        .get("--num-ctx")
        .copied()
        .unwrap_or("32768")
        .parse()
        .map_err(|_| "orihon-fold: invalid context size")?;

    if !(4096..=262_144).contains(&num_ctx) {
        return Err("orihon-fold: --num-ctx must be between 4096 and 262144".into());
    }
    let model = FolderConfig {
        endpoint: values
            .get("--endpoint")
            .copied()
            .unwrap_or("http://127.0.0.1:11434")
            .parse()?,
        model: values["--model"].to_owned(),
        timeout: Duration::from_secs(120),
        num_ctx,
    };
    let book = if let Some(directory) = values.get("--book-dir") {
        Book::File(PathBuf::from(directory))
    } else {
        let config = match values.get("--env-file") {
            Some(path) => Config::from_env_file(std::path::Path::new(path))?,
            None => Config::from_env()?,
        };
        Book::Postgres(config.pool().await?)
    };
    let receipt = fold_page(&book, &model, values["--room"], turn_no).await?;
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}
