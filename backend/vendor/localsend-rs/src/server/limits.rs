//! Connection and request resource limits for the receiver.

use std::time::Duration;

/// Bounds on what one LAN peer can make the receiver hold.
///
/// The defaults suit a desktop receiver. Timeouts only measure time spent
/// waiting for the peer: an accept decision, file hashing and disk writes
/// never count against them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerLimits {
    /// Time allowed for a request head, including an idle keep-alive wait.
    pub header_read_timeout: Duration,
    /// Time allowed to receive a whole body on routes other than upload.
    pub request_body_timeout: Duration,
    /// Longest pause allowed between upload body chunks.
    pub upload_idle_timeout: Duration,
    /// Simultaneous connections across all peers.
    pub max_connections: usize,
    /// Simultaneous connections from one IP address.
    pub max_connections_per_ip: usize,
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            header_read_timeout: Duration::from_secs(30),
            request_body_timeout: Duration::from_secs(30),
            upload_idle_timeout: Duration::from_secs(120),
            max_connections: 64,
            max_connections_per_ip: 16,
        }
    }
}

/// Admits connections within the global and per-IP limits. A refused
/// connection is dropped before any TLS or HTTP work; an admitted one holds
/// its slot until the stream is dropped.
#[derive(Clone, Debug)]
pub(crate) struct ConnectionLimit {
    counts: std::sync::Arc<std::sync::Mutex<ConnectionCounts>>,
    max_connections: usize,
    max_connections_per_ip: usize,
}

#[derive(Debug, Default)]
struct ConnectionCounts {
    total: usize,
    per_ip: std::collections::HashMap<std::net::IpAddr, usize>,
}

impl ConnectionLimit {
    pub(crate) fn new(limits: &ServerLimits) -> Self {
        Self {
            counts: Default::default(),
            max_connections: limits.max_connections,
            max_connections_per_ip: limits.max_connections_per_ip,
        }
    }

    fn admit(&self, ip: std::net::IpAddr) -> Option<ConnectionSlot> {
        let mut counts = self.counts.lock().expect("connection counts poisoned");
        let from_ip = counts.per_ip.get(&ip).copied().unwrap_or(0);
        if counts.total >= self.max_connections || from_ip >= self.max_connections_per_ip {
            return None;
        }
        counts.total += 1;
        counts.per_ip.insert(ip, from_ip + 1);
        Some(ConnectionSlot {
            counts: self.counts.clone(),
            ip,
        })
    }
}

impl<S> axum_server::accept::Accept<tokio::net::TcpStream, S> for ConnectionLimit {
    type Stream = LimitedStream;
    type Service = S;
    type Future = std::future::Ready<std::io::Result<(LimitedStream, S)>>;

    fn accept(&self, stream: tokio::net::TcpStream, service: S) -> Self::Future {
        let slot = stream
            .peer_addr()
            .ok()
            .and_then(|peer| self.admit(peer.ip()));
        std::future::ready(match slot {
            Some(slot) => Ok((
                LimitedStream {
                    stream,
                    _slot: slot,
                },
                service,
            )),
            None => {
                tracing::warn!("Connection refused: receiver connection limit reached");
                Err(std::io::Error::other("connection limit reached"))
            }
        })
    }
}

#[derive(Debug)]
struct ConnectionSlot {
    counts: std::sync::Arc<std::sync::Mutex<ConnectionCounts>>,
    ip: std::net::IpAddr,
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        let mut counts = self.counts.lock().expect("connection counts poisoned");
        counts.total -= 1;
        if let Some(from_ip) = counts.per_ip.get_mut(&self.ip) {
            *from_ip -= 1;
            if *from_ip == 0 {
                counts.per_ip.remove(&self.ip);
            }
        }
    }
}

/// A TCP stream that releases its connection slot when dropped.
#[derive(Debug)]
pub(crate) struct LimitedStream {
    stream: tokio::net::TcpStream,
    _slot: ConnectionSlot,
}

impl tokio::io::AsyncRead for LimitedStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for LimitedStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// How long a request body may take to arrive.
#[derive(Clone, Copy, Debug)]
pub(crate) enum BodyTimeout {
    /// The whole body must arrive within this time of the request head.
    Total(Duration),
    /// Each chunk must arrive within this time of asking for it. Time the
    /// receiver spends between chunks, such as writing to disk, is not counted.
    Idle(Duration),
}

/// Wrap a request body so a peer that stops or trickles sending ends the body
/// with an error instead of holding the request indefinitely.
pub(crate) fn time_limited_body(body: axum::body::Body, limit: BodyTimeout) -> axum::body::Body {
    use futures_util::StreamExt;

    let (deadline, idle) = match limit {
        BodyTimeout::Total(total) => (Some(tokio::time::Instant::now() + total), total),
        BodyTimeout::Idle(idle) => (None, idle),
    };
    let stream =
        futures_util::stream::unfold(Some(body.into_data_stream()), move |stream| async move {
            let mut stream = stream?;
            let next = match deadline {
                Some(deadline) => tokio::time::timeout_at(deadline, stream.next()).await,
                None => tokio::time::timeout(idle, stream.next()).await,
            };
            match next {
                Ok(Some(chunk)) => Some((chunk, Some(stream))),
                Ok(None) => None,
                Err(_) => Some((
                    Err(axum::Error::new(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "request body timed out",
                    ))),
                    None,
                )),
            }
        });
    axum::body::Body::from_stream(stream)
}

/// Middleware applying [`time_limited_body`] to every request it wraps.
pub(crate) async fn limit_request_body(
    limit: BodyTimeout,
    request: axum::extract::Request,
) -> axum::extract::Request {
    request.map(|body| time_limited_body(body, limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_bounded_per_ip_and_globally_and_released_on_drop() {
        let limit = ConnectionLimit::new(&ServerLimits {
            max_connections: 3,
            max_connections_per_ip: 2,
            ..ServerLimits::default()
        });
        let a: std::net::IpAddr = "192.168.1.7".parse().unwrap();
        let b: std::net::IpAddr = "192.168.1.8".parse().unwrap();
        let first = limit.admit(a).unwrap();
        let _second = limit.admit(a).unwrap();
        assert!(limit.admit(a).is_none(), "per-IP limit");
        let _third = limit.admit(b).unwrap();
        assert!(limit.admit(b).is_none(), "global limit");
        drop(first);
        assert!(limit.admit(a).is_some(), "dropping a slot frees it");
    }
}
