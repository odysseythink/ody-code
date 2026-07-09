use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

/// A wrapper around an async reader that preserves backpressure while still
/// allowing consumers to read buffered output after the source has ended.
pub struct BufferedReadable<R> {
    inner: R,
    buffer: Vec<u8>,
    ended: bool,
}

impl<R> BufferedReadable<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: Vec::with_capacity(128 * 1024),
            ended: false,
        }
    }

    pub fn is_ended(&self) -> bool {
        self.ended
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for BufferedReadable<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            if !self.buffer.is_empty() {
                let n = std::cmp::min(buf.remaining(), self.buffer.len());
                buf.put_slice(&self.buffer[..n]);
                self.buffer.drain(..n);
                return Poll::Ready(Ok(()));
            }
            if self.ended {
                return Poll::Ready(Ok(()));
            }

            let mut temp = [0u8; 4096];
            let mut temp_buf = ReadBuf::new(&mut temp);
            match Pin::new(&mut self.inner).poll_read(cx, &mut temp_buf) {
                Poll::Pending => {
                    if self.buffer.is_empty() {
                        return Poll::Pending;
                    }
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {
                    let n = temp_buf.filled().len();
                    if n == 0 {
                        self.ended = true;
                    } else {
                        self.buffer.extend_from_slice(&temp[..n]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn buffers_all_data_and_allows_read_after_source_ends() {
        let data = b"hello world";
        let source = Cursor::new(data.to_vec());
        let mut buffered = BufferedReadable::new(source);

        // Wait until the source is fully drained into the internal buffer.
        let mut all = Vec::new();
        buffered.read_to_end(&mut all).await.unwrap();
        assert_eq!(all, data);
        assert!(buffered.is_ended());
    }

    #[tokio::test]
    async fn partial_reads_then_wait_then_remaining() {
        let data = b"abcdefghij";
        let source = Cursor::new(data.to_vec());
        let mut buffered = BufferedReadable::new(source);

        let mut first = [0u8; 3];
        buffered.read_exact(&mut first).await.unwrap();
        assert_eq!(&first, b"abc");

        let mut rest = Vec::new();
        buffered.read_to_end(&mut rest).await.unwrap();
        assert_eq!(rest, b"defghij");
    }
}
