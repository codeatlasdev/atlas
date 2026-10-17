//! iroh P2P transport API exposed to Flutter via flutter_rust_bridge.
//!
//! Bindings: `lib/src/rust/` (regenerate with `flutter_rust_bridge_codegen generate`).

use anyhow::{Context, Result};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey, TransportAddr};
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;
use std::sync::Arc;

/// An iroh endpoint (phone side).  Opaque handle owned by Flutter.
pub struct NativeEndpoint {
    inner: Arc<Endpoint>,
}

/// An open bidirectional QUIC stream to the Mac.  Opaque handle owned by Flutter.
pub struct NativeConn {
    send: Arc<Mutex<SendStream>>,
    recv: Arc<Mutex<BufReader<RecvStream>>>,
}

/// Open a new iroh endpoint using the supplied 32-byte secret key (hex-encoded).
///
/// The secret key is the phone's identity, stored once in secure storage
/// and reused across reconnections.
pub async fn open(secret_key_hex: String) -> Result<NativeEndpoint> {
    let raw = hex::decode(&secret_key_hex)
        .context("secret_key_hex must be valid hex")?;
    let arr: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("secret key must be exactly 32 bytes"))?;
    let secret_key = SecretKey::from_bytes(&arr);

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![b"atlas-mobile/1".to_vec()])
        .bind()
        .await
        .context("failed to bind iroh endpoint")?;

    Ok(NativeEndpoint { inner: Arc::new(endpoint) })
}

/// Return the string representation of the endpoint's public key (NodeId).
pub fn endpoint_id(ep: &NativeEndpoint) -> String {
    ep.inner.id().to_string()
}

/// Connect to a Mac's iroh endpoint and open one bidirectional stream.
///
/// `node_id`  – the Mac's EndpointId (from the QR `node` parameter).
/// `addrs`    – optional direct IP:port hints.
/// `relay`    – optional relay URL hint.
/// ALPN used: `atlas-mobile/1`.
pub async fn connect(
    ep: &NativeEndpoint,
    node_id: String,
    addrs: Vec<String>,
    relay: Option<String>,
) -> Result<NativeConn> {
    let id = EndpointId::from_str(&node_id)
        .context("invalid node_id")?;

    let mut transport_addrs: Vec<TransportAddr> = Vec::new();
    for a in &addrs {
        let sa: std::net::SocketAddr = a.parse().with_context(|| format!("invalid addr: {a}"))?;
        transport_addrs.push(TransportAddr::Ip(sa));
    }
    if let Some(r) = relay {
        let ru: RelayUrl = r.parse().context("invalid relay URL")?;
        transport_addrs.push(TransportAddr::Relay(ru));
    }

    let addr = EndpointAddr::from_parts(id, transport_addrs.into_iter());
    let conn: Connection = ep.inner.connect(addr, b"atlas-mobile/1").await
        .context("connect failed")?;

    let (send, recv) = conn.open_bi().await
        .context("open_bi failed")?;

    Ok(NativeConn {
        send: Arc::new(Mutex::new(send)),
        recv: Arc::new(Mutex::new(BufReader::new(recv))),
    })
}

/// Send one NDJSON line to the Mac (appends '\n').
pub async fn send_line(conn: &NativeConn, line: String) -> Result<()> {
    let mut send = conn.send.lock().await;
    send.write_all(line.as_bytes()).await?;
    send.write_all(b"\n").await?;
    Ok(())
}

/// Read the next line from the Mac.  Returns `None` on stream close.
pub async fn read_line(conn: &NativeConn) -> Result<Option<String>> {
    let mut recv = conn.recv.lock().await;
    let mut line = String::new();
    let n = recv.read_line(&mut line).await?;
    if n == 0 {
        return Ok(None);
    }
    // strip trailing newline
    if line.ends_with('\n') { line.pop(); }
    if line.ends_with('\r') { line.pop(); }
    Ok(Some(line))
}

/// Close the connection gracefully.
pub async fn close_conn(conn: &NativeConn) -> Result<()> {
    let mut send = conn.send.lock().await;
    send.finish()?;
    Ok(())
}
