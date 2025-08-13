use dirs;
use eyre::{eyre, Context};
use reqwest::Client as ReqwestClient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use twitch_oauth2::{DeviceUserTokenBuilder, UserToken};

const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const TOKEN_FILE_NAME: &str = "token.json";

/// A message from the auth task to the UI thread.
#[derive(Debug)]
pub enum AuthMessage {
    /// Prompt the user to authorize with the given URI and code.
    AwaitingDeviceActivation { uri: String, user_code: String },
    /// Authentication was successful.
    Success(UserToken),
    /// An error occurred.
    Error(String),
}

/// A simplified token structure for serialization.
#[derive(Serialize, Deserialize, Debug)]
struct StoredToken {
    access_token: twitch_oauth2::AccessToken,
    refresh_token: twitch_oauth2::RefreshToken,
}

/// The client responsible for handling authentication.
#[derive(Clone)]
pub struct AuthClient {
    reqwest_client: ReqwestClient,
    scopes: Vec<twitch_oauth2::Scope>,
    client_id: twitch_oauth2::ClientId,
    data_path: PathBuf,
    auth_message_tx: mpsc::Sender<AuthMessage>,
}

impl AuthClient {
    pub fn new(
        client_id: String,
        auth_message_tx: mpsc::Sender<AuthMessage>,
    ) -> Result<Self, eyre::Report> {
        let reqwest_client = ReqwestClient::builder()
            .user_agent(APP_USER_AGENT)
            .build()?;

        let scopes = vec![
            twitch_oauth2::Scope::ChatRead,
            twitch_oauth2::Scope::ChatEdit,
            twitch_oauth2::Scope::UserReadChat,
            twitch_oauth2::Scope::UserWriteChat,
            twitch_oauth2::Scope::ModeratorManageAnnouncements,
            twitch_oauth2::Scope::ModeratorReadChatters,
            twitch_oauth2::Scope::UserReadEmotes,
        ];

        let data_path = dirs::config_dir()
            .ok_or_else(|| eyre!("Could not find a config directory"))?
            .join(env!("CARGO_PKG_NAME"));
        if !data_path.exists() {
            std::fs::create_dir_all(&data_path).context("Failed to create config directory")?;
        }

        Ok(Self {
            reqwest_client,
            scopes,
            client_id: twitch_oauth2::ClientId::new(client_id),
            data_path,
            auth_message_tx,
        })
    }

    /// The main entry point for authentication.
    pub async fn get_or_refresh_token(self) {
        tracing::info!("Starting token acquisition process...");
        match self.load_and_validate_token().await {
            Ok(token) => {
                tracing::info!("Successfully loaded and validated a token.");
                let _ = self.save_token_to_disk(&token).await; // Save the potentially refreshed token
                self.send_message(AuthMessage::Success(token)).await;
            }
            Err(e) => {
                tracing::warn!(
                    "Could not load a valid token ({}). Starting device flow.",
                    e
                );
                if let Err(e) = self.run_device_flow().await {
                    tracing::error!("Device flow failed: {}", e);
                    self.send_message(AuthMessage::Error(format!("Authentication failed: {}", e)))
                        .await;
                }
            }
        }
    }

    /// Tries to load a token from disk and validate it. Refreshes if expired.
    async fn load_and_validate_token(&self) -> Result<UserToken, eyre::Report> {
        tracing::info!("Attempting to load token from disk...");
        let stored = self.load_token_from_disk().await?;

        let token = UserToken::from_existing_or_refresh_token(
            &self.reqwest_client,
            stored.access_token,
            stored.refresh_token,
            self.client_id.clone(),
            None, // No client secret for public clients
        )
        .await
        .context("Failed to validate or refresh token")?;

        tracing::info!("Token is valid.");
        Ok(token)
    }

    /// Executes the full Device Code Flow.
    async fn run_device_flow(&self) -> Result<(), eyre::Report> {
        tracing::info!("Starting new device flow...");
        let mut builder =
            DeviceUserTokenBuilder::new(self.client_id.clone(), self.scopes.clone());

        let code = builder
            .start(&self.reqwest_client)
            .await
            .context("Failed to start device flow")?;

        tracing::info!(
            "Device flow started. URI: {}, Code: {}",
            code.verification_uri,
            code.user_code
        );

        self.send_message(AuthMessage::AwaitingDeviceActivation {
            uri: code.verification_uri.clone(),
            user_code: code.user_code.clone(),
        })
        .await;

        let token = builder
            .wait_for_code(&self.reqwest_client, tokio::time::sleep)
            .await
            .context("Failed to get token from device flow")?;
        tracing::info!("Successfully received token from device flow.");

        self.save_token_to_disk(&token).await?;

        self.send_message(AuthMessage::Success(token)).await;

        Ok(())
    }

    /// Saves the UserToken to a file on disk asynchronously.
    async fn save_token_to_disk(&self, token: &UserToken) -> Result<(), eyre::Report> {
        let path = self.data_path.join(TOKEN_FILE_NAME);
        tracing::info!("Saving token to {:?}", path);
        let stored_token = StoredToken {
            access_token: token.access_token.clone(),
            refresh_token: token
                .refresh_token
                .clone()
                .ok_or_else(|| eyre!("Cannot save a token without a refresh token"))?,
        };

        let bytes = serde_json::to_vec_pretty(&stored_token)
            .context("Failed to serialize token")?;

        if !self.data_path.exists() {
            tokio::fs::create_dir_all(&self.data_path).await.context("Failed to create config directory")?;
        }
        
        let mut file = tokio::fs::File::create(path)
            .await
            .context("Failed to create token file")?;

        file.write_all(&bytes)
            .await
            .context("Failed to write token to file")?;

        Ok(())
    }

    /// Loads a UserToken from a file on disk asynchronously.
    async fn load_token_from_disk(&self) -> Result<StoredToken, eyre::Report> {
        let path = self.data_path.join(TOKEN_FILE_NAME);
        tracing::info!("Loading token from {:?}", path);

        let mut file = tokio::fs::File::open(path)
            .await
            .context("Could not open token file")?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .context("Could not read token file")?;

        let token: StoredToken =
            serde_json::from_slice(&buffer).context("Could not parse token file")?;

        Ok(token)
    }

    /// Helper to send a message to the UI thread.
    async fn send_message(&self, msg: AuthMessage) {
        if self.auth_message_tx.send(msg).await.is_err() {
            tracing::error!("Failed to send message to UI thread: channel is closed.");
        }
    }
}
