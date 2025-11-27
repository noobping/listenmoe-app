use futures_util::StreamExt;
use serde_json::Value;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio_tungstenite::connect_async;

/// Track info sent to the UI thread.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub artist: String,
    pub title: String,
}

pub struct Meta {
    sender: Sender<TrackInfo>,
    running: Arc<AtomicBool>,
}

impl Meta {
    /// Create a new Meta, using the given channel to send track updates.
    pub fn new(sender: Sender<TrackInfo>) -> Rc<Self> {
        Rc::new(Self {
            sender,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start the background websocket/metadata loop.
    pub fn start(self: &Rc<Self>) {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            // already running
            return;
        }

        let running = self.running.clone();
        let sender = self.sender.clone();

        thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create Tokio runtime for Meta");

            if let Err(err) = rt.block_on(run_meta_loop(sender, running.clone())) {
                eprintln!("[meta] Fatal error in metadata loop: {err}");
            }
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Outer loop: reconnect if needed.
async fn run_meta_loop(
    sender: Sender<TrackInfo>,
    running: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while running.load(Ordering::SeqCst) {
        match run_once(sender.clone(), running.clone()).await {
            Ok(()) => {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(err) => {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                eprintln!("[meta] connection error: {err}, retrying in 5s…");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }

    Ok(())
}

/// Single websocket session.
async fn run_once(
    sender: Sender<TrackInfo>,
    running: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !running.load(Ordering::SeqCst) {
        return Ok(());
    }

    let (ws_stream, _) = connect_async("wss://listen.moe/gateway_v2").await?;
    eprintln!("[meta] Connected to LISTEN.moe gateway");

    let (_write, mut read) = ws_stream.split();

    // 1. Read hello (op 0)
    if let Some(msg) = read.next().await {
        let msg = msg?;
        if msg.is_text() {
            let _txt = msg.into_text()?;
        }
    }

    // 2. Process messages, look for TRACK_UPDATE
    while running.load(Ordering::SeqCst) {
        let Some(msg) = read.next().await else {
            break;
        };

        let msg = msg?;
        if !msg.is_text() {
            continue;
        }

        let txt = msg.into_text()?;
        let json: Value = match serde_json::from_str(&txt) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("[meta] JSON parse error: {err}");
                continue;
            }
        };

        let op = json["op"].as_i64().unwrap_or(-1);
        let t = json["t"].as_str().unwrap_or("");

        if op == 1 && t == "TRACK_UPDATE" {
            if let Some(info) = parse_track_info(&json) {
                let _ = sender.send(info);
            }
        }
    }

    Ok(())
}

/// Extract artist(s) + title from JSON.
fn parse_track_info(json: &Value) -> Option<TrackInfo> {
    let song = json.get("d")?.get("song")?;

    let title = song
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown title")
        .to_string();

    let artists: Vec<String> = song
        .get("artists")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                .map(|s| s.to_owned())
                .collect::<Vec<String>>()
        })
        .unwrap_or_else(Vec::new);

    let artist = if artists.is_empty() {
        "Unknown artist".to_string()
    } else {
        artists.join(", ")
    };

    Some(TrackInfo { artist, title })
}
