use crate::{
    app::config::Config, core::auth::AuthMessage, emotes::twitch_api::TwitchEmote,
    models::message::ChatMessage,
};
use twitch_oauth2::UserToken;

#[derive(Debug)]
pub enum AppEvent {
    ConfigLoaded(Result<Config, eyre::Report>),
    SilentLoginComplete(Result<UserToken, eyre::Report>),
    ProfileSwitchSilentLoginComplete(Result<UserToken, eyre::Report>, String),
    Auth(AuthMessage),
    AuthCancel,
    AuthFlowStartFailed(String),
    Chat(ChatEvent),
    GlobalEmotesLoaded(Result<Vec<TwitchEmote>, String>),
}

#[derive(Debug)]
pub enum ChatEvent {
    NewChatMessage(ChatMessage),
    MessageSent,
    MessageSendError(String),
    EventSubError(String),
}
