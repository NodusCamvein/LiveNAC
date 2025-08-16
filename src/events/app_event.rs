use crate::{app::config::Config, core::auth::AuthMessage, models::message::ChatMessage};
use twitch_oauth2::UserToken;

#[derive(Debug)]
pub enum AppEvent {
    ConfigLoaded(Result<Config, eyre::Report>),
    SilentLoginComplete(Result<UserToken, eyre::Report>),
    Auth(AuthMessage),
    Chat(ChatEvent),
}

#[derive(Debug)]
pub enum ChatEvent {
    NewChatMessage(ChatMessage),
    MessageSent,
    MessageSendError(String),
    EventSubError(String),
}
