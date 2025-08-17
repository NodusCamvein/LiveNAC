use eyre::{Context, eyre};
use figment::{
    Figment,
    providers::{Format, Toml},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Profile {
    pub name: String,
    pub twitch_user_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
    pub client_id: Option<String>,
    pub enable_cjk_font: bool,
    pub font_size: f32,
    pub emote_size: f32,
    pub show_timestamps: bool,
    pub collapse_emotes: bool,
    pub profiles: Vec<Profile>,
    pub active_profile_name: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            client_id: None,
            enable_cjk_font: false,
            font_size: 14.0,
            emote_size: 14.0,
            show_timestamps: false,
            collapse_emotes: false,
            profiles: Vec::new(),
            active_profile_name: None,
        }
    }
}

impl Config {
    pub fn get_active_profile(&self) -> Option<&Profile> {
        match &self.active_profile_name {
            Some(name) => self.profiles.iter().find(|p| &p.name == name),
            None => None,
        }
    }
}

fn get_config_path() -> Result<PathBuf, eyre::Report> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| eyre!("Could not find a config directory"))?
        .join(env!("CARGO_PKG_NAME"));

    Ok(config_dir.join("app_config.toml"))
}

pub async fn load() -> Result<Config, eyre::Report> {
    let user_config_path = get_config_path()?;
    tracing::info!("Loading user config from {:?}", user_config_path);

    let base_config_path = "config/app_config.toml";
    tracing::info!("Loading base config from {:?}", base_config_path);

    let config: Config = Figment::new()
        .merge(Toml::file(base_config_path))
        .merge(Toml::file(&user_config_path))
        .extract()
        .context("Could not load config")?;

    if !user_config_path.exists() {
        if let Err(e) = save(&config).await {
            tracing::warn!("Failed to save initial config: {}", e);
        }
    }

    Ok(config)
}

pub async fn save(config: &Config) -> Result<(), eyre::Report> {
    let path = get_config_path()?;
    tracing::info!("Saving config to {:?}", path);

    let bytes = toml::to_string_pretty(config).context("Failed to serialize config")?;

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

    file.write_all(bytes.as_bytes())
        .await
        .context("Failed to write config to file")?;

    Ok(())
}