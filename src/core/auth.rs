use crate::events::app_event::AppEvent;
use dirs;
use eyre::{eyre, Context};
use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
    Request, Response,
};
use hyper_util::rt::TokioIo;
use reqwest::Client as ReqwestClient;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, oneshot},
};
use twitch_oauth2::{AccessToken, RefreshToken, UserToken, UserTokenBuilder};
use url::{form_urlencoded, Url};

const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const TOKEN_FILE_NAME: &str = "token.json";
const REDIRECT_URI: &str = "http://localhost:3000";
const HTML_LANDING_PAGE: &str = "<html><head><title>LiveNAC Authentication</title></head><body>Success! You can close this window now.</body></html>";

#[derive(Debug)]
pub enum AuthMessage {
    Success(UserToken),
    Error(String),
}

#[derive(Serialize, Deserialize)]
struct StoredToken {
    access_token: AccessToken,
    refresh_token: Option<RefreshToken>,
}

#[derive(Clone)]
pub struct AuthClient {
    reqwest_client: ReqwestClient,
    scopes: Vec<twitch_oauth2::Scope>,
    client_id: twitch_oauth2::ClientId,
    client_secret: twitch_oauth2::ClientSecret,
    data_path: PathBuf,
    // No longer needed for interactive flow, but might be useful for silent flow errors.
    ui_message_tx: mpsc::Sender<AppEvent>,
    active_profile_name: Option<String>,
}

impl AuthClient {
    pub async fn new(
        client_id: String,
        client_secret: String,
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
            client_secret: twitch_oauth2::ClientSecret::new(client_secret),
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

    /// Kicks off the interactive login flow using the Authorization Code Grant Flow.
    /// This will start a short-lived local server and open the user's browser.
    pub async fn start_interactive_login(self) -> Result<UserToken, eyre::Report> {
        tracing::info!("Starting interactive login flow...");

        let redirect_uri = Url::parse(REDIRECT_URI).expect("is known-good").into();

        let mut builder =
            UserTokenBuilder::new(self.client_id.clone(), self.client_secret.clone(), redirect_uri)
                .set_scopes(self.scopes.clone())
                .force_verify(true);

        let (auth_url, _csrf_token) = builder.generate_url();

        let (tx, rx) = oneshot::channel();

        // Spawn the server to listen for the redirect
        tokio::spawn(run_single_use_auth_server(tx));

        if webbrowser::open(auth_url.as_str()).is_err() {
            tracing::error!(
                "Failed to open browser. Please navigate to this URL manually: {}",
                auth_url
            );
        }

        // Wait for the server to receive the code
        let (code, state) = rx.await.context("Auth server task failed")?;

        // The builder internally checks the CSRF token (state)
        let token = builder
            .get_user_token(&self.reqwest_client, code.as_str(), &state)
            .await
            .context("Failed to exchange code for token")?;

        tracing::info!("Successfully received token via interactive flow.");
        Ok(token)
    }

    pub async fn save_token(&self, token: &UserToken) -> Result<(), eyre::Report> {
        let stored_token = StoredToken {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&stored_token)?;
        let path = self.get_token_path()?;
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(&bytes).await?;
        Ok(())
    }

    async fn load_and_validate_token(&self) -> Result<UserToken, eyre::Report> {
        tracing::info!("Attempting to load token from disk...");
        let stored = self.load_token_from_disk().await?;

        let token = UserToken::from_existing(
            &self.reqwest_client,
            stored.access_token,
            stored.refresh_token,
            Some(self.client_secret.clone()),
        )
        .await
        .context("Failed to validate and/or refresh token")?;

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
async fn run_single_use_auth_server(tx: oneshot::Sender<(String, String)>) {
    let tx = Arc::new(Mutex::new(Some(tx)));

    let service = service_fn(move |req: Request<Incoming>| {
        let tx = tx.clone();
        async move {
            let mut code = String::new();
            let mut state = String::new();

            if let Some(q) = req.uri().query() {
                for (k, v) in form_urlencoded::parse(q.as_bytes()) {
                    match k.as_ref() {
                        "code" => code = v.to_string(),
                        "state" => state = v.to_string(),
                        _ => {}
                    }
                }
            }

            if let Some(tx) = tx.lock().unwrap().take() {
                if tx.send((code, state)).is_err() {
                    tracing::error!("oneshot channel receiver dropped before being used");
                }
            }

            Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from(HTML_LANDING_PAGE))))
        }
    });

    let addr: std::net::SocketAddr = ([127, 0, 0, 1], 3000).into();

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind to port 3000: {}", e);
            return;
        }
    };

    if let Ok((stream, _)) = listener.accept().await {
        let io = TokioIo::new(stream);

        if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
            tracing::error!("Server connection error: {}", err);
        }
    }
}
