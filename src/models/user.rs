#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct User {
    pub name: String,
    pub color: Option<(u8, u8, u8)>,
}
