use axum::{Router, routing::get};
use clap::{Parser, ValueEnum};
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod assets_routes;
mod ws_handler;

#[derive(Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum AuthMode {
    Unsafe,
    Token,
}

#[derive(Parser, Debug)]
#[command(name = "ttyd-rs", version = include_str!("assets/version.txt").trim(), about = "Share your terminal over the web")]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value_t = 7681)]
    port: u16,

    /// Interfaces to bind to. Can be specified multiple times.
    #[arg(short, long, default_values_t = vec!["127.0.0.1".to_string()])]
    bind: Vec<String>,

    /// Bind to all interfaces (0.0.0.0 / [::])
    #[arg(long, default_value_t = false)]
    bind_all: bool,

    /// Log in JSON format
    #[arg(long, default_value_t = false)]
    log_json: bool,

    /// Enable debug logging
    #[arg(long, default_value_t = false)]
    debug: bool,

    /// Accept only one client and exit on disconnect (same as --max-clients=1 and --exit-no-conn)
    #[arg(long, default_value_t = false)]
    once: bool,

    /// Limit the maximum clients which can connect at the same time (0 = no limit)
    #[arg(long, default_value_t = 10)]
    max_clients: usize,

    /// Exit after the last client disconnected
    #[arg(long, default_value_t = false)]
    exit_no_conn: bool,

    /// Authentication mode: unsafe (no authentication), token (requires token to login)
    #[arg(long, value_enum, default_value_t = AuthMode::Token)]
    auth_mode: AuthMode,

    /// Token to use for authentication (if --auth-mode=token). If not provided, a random UUID will be generated.
    #[arg(long)]
    auth_token: Option<String>,

    /// Command and arguments to run (default: bash)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command_and_args: Vec<String>,
}

#[derive(Clone)]
pub struct AppState {
    command: String,
    args: Vec<String>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    pub auth_mode: AuthMode,
    pub auth_token: Option<String>,
    pub version: String,
    pub exit_no_conn: bool,
    pub max_clients: usize,
    pub active_clients: Arc<tokio::sync::Mutex<usize>>,
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(unix)]
    let hangup = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    #[cfg(not(unix))]
    let hangup = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
        _ = hangup => {},
    }
}

async fn main_internal(cli: Cli) -> anyhow::Result<()> {
    let port = cli.port;
    let (command, cmd_args) = if cli.command_and_args.is_empty() {
        ("bash".to_string(), vec![])
    } else {
        (
            cli.command_and_args[0].clone(),
            cli.command_and_args[1..].to_vec(),
        )
    };

    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    let version = String::from_utf8_lossy(include_bytes!("assets/version.txt"))
        .trim()
        .to_string();
    info!("TTYD in Rust - v{}", version);

    let mut final_token = cli.auth_token.clone();
    if cli.auth_mode == AuthMode::Token {
        let token = if let Some(ref t) = final_token {
            if uuid::Uuid::parse_str(t).is_err() {
                anyhow::bail!("Provided auth token is not a valid UUID: {}", t);
            }
            t.clone()
        } else {
            let generated_token = uuid::Uuid::new_v4().to_string();
            final_token = Some(generated_token.clone());
            generated_token
        };

        info!("Authentication token: {}", token);
    }

    let mut serve_futures = Vec::new();

    let mut bind_addrs = cli.bind.clone();
    if cli.bind_all {
        bind_addrs = vec!["[::]".to_string(), "0.0.0.0".to_string()];
    }

    let mut max_clients = cli.max_clients;
    let mut exit_no_conn = cli.exit_no_conn;

    if cli.once {
        max_clients = 1;
        exit_no_conn = true;
    }

    if max_clients > 0 {
        info!("Max clients limit set to: {}", max_clients);
    }
    if exit_no_conn {
        info!("Server will exit after all clients disconnected");
    }

    let active_clients = Arc::new(tokio::sync::Mutex::new(0));

    for bind_addr in bind_addrs {
        let addr = format!("{}:{}", bind_addr, port);

        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                if cli.bind_all {
                    // Ignore errors when trying to bind all (e.g. if IPv6 is disabled on the host)
                    continue;
                } else {
                    anyhow::bail!("Failed to bind to {}: {}", addr, e);
                }
            }
        };

        let local_addr = listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        info!("Server running at http://{}", local_addr);

        if cli.auth_mode == AuthMode::Token {
            let token_str = final_token.as_ref().unwrap();
            let host_ip = if bind_addr == "0.0.0.0" || bind_addr == "[::]" {
                "127.0.0.1" // Better clickable default
            } else {
                &bind_addr
            };
            info!(
                "Access the console at: http://{}:{}?token={}",
                host_ip, port, token_str
            );
        }

        let state = Arc::new(AppState {
            command: command.clone(),
            args: cmd_args.clone(),
            shutdown_tx: shutdown_tx.clone(),
            auth_mode: cli.auth_mode.clone(),
            auth_token: final_token.clone(),
            version: version.clone(),
            exit_no_conn,
            max_clients,
            active_clients: active_clients.clone(),
        });

        let app = Router::new()
            .route("/", get(assets_routes::index))
            .route("/favicon.png", get(assets_routes::favicon))
            .route("/fonts/jetbrains-mono-regular.woff2", get(assets_routes::font_regular))
            .route("/fonts/jetbrains-mono-bold.woff2", get(assets_routes::font_bold))
            .route("/fonts/jetbrains-mono-italic.woff2", get(assets_routes::font_italic))
            .route("/fonts/jetbrains-mono-bold-italic.woff2", get(assets_routes::font_bold_italic))
            .route("/config", get(assets_routes::config))
            .route("/ws", get(ws_handler::ws_handler))
            .with_state(state)
            .into_make_service_with_connect_info::<std::net::SocketAddr>();

        let shutdown_rx = shutdown_tx.subscribe();

        serve_futures.push(tokio::spawn(async move {
            let mut rx = shutdown_rx;
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.recv().await;
                })
                .await;
        }));
    }

    if serve_futures.is_empty() {
        anyhow::bail!("Failed to start server: Could not bind to any interface.");
    }

    tokio::select! {
        _ = shutdown_signal() => {
            info!("Shutdown signal received, shutting down gracefully...");
        }
        _ = futures::future::join_all(&mut serve_futures) => {
             // this shouldn't happen unless listeners fail, but we can handle it
        }
    };

    let _ = shutdown_tx.send(());

    // Give some time for ongoing graceful disconnections to finish
    // We don't join the futures again because they may have already completed in the select macro.
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let default_level = if cli.debug {
        "ttyd_rs=debug"
    } else {
        "ttyd_rs=info"
    };

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_level.into());

    if cli.log_json {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    if let Err(e) = main_internal(cli).await {
        error!("Fatal error: {}", e);
        std::process::exit(1);
    }
}
