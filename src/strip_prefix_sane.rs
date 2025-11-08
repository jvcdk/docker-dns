pub trait SaneStrip {
    fn strip_prefix_sane<'a>(&'a self, prefix: &str) -> &'a str;
}

impl SaneStrip for str {
    #[inline]
    fn strip_prefix_sane<'a>(&'a self, prefix: &str) -> &'a str {
        if let Some(result) = self.strip_prefix(prefix) {
            result
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SaneStrip;

    #[test]
    fn prefix_ok() {
        assert_eq!("abc".strip_prefix_sane("a"), "bc");
        assert_eq!("abc".strip_prefix_sane("x"), "abc");
        assert_eq!("".strip_prefix_sane("x"), "");
    }

    #[test]
    fn unicode_ok() {
        let s = "æøåzzz";
        assert_eq!(s.strip_prefix_sane("æø"), "åzzz");
    }
}
