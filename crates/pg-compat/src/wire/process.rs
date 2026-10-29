//! A connection's life: SSL and GSS requests refused (the server speaks
//! plain TCP) and the startup, all within one time limit, then each
//! frontend message handed to pgwire's handlers, with pgwire's state
//! machine and error recovery, until either side closes the connection.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use futures::SinkExt;
use nuclei::time::{timeout_at, Instant};
use pgwire::api::auth::StartupHandler;
use pgwire::api::cancel::CancelHandler;
use pgwire::api::copy::CopyHandler;
use pgwire::api::query::{send_ready_for_query, ExtendedQueryHandler, SimpleQueryHandler};
use pgwire::api::{ClientInfo, ErrorHandler, PgWireConnectionState, PgWireServerHandlers};
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::response::{GssEncResponse, ReadyForQuery, SslResponse, TransactionStatus};
use pgwire::messages::{PgWireBackendMessage, PgWireFrontendMessage, SslNegotiationMetaMessage};

use super::connection::Connection;

/// How long a client may take from connecting to the end of its startup.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

/// The first byte of a TLS ClientHello: a client that starts TLS directly,
/// which this server does not accept.
const TLS_HANDSHAKE: u8 = 0x16;

/// Serves one client connection with `handlers` until either side closes
/// it. A client that starts TLS directly is closed at once: this server
/// speaks plain TCP.
pub async fn serve<H>(
    socket: nuclei::net::TcpStream,
    peer: std::net::SocketAddr,
    handlers: H,
) -> io::Result<()>
where
    H: PgWireServerHandlers,
{
    socket.get_ref().set_nodelay(true)?;
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut first = [0u8; 1];
    let Ok(peeked) = timeout_at(deadline, socket.peek(&mut first)).await else {
        return Ok(());
    };
    if peeked? == 1 && first[0] == TLS_HANDSHAKE {
        return Ok(());
    }
    run(
        socket,
        peer,
        deadline,
        handlers.startup_handler(),
        handlers.simple_query_handler(),
        handlers.extended_query_handler(),
        handlers.copy_handler(),
        handlers.cancel_handler(),
        handlers.error_handler(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run<A, Q, EQ, C, CR, E>(
    socket: nuclei::net::TcpStream,
    peer: std::net::SocketAddr,
    deadline: Instant,
    startup: Arc<A>,
    simple: Arc<Q>,
    extended: Arc<EQ>,
    copy: Arc<C>,
    cancel: Arc<CR>,
    errors: Arc<E>,
) -> io::Result<()>
where
    A: StartupHandler,
    Q: SimpleQueryHandler,
    EQ: ExtendedQueryHandler,
    EQ::Statement: Clone + Send + Sync,
    C: CopyHandler,
    CR: CancelHandler,
    E: ErrorHandler,
{
    let mut connection = Connection::<EQ::Statement>::new(socket, peer);
    let Ok(negotiated) = timeout_at(deadline, refuse_ssl(&mut connection)).await else {
        return Ok(());
    };
    if !negotiated? {
        return Ok(());
    }
    connection.set_state(PgWireConnectionState::AwaitingStartup);

    loop {
        let starting = matches!(
            connection.state(),
            PgWireConnectionState::AwaitingStartup
                | PgWireConnectionState::AuthenticationInProgress
        );
        let message = if starting {
            timeout_at(deadline, connection.next()).await.ok().flatten()
        } else {
            connection.next().await
        };
        let Some(Ok(message)) = message else {
            return Ok(());
        };
        let extended_query = match connection.state() {
            PgWireConnectionState::CopyInProgress(extended_query) => extended_query,
            _ => message.is_extended_query(),
        };
        let handled = dispatch(
            message,
            &mut connection,
            startup.as_ref(),
            simple.as_ref(),
            extended.as_ref(),
            copy.as_ref(),
            cancel.as_ref(),
        )
        .await;
        if let Err(mut error) = handled {
            errors.on_error(&connection, &mut error);
            report(&mut connection, error, extended_query).await?;
        }
        if connection.is_closed() {
            return Ok(());
        }
    }
}

/// Answers SSL and GSS encryption requests with a refusal. `false` when
/// the client closed the connection during them. The first packet that is
/// neither (the startup, or a cancel request) stays unread.
async fn refuse_ssl<S>(connection: &mut Connection<S>) -> io::Result<bool> {
    let (mut ssl_done, mut gss_done) = (false, false);
    while !(ssl_done && gss_done) {
        match connection.next().await {
            Some(Ok(PgWireFrontendMessage::SslNegotiation(
                SslNegotiationMetaMessage::PostgresSsl(_),
            ))) => {
                connection
                    .send(PgWireBackendMessage::SslResponse(SslResponse::Refuse))
                    .await?;
                ssl_done = true;
            }
            Some(Ok(PgWireFrontendMessage::SslNegotiation(
                SslNegotiationMetaMessage::PostgresGss(_),
            ))) => {
                connection
                    .send(PgWireBackendMessage::GssEncResponse(GssEncResponse::Refuse))
                    .await?;
                gss_done = true;
            }
            Some(Ok(_)) => break,
            Some(Err(error)) => return Err(io::Error::other(error)),
            None => return Ok(false),
        }
    }
    Ok(true)
}

/// One frontend message through pgwire's handlers, by connection state.
async fn dispatch<S, A, Q, EQ, C, CR>(
    message: PgWireFrontendMessage,
    connection: &mut Connection<S>,
    startup: &A,
    simple: &Q,
    extended: &EQ,
    copy: &C,
    cancel: &CR,
) -> PgWireResult<()>
where
    S: Clone + Send + Sync,
    A: StartupHandler,
    Q: SimpleQueryHandler,
    EQ: ExtendedQueryHandler<Statement = S>,
    C: CopyHandler,
    CR: CancelHandler,
{
    if let PgWireFrontendMessage::CancelRequest(request) = message {
        cancel.on_cancel_request(request).await;
        connection.close().await?;
        return Ok(());
    }
    match connection.state() {
        PgWireConnectionState::AwaitingStartup
        | PgWireConnectionState::AuthenticationInProgress => {
            startup.on_startup(connection, message).await?;
        }
        PgWireConnectionState::AwaitingSync => {
            if let PgWireFrontendMessage::Sync(sync) = message {
                extended.on_sync(connection, sync).await?;
                connection.set_state(PgWireConnectionState::ReadyForQuery);
            }
        }
        PgWireConnectionState::CopyInProgress(extended_query) => match message {
            PgWireFrontendMessage::CopyData(data) => copy.on_copy_data(connection, data).await?,
            PgWireFrontendMessage::CopyDone(done) => {
                let result = copy.on_copy_done(connection, done).await;
                // The copy ends here, before an error is reported. A copy
                // begun by a simple query is answered now; in the extended
                // protocol the Sync that follows reaches on_sync and answers.
                connection.set_state(PgWireConnectionState::ReadyForQuery);
                result?;
                if !extended_query {
                    send_ready_for_query(connection, TransactionStatus::Idle).await?;
                }
            }
            PgWireFrontendMessage::CopyFail(fail) => {
                let error = copy.on_copy_fail(connection, fail).await;
                if !extended_query {
                    connection.set_state(PgWireConnectionState::ReadyForQuery);
                }
                return Err(error);
            }
            _ => {}
        },
        _ => match message {
            PgWireFrontendMessage::Query(query) => simple.on_query(connection, query).await?,
            PgWireFrontendMessage::Parse(parse) => extended.on_parse(connection, parse).await?,
            PgWireFrontendMessage::Bind(bind) => extended.on_bind(connection, bind).await?,
            PgWireFrontendMessage::Execute(execute) => {
                extended.on_execute(connection, execute).await?
            }
            PgWireFrontendMessage::Describe(describe) => {
                extended.on_describe(connection, describe).await?
            }
            PgWireFrontendMessage::Flush(flush) => extended.on_flush(connection, flush).await?,
            PgWireFrontendMessage::Sync(sync) => extended.on_sync(connection, sync).await?,
            PgWireFrontendMessage::Close(close) => extended.on_close(connection, close).await?,
            _ => {}
        },
    }
    Ok(())
}

/// Reports `error` to the client: after an error in the extended protocol
/// messages are skipped until a Sync, otherwise the client is ready for its
/// next query at once. A fatal error closes the connection.
async fn report<S>(
    connection: &mut Connection<S>,
    error: PgWireError,
    wait_for_sync: bool,
) -> io::Result<()> {
    let info: ErrorInfo = error.into();
    let fatal = info.is_fatal();
    connection
        .send(PgWireBackendMessage::ErrorResponse(info.into()))
        .await?;
    let status = connection.transaction_status().to_error_state();
    connection.set_transaction_status(status);
    if wait_for_sync {
        connection.set_state(PgWireConnectionState::AwaitingSync);
    } else {
        connection.set_state(PgWireConnectionState::ReadyForQuery);
        connection
            .feed(PgWireBackendMessage::ReadyForQuery(ReadyForQuery::new(
                status,
            )))
            .await?;
    }
    connection.flush().await?;
    if fatal {
        connection.close().await?;
    }
    Ok(())
}
