use crate::{
    events::app_event::{AppEvent, ChatEvent},
    models::{
        emote::{Emote, EmoteSource},
        message::{ChatMessage, MessageFragment},
    },
};
use chrono::Local;
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
    message_tx: mpsc::Sender<AppEvent>,
    broadcaster_id: UserId,
    session_id: Option<String>,
}

impl EventSubClient {
    pub fn new(
        user_id: UserId,
        token: Arc<UserToken>,
        message_tx: mpsc::Sender<AppEvent>,
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
                    .send(AppEvent::Chat(ChatEvent::EventSubError(e.to_string())))
                    .await;
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

    async fn handle_welcome(&mut self, welcome: WelcomePayload<'_>) -> Result<(), eyre::Report> {
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
            .req_post(
                CreateEventSubSubscriptionRequest::new(),
                body.into(),
                &*self.token,
            )
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
                let chatter_display_name = event_data.chatter_user_name;

                let sender_color = if !event_data.color.as_str().is_empty()
                    && event_data.color.as_str().len() == 7
                    && event_data.color.as_str().starts_with('#')
                {
                    let r = u8::from_str_radix(&event_data.color.as_str()[1..3], 16).unwrap_or(255);
                    let g = u8::from_str_radix(&event_data.color.as_str()[3..5], 16).unwrap_or(255);
                    let b = u8::from_str_radix(&event_data.color.as_str()[5..7], 16).unwrap_or(255);
                    Some((r, g, b))
                } else {
                    None
                };

                let mut fragments = Vec::new();
                for fragment in &event_data.message.fragments {
                    match fragment {
                        twitch_api::eventsub::channel::chat::Fragment::Text { text } => {
                            fragments.push(MessageFragment::Text(text.to_string()));
                        }
                        twitch_api::eventsub::channel::chat::Fragment::Emote { text, emote } => {
                            let emote_url = format!(
                                "https://static-cdn.jtvnw.net/emoticons/v2/{}/default/dark/1.0",
                                emote.id
                            );
                            fragments.push(MessageFragment::Emote(Emote {
                                name: text.to_string(),
                                url: emote_url,
                                source: EmoteSource::Twitch,
                            }));
                        }
                        _ => {
                            // TODO: Maybe log this
                        }
                    }
                }

                let message = ChatMessage {
                    sender_name: chatter_display_name.to_string(),
                    sender_color,
                    fragments,
                    timestamp: Local::now(),
                };

                let msg = AppEvent::Chat(ChatEvent::NewChatMessage(message));
                if self.message_tx.send(msg).await.is_err() {
                    tracing::error!("Failed to send message to UI thread: channel is closed.");
                }
            }
        }
    }
}
