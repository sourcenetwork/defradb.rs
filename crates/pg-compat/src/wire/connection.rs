//! One client connection on a nuclei socket: the frontend messages read
//! from it, the backend messages written to it, and the client's state
//! pgwire's handlers read and change.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use bytes::{Buf, BytesMut};
use futures::io::AsyncWrite;
use futures::Sink;
use nuclei::net::TcpStream;
use pgwire::api::{ClientInfo, ClientPortalStore, DefaultClient, PgWireConnectionState};
use pgwire::error::PgWireResult;
use pgwire::messages::response::TransactionStatus;
use pgwire::messages::startup::SecretKey;
use pgwire::messages::{
    DecodeContext, PgWireBackendMessage, PgWireFrontendMessage, ProtocolVersion,
};
use rustls::pki_types::CertificateDer;

/// How much a read asks the socket for at once.
const READ_CHUNK: usize = 16 * 1024;

/// Pending output past which a send waits for the socket to take some.
const WRITE_HIGH_WATER: usize = 64 * 1024;

pub(crate) struct Connection<S> {
    socket: TcpStream,
    read: BytesMut,
    chunk: Box<[u8]>,
    write: BytesMut,
    client: DefaultClient<S>,
    decode: DecodeContext,
    closed: bool,
}

impl<S> Connection<S> {
    pub(crate) fn new(socket: TcpStream, peer: SocketAddr) -> Self {
        Self {
            socket,
            read: BytesMut::with_capacity(READ_CHUNK),
            chunk: vec![0; READ_CHUNK].into_boxed_slice(),
            write: BytesMut::with_capacity(READ_CHUNK),
            client: DefaultClient::new(peer, false),
            decode: DecodeContext::default(),
            closed: false,
        }
    }

    /// Whether this side closed the connection: nothing more is read from it.
    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }

    /// The next frontend message, or `None` once the client closed the
    /// connection. How a message is decoded depends on the state: an SSL
    /// request and the startup message have no type byte.
    pub(crate) async fn next(&mut self) -> Option<PgWireResult<PgWireFrontendMessage>> {
        loop {
            self.decode.protocol_version = self.client.protocol_version;
            let state = self.client.state;
            self.decode.awaiting_frontend_ssl =
                matches!(state, PgWireConnectionState::AwaitingSslRequest);
            self.decode.awaiting_frontend_startup = matches!(
                state,
                PgWireConnectionState::AwaitingSslRequest | PgWireConnectionState::AwaitingStartup
            );
            match PgWireFrontendMessage::decode(&mut self.read, &self.decode) {
                Ok(Some(message)) => return Some(Ok(message)),
                Ok(None) => {}
                Err(error) => return Some(Err(error)),
            }
            match futures::AsyncReadExt::read(&mut self.socket, &mut self.chunk).await {
                Ok(0) => return None,
                Ok(read) => self.read.extend_from_slice(&self.chunk[..read]),
                Err(error) => return Some(Err(error.into())),
            }
        }
    }

    fn poll_write_out(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while !self.write.is_empty() {
            let written = ready!(Pin::new(&mut self.socket).poll_write(cx, &self.write))?;
            if written == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            self.write.advance(written);
        }
        Poll::Ready(Ok(()))
    }
}

impl<S> Sink<PgWireBackendMessage> for Connection<S> {
    type Error = io::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.write.len() >= WRITE_HIGH_WATER {
            ready!(this.poll_write_out(cx))?;
        }
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, message: PgWireBackendMessage) -> io::Result<()> {
        message
            .encode(&mut self.get_mut().write)
            .map_err(io::Error::other)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.poll_write_out(cx))?;
        Pin::new(&mut this.socket).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.closed = true;
        ready!(this.poll_write_out(cx))?;
        Pin::new(&mut this.socket).poll_close(cx)
    }
}

impl<S> ClientInfo for Connection<S> {
    fn socket_addr(&self) -> SocketAddr {
        self.client.socket_addr
    }

    fn is_secure(&self) -> bool {
        self.client.is_secure
    }

    fn protocol_version(&self) -> ProtocolVersion {
        self.client.protocol_version()
    }

    fn set_protocol_version(&mut self, version: ProtocolVersion) {
        self.client.set_protocol_version(version);
    }

    fn pid_and_secret_key(&self) -> (i32, SecretKey) {
        self.client.pid_and_secret_key()
    }

    fn set_pid_and_secret_key(&mut self, pid: i32, secret_key: SecretKey) {
        self.client.set_pid_and_secret_key(pid, secret_key);
    }

    fn state(&self) -> PgWireConnectionState {
        self.client.state
    }

    fn set_state(&mut self, new_state: PgWireConnectionState) {
        self.client.set_state(new_state);
    }

    fn transaction_status(&self) -> TransactionStatus {
        self.client.transaction_status()
    }

    fn set_transaction_status(&mut self, new_status: TransactionStatus) {
        self.client.set_transaction_status(new_status);
    }

    fn metadata(&self) -> &HashMap<String, String> {
        self.client.metadata()
    }

    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.client.metadata_mut()
    }

    fn sni_server_name(&self) -> Option<&str> {
        None
    }

    fn client_certificates<'a>(&self) -> Option<&[CertificateDer<'a>]> {
        None
    }
}

impl<S> ClientPortalStore for Connection<S> {
    type PortalStore = <DefaultClient<S> as ClientPortalStore>::PortalStore;

    fn portal_store(&self) -> &Self::PortalStore {
        self.client.portal_store()
    }
}
