use crate::app::config::Config;
use crate::core::auth::AuthMessage;

#[derive(Debug)]
pub enum AppEvent {
    ConfigLoaded(Result<Config, eyre::Report>),
    Auth(AuthMessage),
    Chat(ChatEvent),
}

#[derive(Debug)]
pub enum ChatEvent {
    NewChatMessage(String),
    MessageSent,
    MessageSendError(String),
    EventSubError(String),
}
