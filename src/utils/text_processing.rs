use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, PartialEq)]
pub enum TextOrUrl {
    Text(String),
    Url(String),
}

static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\b((?:https?://|www\d{0,3}[.]|[a-z0-9.\-]+[.][a-z]{2,4}/)(?:[^\s()<>]+|\(([^\s()<>]+|(\([^\s()<>]+\)))*\))+(?:\(([^\s()<>]+|(\([^\s()<>]+\)))*\)|[^\s`!()\[\]{};:'".,<>?«»“”‘’]))"#).unwrap()
});

pub fn parse_text_for_urls(text: &str) -> Vec<TextOrUrl> {
    let mut result = Vec::new();
    let mut last_end = 0;

    for mat in URL_REGEX.find_iter(text) {
        if mat.start() > last_end {
            result.push(TextOrUrl::Text(
                text[last_end..mat.start()].to_string(),
            ));
        }
        let mut url = mat.as_str().to_string();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            url = format!("http://{}", url);
        }
        result.push(TextOrUrl::Url(url));
        last_end = mat.end();
    }

    if last_end < text.len() {
        result.push(TextOrUrl::Text(text[last_end..].to_string()));
    }

    result
}
