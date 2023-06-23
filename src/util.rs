pub trait Vec8ToString {
    fn as_str(&self) -> Option<&str>;
}

impl Vec8ToString for Vec<u8> {
    fn as_str(&self) -> Option<&str> {
        match std::str::from_utf8(self) {
            Ok(s) => Some(s),
            Err(_) => Option::None,
        }
    }
}
