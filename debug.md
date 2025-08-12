use std::sync::Arc;

use eyre::Context;
use tokio_tungstenite::tungstenite;
use tracing::Instrument;
use twitch_api::{
    eventsub::{
        self,
        event::websocket::{EventsubWebsocketData, ReconnectPayload, SessionData, WelcomePayload},
        Event,
    },
    types::{self},
    HelixClient,
};
use twitch_oauth2::{TwitchToken, UserToken};

pub struct WebsocketClient {
    /// The session id of the websocket connection
    pub session_id: Option<String>,
    /// The token used to authenticate with the Twitch API
    pub token: UserToken,
    /// The client used to make requests to the Twitch API
    pub client: HelixClient<'static, reqwest::Client>,
    /// The user id of the channel we want to listen to
    pub user_id: types::UserId,
    /// The url to use for websocket
    pub connect_url: url::Url,
    pub opts: Arc<crate::Opts>,
}

impl WebsocketClient {
    /// Connect to the websocket and return the stream
    pub async fn connect(
        &self,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        eyre::Error,
    > {
        tracing::info!("connecting to twitch");
        let config = tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(64 << 20), // 64 MiB
            max_frame_size: Some(16 << 20),   // 16 MiB
            accept_unmasked_frames: false,
            ..tungstenite::protocol::WebSocketConfig::default()
        };
        let (socket, _) =
            tokio_tungstenite::connect_async_with_config(&self.connect_url, Some(config), false)
                .await
                .context("Can't connect")?;

        Ok(socket)
    }

    /// Run the websocket subscriber
    #[tracing::instrument(name = "subscriber", skip_all, fields())]
    pub async fn run(mut self) -> Result<(), eyre::Error> {
        // Establish the stream
        let mut s = self
            .connect()
            .await
            .context("when establishing connection")?;
        // Loop over the stream, processing messages as they come in.
        loop {
            tokio::select!(
            Some(msg) = futures::StreamExt::next(&mut s) => {
                let span = tracing::info_span!("message received", raw_message = ?msg);
                let msg = match msg {
                    Err(tungstenite::Error::Protocol(
                        tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                    )) => {
                        tracing::warn!(
                            "connection was sent an unexpected frame or was reset, reestablishing it"
                        );
                        s = self
                            .connect().instrument(span)
                            .await
                            .context("when reestablishing connection")?;
                        continue
                    }
                    _ => msg.context("when getting message")?,
                };
                self.process_message(msg).instrument(span).await?
            })
        }
    }

    /// Process a message from the websocket
    pub async fn process_message(&mut self, msg: tungstenite::Message) -> Result<(), eyre::Report> {
        match msg {
            tungstenite::Message::Text(s) => {
                tracing::info!("{s}");
                // Parse the message into a [twitch_api::eventsub::EventsubWebsocketData]
                match Event::parse_websocket(&s)? {
                    EventsubWebsocketData::Welcome {
                        payload: WelcomePayload { session },
                        ..
                    }
                    | EventsubWebsocketData::Reconnect {
                        payload: ReconnectPayload { session },
                        ..
                    } => {
                        self.process_welcome_message(session).await?;
                        Ok(())
                    }
                    // Here is where you would handle the events you want to listen to
                    EventsubWebsocketData::Notification {
                        metadata: _,
                        payload,
                    } => {
                        match payload {
                            Event::ChannelBanV1(eventsub::Payload { message, .. }) => {
                                tracing::info!(?message, "got ban event");
                            }
                            Event::ChannelUnbanV1(eventsub::Payload { message, .. }) => {
                                tracing::info!(?message, "got ban event");
                            }
                            _ => {}
                        }
                        Ok(())
                    }
                    EventsubWebsocketData::Revocation {
                        metadata,
                        payload: _,
                    } => eyre::bail!("got revocation event: {metadata:?}"),
                    EventsubWebsocketData::Keepalive {
                        metadata: _,
                        payload: _,
                    } => Ok(()),
                    _ => Ok(()),
                }
            }
            tungstenite::Message::Close(_) => todo!(),
            _ => Ok(()),
        }
    }

    pub async fn process_welcome_message(
        &mut self,
        data: SessionData<'_>,
    ) -> Result<(), eyre::Report> {
        self.session_id = Some(data.id.to_string());
        if let Some(url) = data.reconnect_url {
            self.connect_url = url.parse()?;
        }
        // check if the token is expired, if it is, request a new token. This only works if using a oauth service for getting a token
        if self.token.is_elapsed() {
            self.token =
                crate::util::get_access_token(self.client.get_client(), &self.opts).await?;
        }
        let transport = eventsub::Transport::websocket(data.id.clone());
        self.client
            .create_eventsub_subscription(
                eventsub::channel::ChannelBanV1::broadcaster_user_id(self.user_id.clone()),
                transport.clone(),
                &self.token,
            )
            .await?;
        self.client
            .create_eventsub_subscription(
                eventsub::channel::ChannelUnbanV1::broadcaster_user_id(self.user_id.clone()),
                transport,
                &self.token,
            )
            .await?;
        tracing::info!("listening to ban and unbans");
        Ok(())
    }
}

use eyre::Context;
use twitch_oauth2::UserToken;

/// Setup dotenv, tracing and error reporting with eyre
pub fn install_utils() -> eyre::Result<()> {
    let _ = dotenvy::dotenv(); //ignore error
    install_tracing();
    install_eyre()?;
    Ok(())
}

/// Install eyre and setup a panic hook
fn install_eyre() -> eyre::Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();

    eyre_hook.install()?;

    std::panic::set_hook(Box::new(move |pi| {
        tracing::error!("{}", panic_hook.panic_report(pi));
    }));
    Ok(())
}
/// Install tracing with a specialized filter
fn install_tracing() {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_target(true);
    #[rustfmt::skip]
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .map(|f| {
            // common filters which can be very verbose
            f.add_directive("hyper=error".parse().expect("could not make directive"))
                .add_directive("h2=error".parse().expect("could not make directive"))
                .add_directive("rustls=error".parse().expect("could not make directive"))
                .add_directive("tungstenite=error".parse().expect("could not make directive"))
                .add_directive("retainer=info".parse().expect("could not make directive"))
                .add_directive("want=info".parse().expect("could not make directive"))
                .add_directive("reqwest=info".parse().expect("could not make directive"))
                .add_directive("mio=info".parse().expect("could not make directive"))
            //.add_directive("tower_http=error".parse().unwrap())
        })
        .expect("could not make filter layer");

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();
}

/// Create a new [UserToken] from an [AccessToken]
#[tracing::instrument(skip(client, token))]
pub async fn make_token<'a>(
    client: &'a impl twitch_oauth2::client::Client,
    token: impl Into<twitch_oauth2::AccessToken>,
) -> Result<UserToken, eyre::Report> {
    UserToken::from_token(client, token.into())
        .await
        .context("could not get/make access token")
        .map_err(Into::into)
}

/// Get an access token from either the cli, dotenv (via [clap::Arg::env]) or an oauth service
#[tracing::instrument(skip(client, opts))]
pub async fn get_access_token(
    client: &reqwest::Client,
    opts: &crate::Opts,
) -> Result<UserToken, eyre::Report> {
    if let Some(ref access_token) = opts.access_token {
        make_token(client, access_token.secret().to_string()).await
    } else if let (Some(ref oauth_service_url), Some(ref pointer)) =
        (&opts.oauth2_service_url, &opts.oauth2_service_pointer)
    {
        tracing::info!(
            "using oauth service on `{}` to get oauth token",
            oauth_service_url
        );

        let mut request = client.get(oauth_service_url.as_str());
        if let Some(ref key) = opts.oauth2_service_key {
            request = request.bearer_auth(key.secret());
        }
        let request = request.build()?;
        tracing::debug!("request: {:?}", request);

        match client.execute(request).await {
            Ok(response)
                if !(response.status().is_client_error()
                    || response.status().is_server_error()) =>
            {
                let service_response: serde_json::Value = response
                    .json()
                    .await
                    .context("when transforming oauth service response to json")?;
                make_token(
                    client,
                    service_response
                        .pointer(pointer)
                        .ok_or_else(|| eyre::eyre!("could not get a field on `{}`", pointer))?
                        .as_str()
                        .ok_or_else(|| eyre::eyre!("token is not a string"))?
                        .to_string(),
                )
                .await
            }
            Ok(response_error) => {
                let status = response_error.status();
                let error = response_error.text().await?;
                eyre::bail!(
                    "oauth service returned error code: {} with body: {:?}",
                    status,
                    error
                );
            }
            Err(e) => Err(e)
                .wrap_err_with(|| eyre::eyre!("calling oauth service on `{}`", &oauth_service_url)),
        }
    } else {
        panic!("got empty vals for token cli group")
    }
}

use clap::{builder::ArgPredicate, ArgGroup, Parser};

#[derive(Parser, Debug, Clone)]
#[clap(about, version,
    group = ArgGroup::new("token").multiple(false).required(false),
    group = ArgGroup::new("service").multiple(true).requires("oauth2_service_url"),
    group = ArgGroup::new("channel").multiple(true).required(false),
)]
pub struct Opts {
    /// OAuth2 Access token
    #[clap(long, env, hide_env = true, group = "token", value_parser = to_token, required_unless_present = "service"
    )]
    pub access_token: Option<Secret>,
    /// Name of channel to monitor. If left out, defaults to owner of access token.
    #[clap(long, env, hide_env = true, group = "channel")]
    pub channel_login: Option<String>,
    /// User ID of channel to monitor. If left out, defaults to owner of access token.
    #[clap(long, env, hide_env = true, group = "channel")]
    pub channel_id: Option<String>,
    /// URL to service that provides OAuth2 token. Called on start and whenever the token needs to be refreshed.
    ///
    /// This application does not do any refreshing of tokens.
    #[clap(long, env, hide_env = true, group = "service",
        value_parser = url::Url::parse, required_unless_present = "token"
        )]
    pub oauth2_service_url: Option<url::Url>,
    /// Bearer key for authorizing on the OAuth2 service url.
    #[clap(long, env, hide_env = true, group = "service")]
    pub oauth2_service_key: Option<Secret>,
    /// Grab token by pointer. See https://tools.ietf.org/html/rfc6901
    #[clap(
        long,
        env,
        hide_env = true,
        group = "service",
        default_value_if("oauth2_service_url", ArgPredicate::IsPresent, Some("/access_token"))
    )]
    pub oauth2_service_pointer: Option<String>,
    /// Grab a new token from the OAuth2 service this many seconds before it actually expires. Default is 30 seconds
    #[clap(
        long,
        env,
        hide_env = true,
        group = "service",
        default_value_if("oauth2_service_url", ArgPredicate::IsPresent, Some("30"))
    )]
    pub oauth2_service_refresh: Option<u64>,
}

pub fn to_token(s: &str) -> eyre::Result<Secret> {
    if s.starts_with("oauth:") {
        eyre::bail!("token should not have `oauth:` as a prefix")
    }
    if s.len() != 30 {
        eyre::bail!("token needs to be 30 characters long")
    }
    Ok(Secret(s.to_owned()))
}

#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn secret(&self) -> &str { &self.0 }
}

impl std::str::FromStr for Secret {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(Self(s.to_string())) }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "[secret]") }
}

#![warn(clippy::unwrap_in_result)]
pub mod opts;
pub mod util;
pub mod websocket;

use clap::Parser;
pub use opts::Secret;

use std::sync::Arc;

use opts::Opts;

use eyre::Context;

use twitch_api::{client::ClientDefault, HelixClient};

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    // Setup dotenv, tracing and error reporting with eyre
    util::install_utils()?;
    let opts = Opts::parse();

    tracing::debug!(
        "App started!\n{}",
        Opts::try_parse_from(["app", "--version"])
            .unwrap_err()
            .to_string()
    );

    tracing::debug!(opts = ?opts);

    run(Arc::new(opts))
        .await
        .with_context(|| "when running application")?;

    Ok(())
}

/// Run the application
pub async fn run(opts: Arc<Opts>) -> eyre::Result<()> {
    // Create the HelixClient, which is used to make requests to the Twitch API
    let client: HelixClient<_> = twitch_api::HelixClient::with_client(
        <reqwest::Client>::default_client_with_name(Some(
            "twitch-rs/eventsub"
                .parse()
                .wrap_err_with(|| "when creating header name")
                .unwrap(),
        ))
        .wrap_err_with(|| "when creating client")?,
    );

    // Get the access token from the cli, dotenv or an oauth service
    let token: twitch_oauth2::UserToken =
        util::get_access_token(client.get_client(), &opts).await?;

    // Get the user id of the channel we want to listen to
    let user_id = if let Some(ref id) = opts.channel_id {
        id.clone().into()
    } else if let Some(ref login) = opts.channel_login {
        client
            .get_user_from_login(login, &token)
            .await?
            .ok_or_else(|| eyre::eyre!("no user found with name {login}"))?
            .id
    } else {
        // Use the user id from the token if no channel is specified
        token.user_id.clone()
    };

    let websocket_client = websocket::WebsocketClient {
        session_id: None,
        token,
        client,
        user_id,
        connect_url: twitch_api::TWITCH_EVENTSUB_WEBSOCKET_URL.clone(),
        opts,
    };

    let websocket_client = tokio::spawn(async move { websocket_client.run().await });

    tokio::try_join!(flatten(websocket_client))?;
    Ok(())
}

async fn flatten<T>(
    handle: tokio::task::JoinHandle<Result<T, eyre::Report>>,
) -> Result<T, eyre::Report> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(e) => Err(e).wrap_err_with(|| "handling failed"),
    }
}

   Compiling livenac v0.1.0 (C:\Code\LiveNAC)
error[E0609]: no field `type_` on type `Result<EventSubSubscription, serde_json::Error>`
   --> src\eventsub.rs:111:89
    |
111 |                     tracing::warn!("Subscription revoked: {:?}", payload.subscription().type_);
    |                                                                                         ^^^^^ unknown field
    |
help: one of the expressions' fields has a field of the same name
    |
111 |                     tracing::warn!("Subscription revoked: {:?}", payload.subscription().unwrap().type_);
    |                                                                                         +++++++++

error[E0609]: no field `message` on type `twitch_api::eventsub::Message<twitch_api::eventsub::channel::ChannelChatMessageV1>`
   --> src\eventsub.rs:122:49
    |
122 |             let message_text = &payload.message.message.text;
    |                                                 ^^^^^^^ unknown field

error[E0609]: no field `chatter_user_login` on type `twitch_api::eventsub::Message<twitch_api::eventsub::channel::ChannelChatMessageV1>`
   --> src\eventsub.rs:123:50
    |
123 |             let chatter_login = &payload.message.chatter_user_login;
    |                                                  ^^^^^^^^^^^^^^^^^^ unknown field

For more information about this error, try `rustc --explain E0609`.
error: could not compile `livenac` (bin "livenac") due to 3 previous errors