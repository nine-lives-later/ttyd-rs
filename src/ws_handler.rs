use axum::{
    extract::{
        ConnectInfo, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use pty_process::Command;
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::select;
use tracing::{error, info};

use crate::{AppState, AuthMode};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.protocols(["tty"])
        .on_upgrade(move |socket| handle_socket(socket, state, addr))
}

#[derive(Deserialize, Debug)]
struct InitMessage {
    #[serde(default)]
    columns: u16,
    #[serde(default)]
    rows: u16,
}

#[derive(Deserialize, Debug)]
struct ResizeMessage {
    columns: u16,
    rows: u16,
}

#[derive(Deserialize, Debug)]
struct AuthMessage {
    #[serde(rename = "authToken")]
    auth_token: String,
}

#[derive(Debug)]
enum ConnectionState {
    Initializing,
    LimitReached,
    WaitingForAuthentication,
    WaitingForWebsocket,
    Active(u16, u16), // columns, rows
    Disconnecting,
    Disconnected,
}

async fn handle_socket(
    socket: WebSocket,
    app_state: Arc<AppState>,
    remote_addr: std::net::SocketAddr,
) {
    let remote_ip = remote_addr.ip().to_string();
    let remote_port = remote_addr.port();
    let connection_id = uuid::Uuid::now_v7().to_string();

    let mut current_state = ConnectionState::Initializing;
    tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);

    let (mut socket_tx, mut socket_rx) = socket.split();

    let mut added_to_active_count = false;

    loop {
        match current_state {
            ConnectionState::Initializing => {
                info!(
                    connection_id = %connection_id,
                    remote_ip = %remote_ip,
                    remote_port = remote_port,
                    "Connection established"
                );

                let mut active_count = app_state.active_clients.lock().await;
                if app_state.max_clients > 0 && *active_count >= app_state.max_clients {
                    current_state = ConnectionState::LimitReached;
                } else {
                    *active_count += 1;
                    added_to_active_count = true;
                    current_state = ConnectionState::WaitingForAuthentication;
                }
                drop(active_count);
                tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
            }
            ConnectionState::LimitReached => {
                error!(connection_id = %connection_id, "Max clients limit reached, rejecting connection");
                current_state = ConnectionState::Disconnecting;
                tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
            }
            ConnectionState::WaitingForAuthentication => {
                let mut shutdown_rx = app_state.shutdown_tx.subscribe();
                if app_state.auth_mode == AuthMode::Unsafe {
                    current_state = ConnectionState::WaitingForWebsocket;
                    tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                } else if app_state.auth_mode == AuthMode::Token {
                    select! {
                        _ = shutdown_rx.recv() => {
                            info!(connection_id = %connection_id, "Shutdown signal received while waiting for auth");
                            current_state = ConnectionState::Disconnecting;
                            tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                        }
                        msg = socket_rx.next() => {
                            if let Some(msg) = msg {
                                if let Ok(Message::Text(text)) = msg {
                                    if let Ok(auth_msg) = serde_json::from_str::<AuthMessage>(&text) {
                                        if Some(auth_msg.auth_token) == app_state.auth_token {
                                            info!(connection_id = %connection_id, "Authentication successful");
                                            current_state = ConnectionState::WaitingForWebsocket;
                                            tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                                            continue;
                                        }
                                    }
                                }
                            }
                            error!(connection_id = %connection_id, "Authentication failed");
                            current_state = ConnectionState::Disconnecting;
                            tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                        }
                    }
                } else {
                    error!(connection_id = %connection_id, "Unknown auth mode");
                    current_state = ConnectionState::Disconnecting;
                    tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                }
            }
            ConnectionState::WaitingForWebsocket => {
                let mut shutdown_rx = app_state.shutdown_tx.subscribe();
                select! {
                    _ = shutdown_rx.recv() => {
                        info!(connection_id = %connection_id, "Shutdown signal received while waiting for init");
                        current_state = ConnectionState::Disconnecting;
                        tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                    }
                    msg = socket_rx.next() => {
                        if let Some(msg) = msg {
                            if let Ok(msg) = msg {
                                let data = match msg {
                                    Message::Text(t) => t.as_str().as_bytes().to_vec(),
                                    Message::Binary(b) => b.to_vec(),
                                    _ => {
                                        current_state = ConnectionState::Disconnecting;
                                        tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                                        continue;
                                    }
                                };

                                let (columns, rows) = if data.starts_with(b"{") {
                                    if let Ok(init) = serde_json::from_slice::<InitMessage>(&data) {
                                        (init.columns, init.rows)
                                    } else {
                                        (80, 24)
                                    }
                                } else {
                                    (80, 24)
                                };

                                current_state = ConnectionState::Active(columns, rows);
                                tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                            } else {
                                current_state = ConnectionState::Disconnecting;
                                tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                            }
                        } else {
                            current_state = ConnectionState::Disconnecting;
                            tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                        }
                    }
                }
            }
            ConnectionState::Active(columns, rows) => {
                let (pty, pts) = match pty_process::open() {
                    Ok(res) => res,
                    Err(e) => {
                        error!(connection_id = %connection_id, "Failed to open PTY: {}", e);
                        current_state = ConnectionState::Disconnecting;
                        tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                        continue;
                    }
                };
                let _ = pty.resize(pty_process::Size::new(rows.max(1), columns.max(1)));

                let command = Command::new(&app_state.command);
                let command = command.args(&app_state.args);
                let command = command.env("TERM", "xterm-256color");
                let command = command.env("COLORTERM", "truecolor");
                let mut child = match command.spawn(pts) {
                    Ok(c) => c,
                    Err(e) => {
                        error!(connection_id = %connection_id, "Failed to spawn process: {}", e);
                        current_state = ConnectionState::Disconnecting;
                        tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
                        continue;
                    }
                };

                let (mut pty_read, mut pty_write) = pty.into_split();

                let title_msg = format!("1{} (ttyd-rs)", app_state.command);
                let _ = socket_tx
                    .send(Message::Binary(title_msg.into_bytes().into()))
                    .await;
                let _ = socket_tx.send(Message::Binary(b"2{}"[..].into())).await;

                let mut buf = [0u8; 8192];
                let mut shutdown_rx = app_state.shutdown_tx.subscribe();

                loop {
                    select! {
                        _ = shutdown_rx.recv() => {
                            info!(connection_id = %connection_id, "Shutdown signal received, closing active connection");
                            current_state = ConnectionState::Disconnecting;
                            break;
                        }
                        result = socket_rx.next() => {
                            let msg = match result {
                                Some(Ok(msg)) => msg,
                                _ => {
                                    current_state = ConnectionState::Disconnecting;
                                    break;
                                }
                            };

                            let data = match msg {
                                Message::Text(t) => t.as_str().as_bytes().to_vec(),
                                Message::Binary(b) => b.to_vec(),
                                _ => continue,
                            };

                            if data.is_empty() { continue; }

                            match data[0] {
                                b'0' => { // INPUT
                                    if pty_write.write_all(&data[1..]).await.is_err() {
                                        current_state = ConnectionState::Disconnecting;
                                        break;
                                    }
                                }
                                b'1' => { // RESIZE_TERMINAL
                                    if let Ok(resize) = serde_json::from_slice::<ResizeMessage>(&data[1..]) {
                                        let _ = pty_write.resize(pty_process::Size::new(resize.rows.max(1), resize.columns.max(1)));
                                    }
                                }
                                _ => {}
                            }
                        }
                        read_result = pty_read.read(&mut buf) => {
                            match read_result {
                                Ok(0) | Err(_) => {
                                    current_state = ConnectionState::Disconnecting;
                                    break;
                                }
                                Ok(n) => {
                                    let mut payload = Vec::with_capacity(n + 1);
                                    payload.push(b'0');
                                    payload.extend_from_slice(&buf[..n]);
                                    if socket_tx.send(Message::Binary(payload.into())).await.is_err() {
                                        current_state = ConnectionState::Disconnecting;
                                        break;
                                    }
                                }
                            }
                        }
                        _ = child.wait() => {
                            current_state = ConnectionState::Disconnecting;
                            break;
                        }
                    }
                }

                let _ = child.kill().await;
                tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
            }
            ConnectionState::Disconnecting => {
                let _ = socket_tx.close().await;
                current_state = ConnectionState::Disconnected;
                tracing::debug!(connection_id = %connection_id, "State changed to: {:?}", current_state);
            }
            ConnectionState::Disconnected => {
                info!(
                    connection_id = %connection_id,
                    remote_ip = %remote_ip,
                    remote_port = remote_port,
                    "Connection disconnected"
                );

                if added_to_active_count {
                    let mut active_count = app_state.active_clients.lock().await;
                    if *active_count > 0 {
                        *active_count -= 1;
                    }

                    if app_state.exit_no_conn && *active_count == 0 {
                        info!("All clients disconnected. Shutting down server...");
                        let _ = app_state.shutdown_tx.send(());
                    }
                    drop(active_count);
                }

                break;
            }
        }
    }
}
