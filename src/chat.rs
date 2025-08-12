use eyre::Report;
use reqwest::{header, Client as ReqwestClient};
use twitch_api::helix::chat::send_chat_announcement::{
    SendChatAnnouncementBody, SendChatAnnouncementRequest,
};
use twitch_api::helix::chat::send_chat_message::{SendChatMessageBody, SendChatMessageRequest};
use twitch_api::helix::users::GetUsersRequest;
use twitch_api::helix::HelixClient;
use twitch_oauth2::UserToken;
use twitch_types::{UserId, UserIdRef};

const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Default)]
pub struct ChatClient {
    helix_client: HelixClient<'static, ReqwestClient>,
    reqwest_client: ReqwestClient,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AnnouncementColor {
    Blue,
    Green,
    Orange,
    Purple,
    Primary,
}

impl AnnouncementColor {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnnouncementColor::Blue => "blue",
            AnnouncementColor::Green => "green",
            AnnouncementColor::Orange => "orange",
            AnnouncementColor::Purple => "purple",
            AnnouncementColor::Primary => "primary",
        }
    }
}

impl ChatClient {
    pub fn new() -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(APP_USER_AGENT),
        );
        let client = ReqwestClient::builder()
            .default_headers(headers)
            .build()
            .expect("Failed to build reqwest client");

        Self {
            helix_client: HelixClient::with_client(client.clone()),
            reqwest_client: client,
        }
    }

    /// Validates the token against the Twitch API.
    pub async fn validate_token(&self, token: &UserToken) -> Result<(), Report> {
        tracing::info!("Validating token...");
        let validation_url = "https://id.twitch.tv/oauth2/validate";
        let response = self
            .reqwest_client
            .get(validation_url)
            .header(
                "Authorization",
                format!("OAuth {}", token.access_token.secret()),
            )
            .send()
            .await?;

        if response.status().is_success() {
            let body = response.text().await?;
            tracing::info!("Token validation successful: {}", body);
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await?;
            tracing::error!("Token validation failed with status {}: {}", status, body);
            Err(eyre::eyre!(
                "Token validation failed with status {}: {}",
                status,
                body
            ))
        }
    }

    /// Fetches a user's ID from their login name.
    pub async fn get_user_id(
        &self,
        login: &str,
        token: &UserToken,
    ) -> Result<Option<UserId>, Report> {
        let logins: &[&str] = &[login];
        let request = GetUsersRequest::logins(logins);
        let response = self.helix_client.req_get(request, token).await?.data;
        Ok(response.into_iter().next().map(|u| u.id))
    }

    /// Sends a regular chat message to a channel using the Helix API.
    pub async fn send_chat_message(
        &self,
        broadcaster_id: &UserIdRef,
        sender_id: &UserIdRef,
        message: &str,
        token: &UserToken,
    ) -> Result<(), Report> {
        let request = SendChatMessageRequest::new();
        let body = SendChatMessageBody::new(
            broadcaster_id.to_string(),
            sender_id.to_string(),
            message.to_string(),
        );

        let response = self.helix_client.req_post(request, body, token).await?;
        tracing::info!("Message sent successfully: {:?}", response.data);
        Ok(())
    }

    /// Sends an announcement message to a channel using the Helix API.
    /// Requires broadcaster or moderator privileges.
    pub async fn send_announcement(
        &self,
        broadcaster_id: &UserIdRef,
        moderator_id: &UserIdRef,
        message: &str,
        color: Option<AnnouncementColor>,
        token: &UserToken,
    ) -> Result<(), Report> {
        let request = SendChatAnnouncementRequest::new(broadcaster_id, moderator_id);

        let body = if let Some(color) = color {
            SendChatAnnouncementBody::new(message, color.as_str())?
        } else {
            SendChatAnnouncementBody::new(message, "primary")?
        };

        let response = self.helix_client.req_post(request, body, token).await?;
        tracing::info!("Announcement sent successfully: {:?}", response.data);
        Ok(())
    }

    /// Helper function to validate message length (Twitch has a 500 character limit)
    pub fn validate_message(message: &str) -> Result<(), Report> {
        if message.is_empty() {
            return Err(eyre::eyre!("Message cannot be empty"));
        }
        if message.len() > 500 {
            return Err(eyre::eyre!("Message too long (max 500 characters)"));
        }
        Ok(())
    }
}
