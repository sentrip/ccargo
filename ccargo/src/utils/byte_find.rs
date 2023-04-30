
// Find a pattern of bytes in a byte slice
pub trait ByteFind<P> {
    fn find(&self, pat: P) -> Option<usize>;
    fn rfind(&self, pat: P) -> Option<usize>;
}

impl ByteFind<u8> for [u8] {
    #[inline]
    fn find(&self, pat: u8) -> Option<usize> {
        memchr::memchr(pat, self)
    }
    #[inline]
    fn rfind(&self, pat: u8) -> Option<usize> {
        memchr::memrchr(pat, self)
    }
}

impl ByteFind<&[u8]> for [u8] {
    #[inline]
    fn find(&self, pat: &[u8]) -> Option<usize> {
        memchr::memmem::find(self, pat)
    }
    #[inline]
    fn rfind(&self, pat: &[u8]) -> Option<usize> {
        memchr::memmem::rfind(self, pat)
    }
}

impl<const N: usize> ByteFind<&[u8; N]> for [u8] {
    #[inline]
    fn find(&self, pat: &[u8; N]) -> Option<usize> {
        memchr::memmem::find(self, pat.as_slice())
    }
    #[inline]
    fn rfind(&self, pat: &[u8; N]) -> Option<usize> {
        memchr::memmem::rfind(self, pat.as_slice())
    }
}

impl ByteFind<u8> for Vec<u8> {
    #[inline]
    fn find(&self, pat: u8) -> Option<usize> {
        memchr::memchr(pat, self.as_slice())
    }
    #[inline]
    fn rfind(&self, pat: u8) -> Option<usize> {
        memchr::memrchr(pat, self.as_slice())
    }
}

impl ByteFind<&[u8]> for Vec<u8> {
    #[inline]
    fn find(&self, pat: &[u8]) -> Option<usize> {
        memchr::memmem::find(self.as_slice(), pat)
    }
    #[inline]
    fn rfind(&self, pat: &[u8]) -> Option<usize> {
        memchr::memmem::rfind(self.as_slice(), pat)
    }
}

impl<const N: usize> ByteFind<&[u8; N]> for Vec<u8> {
    #[inline]
    fn find(&self, pat: &[u8; N]) -> Option<usize> {
        memchr::memmem::find(self.as_slice(), pat.as_slice())
    }
    #[inline]
    fn rfind(&self, pat: &[u8; N]) -> Option<usize> {
        memchr::memmem::rfind(self.as_slice(), pat.as_slice())
    }
}
