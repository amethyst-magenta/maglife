use serde::Deserialize;
use std::{env, fs, io, path::PathBuf};

#[derive(Debug, Deserialize)]
struct FileConfig {
    database_url: Option<String>,
}

pub fn database_url() -> Result<String, String> {
    if let Ok(url) = env::var("DATABASE_URL") {
        return non_empty(url, "DATABASE_URL");
    }

    let Some(path) = config_path() else {
        return Ok("postgresql:///maglife".into());
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok("postgresql:///maglife".into());
        }
        Err(error) => {
            return Err(format!("Не удалось прочитать {}: {error}", path.display()));
        }
    };
    let config: FileConfig =
        toml::from_str(&text).map_err(|error| format!("Ошибка в {}: {error}", path.display()))?;
    config
        .database_url
        .ok_or_else(|| format!("В {} нет параметра database_url.", path.display()))
        .and_then(|url| non_empty(url, "database_url"))
}

pub fn config_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(path).join("maglife/config.toml"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/maglife/config.toml"))
}

fn non_empty(value: String, source: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err(format!("Параметр {source} пустой."))
    } else {
        Ok(value)
    }
}
