use crate::events::app_event::AppEvent;
use dirs;
use eyre::{eyre, Context};
use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, service::service_fn, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use reqwest::Client as ReqwestClient;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, oneshot},
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
    ui_message_tx: mpsc::Sender<AppEvent>,
}

impl AuthClient {
    pub fn new(
        client_id: String,
        ui_message_tx: mpsc::Sender<AppEvent>,
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
            std::fs::create_dir_all(&data_path).context("Failed to create config directory")?;
        }

        Ok(Self {
            reqwest_client,
            scopes,
            client_id: twitch_oauth2::ClientId::new(client_id),
            data_path,
            ui_message_tx,
        })
    }

    pub async fn authenticate(self) {
        tracing::info!("Starting token acquisition process...");
        match self.load_and_validate_token().await {
            Ok(token) => {
                tracing::info!("Successfully loaded and validated a token.");
                self.send_message(AuthMessage::Success(token)).await;
            }
            Err(e) => {
                tracing::warn!(
                    "Could not load a valid token ({}). Starting browser flow.",
                    e
                );
                if let Err(e) = self.run_browser_flow().await {
                    tracing::error!("Browser flow failed: {}", e);
                    self.send_message(AuthMessage::Error(format!("Authentication failed: {}", e)))
                        .await;
                }
            }
        }
    }

    async fn load_and_validate_token(&self) -> Result<UserToken, eyre::Report> {
        tracing::info!("Attempting to load token from disk...");
        let stored = self.load_token_from_disk().await?;
        let access_token = AccessToken::new(stored.access_token);

        let validated = access_token
            .validate_token(&self.reqwest_client)
            .await
            .context("Failed to validate token")?;

        let token = UserToken::new(access_token, None::<RefreshToken>, validated, None)?;

        tracing::info!("Token is valid.");
        Ok(token)
    }

    async fn run_browser_flow(&self) -> Result<(), eyre::Report> {
        let (token_tx, token_rx) = oneshot::channel();

        let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
        let listener = TcpListener::bind(addr).await?;
        let server_handle = tokio::spawn(run_server(listener, token_tx));

        let scope_str = self
            .scopes
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let auth_url = Url::parse_with_params(
            "https://id.twitch.tv/oauth2/authorize",
            &[
                ("response_type", "token"),
                ("client_id", self.client_id.as_str()),
                ("redirect_uri", REDIRECT_URI),
                ("scope", &scope_str),
            ],
        )?;

        if webbrowser::open(auth_url.as_str()).is_err() {
            tracing::error!(
                "Failed to open browser. Please navigate to this URL manually: {}",
                auth_url
            );
        }

        tokio::select! {
            token_result = token_rx => {
                server_handle.abort();
                match token_result {
                    Ok(token) => {
                        let validated = token.validate_token(&self.reqwest_client).await?;
                        let full_token = UserToken::new(token.clone(), None::<RefreshToken>, validated, None)?;
                        self.save_token_to_disk(&token).await?;
                        self.send_message(AuthMessage::Success(full_token)).await;
                        Ok(())
                    }
                    Err(_) => {
                        let err_msg = "The authentication server was closed before a token was received.".to_string();
                        self.send_message(AuthMessage::Error(err_msg.clone())).await;
                        Err(eyre!(err_msg))
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(180)) => {
                server_handle.abort();
                let err_msg = "Login timed out after 3 minutes.".to_string();
                self.send_message(AuthMessage::Error(err_msg.clone())).await;
                Err(eyre!(err_msg))
            }
        }
    }

    async fn save_token_to_disk(&self, token: &AccessToken) -> Result<(), eyre::Report> {
        let path = self.data_path.join(TOKEN_FILE_NAME);
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
        let path = self.data_path.join(TOKEN_FILE_NAME);
        let mut file = tokio::fs::File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;
        let token: StoredToken =
            serde_json::from_slice(&buffer).context("Failed to deserialize token")?;
        Ok(token)
    }

    async fn send_message(&self, msg: AuthMessage) {
        if self.ui_message_tx.send(AppEvent::Auth(msg)).await.is_err() {
            tracing::error!("Failed to send message to UI thread: channel is closed.");
        }
    }
}

async fn run_server(listener: TcpListener, token_tx: oneshot::Sender<AccessToken>) {
    let token_tx = std::sync::Arc::new(tokio::sync::Mutex::new(Some(token_tx)));
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => return, // Listener was closed
        };
        let io = TokioIo::new(stream);
        let token_tx_clone = token_tx.clone();

        tokio::task::spawn(async move {
            let service = service_fn(move |req| handle_request(req, token_tx_clone.clone()));
            if let Err(err) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                tracing::debug!("Server connection error: {}", err);
            }
        });
    }
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    token_tx: std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Sender<AccessToken>>>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(Full::new(Bytes::from(HTML_LANDING_PAGE)))),
        (&Method::POST, "/token") => {
            let whole_body = req.into_body().collect().await?.to_bytes();
            let token_data: serde_json::Value = match serde_json::from_slice(&whole_body) {
                Ok(data) => data,
                Err(_) => {
                    let mut response = Response::new(Full::new(Bytes::from("Bad Request")));
                    *response.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(response);
                }
            };

            let access_token_str = token_data["access_token"].as_str().unwrap_or_default();
            let access_token = twitch_oauth2::AccessToken::new(access_token_str.to_string());

            if let Some(tx) = token_tx.lock().await.take() {
                if tx.send(access_token).is_ok() {
                    return Ok(Response::new(Full::new(Bytes::from("OK"))));
                }
            }

            let mut response = Response::new(Full::new(Bytes::from("Internal Server Error")));
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            Ok(response)
        }
        _ => {
            let mut not_found = Response::new(Full::new(Bytes::from("Not Found")));
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}
