use eyre::Report;
use reqwest::{Client as ReqwestClient, header};
use twitch_api::helix::HelixClient;
use twitch_api::helix::chat::send_chat_announcement::{
    SendChatAnnouncementBody, SendChatAnnouncementRequest,
};
use twitch_api::helix::chat::send_chat_message::{SendChatMessageBody, SendChatMessageRequest};
use twitch_api::helix::users::GetUsersRequest;
use twitch_oauth2::UserToken;
use twitch_types::{UserId, UserIdRef};

const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Default)]
pub struct ChatClient {
    helix_client: HelixClient<'static, ReqwestClient>,
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
            helix_client: HelixClient::with_client(client),
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
}
