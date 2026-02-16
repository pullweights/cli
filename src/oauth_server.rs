use anyhow::{bail, Result};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Result from the OAuth callback.
pub enum CallbackResult {
    Token(String),
    Error(String),
}

const TIMEOUT: Duration = Duration::from_secs(120);

const SUCCESS_HTML: &str = r#"HTTP/1.1 200 OK
Content-Type: text/html; charset=utf-8
Connection: close

<!DOCTYPE html>
<html>
<head><title>PullWeights</title></head>
<body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#0a0a0a;color:#fafafa">
<div style="text-align:center">
<h1>Login successful</h1>
<p>You can close this tab and return to your terminal.</p>
</div>
</body>
</html>"#;

const ERROR_HTML: &str = r#"HTTP/1.1 200 OK
Content-Type: text/html; charset=utf-8
Connection: close

<!DOCTYPE html>
<html>
<head><title>PullWeights</title></head>
<body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#0a0a0a;color:#fafafa">
<div style="text-align:center">
<h1>Login failed</h1>
<p>Something went wrong. Please try again.</p>
</div>
</body>
</html>"#;

/// Start a one-shot HTTP server on a random localhost port.
///
/// Returns `(port, receiver)`. The receiver yields the callback result
/// when the browser hits `/callback?token=...` or `/callback?error=...`.
pub async fn start_callback_server() -> Result<(u16, oneshot::Receiver<CallbackResult>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let result = tokio::time::timeout(TIMEOUT, accept_one(&listener)).await;
        match result {
            Ok(Ok(cb)) => {
                let _ = tx.send(cb);
            }
            Ok(Err(e)) => {
                let _ = tx.send(CallbackResult::Error(format!("Server error: {e}")));
            }
            Err(_) => {
                let _ = tx.send(CallbackResult::Error("Login timed out (120s)".into()));
            }
        }
    });

    Ok((port, rx))
}

async fn accept_one(listener: &TcpListener) -> Result<CallbackResult> {
    let (mut stream, _) = listener.accept().await?;
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the first line: GET /callback?token=xxx HTTP/1.1
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("");

    let result = if let Some(query) = path.strip_prefix("/callback?") {
        parse_callback_query(query)
    } else {
        CallbackResult::Error("Unexpected request path".into())
    };

    let response = match &result {
        CallbackResult::Token(_) => SUCCESS_HTML,
        CallbackResult::Error(_) => ERROR_HTML,
    };
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;

    Ok(result)
}

fn parse_callback_query(query: &str) -> CallbackResult {
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            match key {
                "token" => return CallbackResult::Token(value.to_string()),
                "error" => return CallbackResult::Error(value.to_string()),
                _ => {}
            }
        }
    }
    CallbackResult::Error("No token or error in callback".into())
}

/// Wait for the callback result from the server.
pub async fn wait_for_callback(rx: oneshot::Receiver<CallbackResult>) -> Result<String> {
    match rx.await {
        Ok(CallbackResult::Token(token)) => Ok(token),
        Ok(CallbackResult::Error(e)) => bail!("OAuth login failed: {e}"),
        Err(_) => bail!("Callback server shut down unexpectedly"),
    }
}
