use crate::events::app_event::AppEvent;
use dirs;
use eyre::{eyre, Context};
use http_body_util::Full;
use hyper::{body::Bytes, server::conn::http1, service::service_fn, Method, Response, StatusCode};
use hyper_util::rt::TokioIo;
use reqwest::Client as ReqwestClient;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
};
use twitch_oauth2::{AccessToken, RefreshToken, UserToken};
use url::Url;

const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const TOKEN_FILE_NAME: &str = "token.json";
const REDIRECT_URI: &str = "http://localhost:3000";

const HTML_LANDING_PAGE: &str = include_str!("success.html");

#[derive(Debug)]
pub enum AuthMessage {
    Success(UserToken),
    Error(String),
}

#[derive(Serialize, Deserialize)]
struct StoredToken {
    access_token: String,
}

#[derive(Clone)]
pub struct AuthClient {
    reqwest_client: ReqwestClient,
    scopes: Vec<twitch_oauth2::Scope>,
    client_id: twitch_oauth2::ClientId,
    data_path: PathBuf,
    // No longer needed for interactive flow, but might be useful for silent flow errors.
    ui_message_tx: mpsc::Sender<AppEvent>,
    active_profile_name: Option<String>,
}

impl AuthClient {
    pub async fn new(
        client_id: String,
        ui_message_tx: mpsc::Sender<AppEvent>,
        active_profile_name: Option<String>,
    ) -> Result<Self, eyre::Report> {
        let reqwest_client = ReqwestClient::builder()
            .user_agent(APP_USER_AGENT)
            .timeout(Duration::from_secs(15))
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
            tokio::fs::create_dir_all(&data_path)
                .await
                .context("Failed to create config directory")?;
        }

        Ok(Self {
            reqwest_client,
            scopes,
            client_id: twitch_oauth2::ClientId::new(client_id),
            data_path,
            ui_message_tx,
            active_profile_name,
        })
    }

    /// Attempts to load and validate a token from disk for the active profile.
    pub async fn try_silent_login(&self) -> Result<UserToken, eyre::Report> {
        if self.active_profile_name.is_none() {
            return Err(eyre!("No active profile selected."));
        }
        self.load_and_validate_token().await
    }

    /// Kicks off the interactive login flow.
    /// This will start a short-lived local server and open the user's browser.
    /// The app's UI is responsible for changing state to wait for the user to paste the token.
    pub async fn start_interactive_login(self) -> Result<(), eyre::Report> {
        tracing::info!("Starting interactive login flow...");

        let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to bind to port 3000. Is another instance of the app running? Error: {}", e);
                return Err(eyre!("Could not start local server on port 3000. It might already be in use. OS error: {}", e));
            }
        };

        // Spawn the server. It will shut down on its own after one connection.
        tokio::spawn(run_single_use_server(listener));

        let scope_str = self
            .scopes
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let params = vec![
            ("response_type", "token"),
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", REDIRECT_URI),
            ("scope", &scope_str),
            ("force_verify", "true"),
        ];

        let auth_url = Url::parse_with_params("https://id.twitch.tv/oauth2/authorize", &params)?;

        if webbrowser::open(auth_url.as_str()).is_err() {
            tracing::error!(
                "Failed to open browser. Please navigate to this URL manually: {}",
                auth_url
            );
        }

        Ok(())
    }

    /// Validates a token that was pasted by the user.
    pub async fn validate_pasted_token(&self, token_str: String) -> Result<UserToken, eyre::Report> {
        let access_token = AccessToken::new(token_str);

        let validated = tokio::time::timeout(
            Duration::from_secs(10),
            access_token.validate_token(&self.reqwest_client),
        )
        .await
        .context("Token validation timed out")?
        .context("Failed to validate token")?;

        let token = UserToken::new(access_token, None::<RefreshToken>, validated, None)?;

        tracing::info!("Pasted token is valid.");
        Ok(token)
    }
    
    pub async fn save_token(&self, token: &UserToken) -> Result<(), eyre::Report> {
        self.save_token_to_disk(&token.access_token).await
    }

    async fn load_and_validate_token(&self) -> Result<UserToken, eyre::Report> {
        tracing::info!("Attempting to load token from disk...");
        let stored = self.load_token_from_disk().await?;
        let access_token = AccessToken::new(stored.access_token);

        let validated = tokio::time::timeout(
            Duration::from_secs(10),
            access_token.validate_token(&self.reqwest_client),
        )
        .await
        .context("Token validation timed out")?
        .context("Failed to validate token")?;

        let token = UserToken::new(access_token, None::<RefreshToken>, validated, None)?;

        tracing::info!("Token is valid.");
        Ok(token)
    }

    fn get_token_path(&self) -> Result<PathBuf, eyre::Report> {
        let profile_name = self
            .active_profile_name
            .as_ref()
            .ok_or_else(|| eyre!("Cannot get token path, no active profile"))?;
        Ok(self
            .data_path
            .join("profiles")
            .join(profile_name)
            .join(TOKEN_FILE_NAME))
    }

    async fn save_token_to_disk(&self, token: &AccessToken) -> Result<(), eyre::Report> {
        let path = self.get_token_path()?;
        let stored_token = StoredToken {
            access_token: token.secret().to_string(),
        };
        let bytes = serde_json::to_vec_pretty(&stored_token)?;
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(&bytes).await?;
        Ok(())
    }

    async fn load_token_from_disk(&self) -> Result<StoredToken, eyre::Report> {
        let path = self.get_token_path()?;
        let mut file = tokio::fs::File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;
        let token: StoredToken =
            serde_json::from_slice(&buffer).context("Failed to deserialize token")?;
        Ok(token)
    }

    #[allow(dead_code)]
    async fn send_message(&self, msg: AuthMessage) {
        if self.ui_message_tx.send(AppEvent::Auth(msg)).await.is_err() {
            tracing::error!("Failed to send message to UI thread: channel is closed.");
        }
    }
}

/// A web server that accepts only one connection, serves the success page, and then shuts down.
async fn run_single_use_server(listener: TcpListener) {
    if let Ok((stream, _)) = listener.accept().await {
        let io = TokioIo::new(stream);

        let service = service_fn(move |req| async move {
            match (req.method(), req.uri().path()) {
                (&Method::GET, "/") => {
                    Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from(HTML_LANDING_PAGE))))
                }
                _ => {
                    let mut not_found = Response::new(Full::new(Bytes::from("Not Found")));
                    *not_found.status_mut() = StatusCode::NOT_FOUND;
                    Ok(not_found)
                }
            }
        });

        if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
            tracing::debug!("Server connection error: {}", err);
        }
    }
    tracing::info!("Single-use server shutting down.");
}
