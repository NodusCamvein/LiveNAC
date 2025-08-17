use super::emote::Emote;
use chrono::{DateTime, Local};

#[derive(Clone, Debug)]
pub enum MessageFragment {
    Text(String),
    Emote(Emote),
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub sender_name: String,
    pub sender_color: Option<(u8, u8, u8)>,
    pub fragments: Vec<MessageFragment>,
    pub timestamp: DateTime<Local>,
}
