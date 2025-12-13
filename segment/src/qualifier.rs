#[derive(Debug, Clone, Copy)]
pub struct Qualifier<'buf> {
    value: &'buf [u8],
}

impl<'buf> From<&'buf [u8]> for Qualifier<'buf> {
    fn from(value: &'buf [u8]) -> Self {
        Qualifier { value }
    }
}

impl<'buf> Qualifier<'buf> {
    pub fn is_phone_number(&self) -> bool {
        self.value == b"12"
    }
}
