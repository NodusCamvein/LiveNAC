use eyre::Report;
use futures::StreamExt;
use reqwest::Client as ReqwestClient;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use twitch_api::eventsub::{
    channel::ChannelChatMessageV1,
    event::websocket::{EventsubWebsocketData, WelcomePayload},
    Transport, Event, Message,
};
use twitch_api::helix::eventsub::{CreateEventSubSubscriptionBody, CreateEventSubSubscriptionRequest};
use twitch_api::helix::users::GetUsersRequest;
use twitch_api::HelixClient;
use twitch_oauth2::UserToken;
use twitch_types::UserId;

pub type EventSubMessage = String;

pub struct EventSubClient {
    helix_client: HelixClient<'static, ReqwestClient>,
    user_id: UserId,
    token: Arc<UserToken>,
    message_tx: mpsc::Sender<EventSubMessage>,
}

impl EventSubClient {
    pub fn new(
        user_id: UserId,
        token: Arc<UserToken>,
        message_tx: mpsc::Sender<EventSubMessage>,
    ) -> Self {
        Self {
            helix_client: HelixClient::with_client(ReqwestClient::new()),
            user_id,
            token,
            message_tx,
        }
    }

    /// Fetches a user's ID from their login name.
    /// This is needed to create the subscription.
    pub async fn get_user_id(&self, login: &str) -> Result<Option<UserId>, Report> {
        let logins: &[&str] = &[login];
        let request = GetUsersRequest::logins(logins);
        let response = self
            .helix_client
            .req_get(request, &*self.token)
            .await?
            .data;
        Ok(response.into_iter().next().map(|u| u.id))
    }

    pub async fn run(self, broadcaster_login: String) -> Result<(), eyre::Report> {
        tracing::info!("Starting EventSub client for: {}", &broadcaster_login);

        // Get the broadcaster's user ID from their login name.
        let broadcaster_id = self.get_user_id(&broadcaster_login).await?.unwrap();

        // Connect to the EventSub websocket.
        let (ws_stream, _) = connect_async("wss://eventsub.wss.twitch.tv/ws").await?;
        let (_write, mut read) = ws_stream.split();

        // The first message is a welcome message containing the session ID.
        let message = read
            .next()
            .await
            .ok_or_else(|| eyre::eyre!("websocket stream ended"))??;
        let message_data = message.into_data();
        let welcome: WelcomePayload = serde_json::from_slice(&message_data)?;
        let session_id = welcome.session.id.to_string();
        tracing::info!("Received session ID: {}", session_id);

        // Create the subscription request body using the session ID and the convenience method
        let subscription_request_body = CreateEventSubSubscriptionBody::new(
            ChannelChatMessageV1::new(broadcaster_id.clone(), self.user_id.clone()),
            Transport::websocket(session_id),
        );

        // Use req_post to send the subscription request
        let subscription = self
            .helix_client
            .req_post(
                CreateEventSubSubscriptionRequest::new(),
                subscription_request_body,
                &*self.token,
            )
            .await?;

        let subscription_type = subscription.data.type_.to_str();
        tracing::info!("Created a subscription: {:?}", subscription_type);

        // Process incoming websocket messages.
        while let Some(msg) = read.next().await {
            let msg = msg?;
            let msg_data = msg.into_data();
            let data: EventsubWebsocketData = serde_json::from_slice(&msg_data)?;
            match data {
                EventsubWebsocketData::Welcome { .. } => {
                    tracing::trace!("Welcome received");
                }
                EventsubWebsocketData::Notification { payload, .. } => {
                    self.handle_notification(payload).await;
                }
                EventsubWebsocketData::Keepalive { .. } => {
                    tracing::trace!("Keepalive received");
                }
                EventsubWebsocketData::Reconnect { .. } => {
                    tracing::warn!("Reconnect message received. You should implement reconnection logic.");
                    break;
                }
                EventsubWebsocketData::Revocation { payload, .. } => {
                    // Fixed: Access the subscription properly
                    match payload.subscription() {
                        Ok(subscription) => {
                            tracing::warn!("Subscription revoked: {:?}", subscription.type_);
                        }
                        Err(_) => {
                            tracing::warn!("Subscription revoked: Unable to parse subscription details");
                        }
                    }
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn handle_notification(&self, notification: Event) {
        if let Event::ChannelChatMessageV1(payload) = notification {
            if let Message::Notification(event_data) = payload.message {
                let message_text = &event_data.message.text;
                let chatter_login = &event_data.chatter_user_login;
                let chatter_display_name = &event_data.chatter_user_name;
                
                let formatted_message = format!(
                    "{} ({}): {}",
                    chatter_display_name, chatter_login, message_text
                );

                if let Err(e) = self.message_tx.send(formatted_message).await {
                    tracing::error!("Failed to send message to UI thread: {}", e);
                }
            }
        }
    }
}