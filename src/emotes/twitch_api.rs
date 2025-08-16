use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use twitch_oauth2::UserToken;

#[derive(Debug, Serialize, Deserialize)]
pub struct TwitchEmote {
    pub id: String,
    pub name: String,
    pub images: EmoteImages,
    pub format: Vec<String>,
    pub scale: Vec<String>,
    pub theme_mode: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmoteImages {
    pub url_1x: String,
    pub url_2x: String,
    pub url_4x: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GlobalEmotesResponse {
    data: Vec<TwitchEmote>,
    template: String,
}

#[derive(Clone)]
pub struct TwitchApiClient {
    client: reqwest::Client,
    client_id: String,
}

impl TwitchApiClient {
    pub fn new(client_id: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id,
        }
    }

    pub async fn get_global_emotes(
        &self,
        token: &UserToken,
    ) -> Result<Vec<TwitchEmote>, reqwest::Error> {
        let response = self
            .client
            .get("https://api.twitch.tv/helix/chat/emotes/global")
            .header(
                AUTHORIZATION,
                format!("Bearer {}", token.access_token.as_str()),
            )
            .header("Client-Id", &self.client_id)
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await?
            .json::<GlobalEmotesResponse>()
            .await?;

        Ok(response.data)
    }
}
