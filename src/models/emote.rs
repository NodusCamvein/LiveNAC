#[derive(Clone, Debug, PartialEq)]
pub struct Emote {
    pub name: String,
    pub url: String,
    pub source: EmoteSource,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmoteSource {
    Twitch,
    Bttv,
    Ffz,
    Stv,
}
