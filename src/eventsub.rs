use crate::ui::UiMessage; // Changed
use crate::ui::ChatUiMessage; // Still need this for creating the message
use eyre::eyre;
use futures::StreamExt;
use reqwest::Client as ReqwestClient;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use twitch_api::{
    eventsub::{
        channel::ChannelChatMessageV1,
        event::websocket::{EventsubWebsocketData, WelcomePayload},
        Event, Message, Transport,
    },
    helix::eventsub::{
        CreateEventSubSubscriptionBody, CreateEventSubSubscriptionRequest,
    },
    HelixClient,
};
use twitch_oauth2::UserToken;
use twitch_types::UserId;

pub struct EventSubClient {
    helix_client: HelixClient<'static, ReqwestClient>,
    user_id: UserId,
    token: Arc<UserToken>,
    message_tx: mpsc::Sender<UiMessage>, // Changed
    broadcaster_id: UserId,
    session_id: Option<String>,
}

impl EventSubClient {
    pub fn new(
        user_id: UserId,
        token: Arc<UserToken>,
        message_tx: mpsc::Sender<UiMessage>, // Changed
        broadcaster_id: UserId,
    ) -> Self {
        let reqwest_client = ReqwestClient::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("Failed to build reqwest client");

        Self {
            helix_client: HelixClient::with_client(reqwest_client),
            user_id,
            token,
            message_tx,
            broadcaster_id,
            session_id: None,
        }
    }

    pub async fn run(mut self) -> Result<(), eyre::Report> {
        tracing::info!(
            "Starting EventSub client for broadcaster ID: {}",
            &self.broadcaster_id
        );

        let (ws_stream, _) = connect_async("wss://eventsub.wss.twitch.tv/ws").await?;
        tracing::info!("WebSocket handshake has been successfully completed");
        let (_write, mut read) = ws_stream.split();

        while let Some(msg) = read.next().await {
            let msg = match msg {
                Ok(msg) => msg,
                Err(e) => {
                    tracing::error!("Error reading message from websocket: {}", e);
                    continue;
                }
            };

            if let Err(e) = self.handle_message(msg).await {
                // Send error back to UI
                let _ = self
                    .message_tx
                    .send(UiMessage::Chat(ChatUiMessage::EventSubError(
                        e.to_string(),
                    )))
                    .await;
                // And then break the loop
                return Err(e);
            }
        }

        Ok(())
    }

    async fn handle_message(&mut self, msg: WsMessage) -> Result<(), eyre::Report> {
        match msg {
            WsMessage::Text(s) => {
                let data: EventsubWebsocketData = Event::parse_websocket(&s)?;
                match data {
                    EventsubWebsocketData::Welcome { payload, .. } => {
                        self.handle_welcome(payload).await?;
                    }
                    EventsubWebsocketData::Notification { payload, .. } => {
                        self.handle_notification(payload).await;
                    }
                    EventsubWebsocketData::Keepalive { .. } => {
                        tracing::trace!("Keepalive received");
                    }
                    EventsubWebsocketData::Reconnect { .. } => {
                        tracing::warn!(
                            "Reconnect message received. You should implement reconnection logic."
                        );
                    }
                    _ => {}
                }
            }
            WsMessage::Close(c) => {
                tracing::info!("Websocket closed: {:?}", c);
                return Err(eyre!("WebSocket connection closed"));
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_welcome(
        &mut self,
        welcome: WelcomePayload<'_>,
    ) -> Result<(), eyre::Report> {
        let session_id = welcome.session.id.to_string();
        tracing::info!("Received session ID: {}", session_id);
        self.session_id = Some(session_id.clone());

        let transport = Transport::websocket(session_id);

        let body = CreateEventSubSubscriptionBody::new(
            ChannelChatMessageV1::new(self.broadcaster_id.clone(), self.user_id.clone()),
            transport,
        );

        let subscription = self
            .helix_client
            .req_post(CreateEventSubSubscriptionRequest::new(), body, &*self.token)
            .await?;

        tracing::info!(
            "Created subscription: {:?}, status: {:?}",
            subscription.data.type_, subscription.data.status
        );
        Ok(())
    }

    async fn handle_notification(&self, notification: Event) {
        if let Event::ChannelChatMessageV1(payload) = notification {
            if let Message::Notification(event_data) = payload.message {
                let message_text = &event_data.message.text;
                let chatter_login = &event_data.chatter_user_login;
                let chatter_display_name = &event_data.chatter_user_name;

                // Conditionally format the message. If the display name is plain ASCII,
                // or just a different capitalization of the login name, don't show the login name.
                // Otherwise, show both for clarity (e.g., for CJK names).
                let formatted_message = if chatter_display_name
                    .as_str()
                    .eq_ignore_ascii_case(chatter_login.as_str())
                {
                    format!("{}: {}", chatter_display_name, message_text)
                } else {
                    format!(
                        "{} ({}): {}",
                        chatter_display_name, chatter_login, message_text
                    )
                };

                let msg = UiMessage::Chat(ChatUiMessage::NewChatMessage(formatted_message));
                if self.message_tx.send(msg).await.is_err() {
                    tracing::error!("Failed to send message to UI thread: channel is closed.");
                }
            }
        }
    }
}
