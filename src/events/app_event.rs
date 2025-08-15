use crate::{app::config::Config, core::auth::AuthMessage, models::message::ChatMessage};

#[derive(Debug)]
pub enum AppEvent {
    ConfigLoaded(Result<Config, eyre::Report>),
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
