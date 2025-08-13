//! Handles loading and saving of application configuration.
//!
//! The `Config` struct is the main entry point for application settings.
//! It is designed to be easily extensible. To add a new setting, you can
//! simply add a new field to the `Config` struct.
//!
//! The configuration is saved in a `config.json` file in the user's
//! config directory (e.g., `~/.config/livenac/` on Linux).

use eyre::{eyre, Context};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Represents the application's configuration.
///
/// To add a new configuration option (e.g., a list of favorite channels):
/// 1. Add a new field to this struct:
///    ```
///    pub favorite_channels: Vec<String>,
///    ```
/// 2. Initialize it with a default value in the `default()` implementation below.
/// 3. The `load` and `save` functions will automatically handle serialization.
/// 4. You can then access this new option from your `Config` object in the UI
///    and create UI elements to modify it.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub client_id: Option<String>,
    pub enable_cjk_font: bool,
    pub font_size: f32,
    // EXAMPLE: How to add a new setting.
    // pub favorite_channels: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            client_id: None,
            enable_cjk_font: false,
            font_size: 14.0,
        }
    }
}

/// Gets the path to the configuration file (`config.json`).
fn get_config_path() -> Result<PathBuf, eyre::Report> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| eyre!("Could not find a config directory"))?
        .join(env!("CARGO_PKG_NAME"));

    Ok(config_dir.join("config.json"))
}

/// Asynchronously loads the application configuration from disk.
///
/// If the file does not exist, it returns the default configuration.
pub async fn load() -> Result<Config, eyre::Report> {
    let path = get_config_path()?;
    tracing::info!("Loading config from {:?}", path);

    // If the file doesn't exist, return a default config.
    // This is expected on the first run.
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        tracing::info!("Config file not found, using default.");
        return Ok(Config::default());
    };

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .await
        .context("Could not read config file")?;

    let config: Config =
        serde_json::from_slice(&buffer).context("Could not parse config file")?;

    Ok(config)
}

/// Asynchronously saves the application configuration to disk.
pub async fn save(config: &Config) -> Result<(), eyre::Report> {
    let path = get_config_path()?;
    tracing::info!("Saving config to {:?}", path);

    let bytes =
        serde_json::to_vec_pretty(config).context("Failed to serialize config")?;

    // Ensure the parent directory exists.
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create config directory")?;
        }
    }

    let mut file = tokio::fs::File::create(path)
        .await
        .context("Failed to create config file")?;

    file.write_all(&bytes)
        .await
        .context("Failed to write config to file")?;

    Ok(())
}
