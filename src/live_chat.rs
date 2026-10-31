use crate::auth::AuthSession;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, ORIGIN, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};

const MAX_CHAT_MESSAGES: usize = 150;
const YOUTUBE_ORIGIN: &str = "https://www.youtube.com";
const USER_AGENT_STR: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub author_name: String,
    pub author_avatar: String,
    pub message: String,
    pub timestamp_str: String,
    pub is_owner: bool,
    pub is_moderator: bool,
}

#[allow(dead_code)]
pub struct LiveChatManager {
    pub messages: Arc<Mutex<VecDeque<ChatMessage>>>,
    pub continuation: Arc<RwLock<Option<String>>>,
    pub is_running: Arc<RwLock<bool>>,
    generation: Arc<std::sync::atomic::AtomicU64>,
    session: Arc<RwLock<AuthSession>>,
    client: reqwest::Client,
}

#[allow(dead_code)]
impl LiveChatManager {
    pub fn new(session: Arc<RwLock<AuthSession>>) -> Self {
        Self {
            messages: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_CHAT_MESSAGES))),
            continuation: Arc::new(RwLock::new(None)),
            is_running: Arc::new(RwLock::new(false)),
            generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            session,
            client: reqwest::Client::builder()
                .gzip(true)
                .brotli(true)
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub async fn set_continuation(&self, token: Option<String>) {
        let mut cont = self.continuation.write().await;
        *cont = token;
        let mut msgs = self.messages.lock().await;
        msgs.clear();
    }

    /// Constant-memory insertion: ensures size never exceeds 150
    pub async fn push_message(&self, msg: ChatMessage) {
        let mut queue = self.messages.lock().await;
        if queue.len() >= MAX_CHAT_MESSAGES {
            queue.pop_front();
        }
        queue.push_back(msg);
    }

    pub async fn get_messages(&self) -> Vec<ChatMessage> {
        let queue = self.messages.lock().await;
        queue.iter().cloned().collect()
    }

    async fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_STR));
        headers.insert(ORIGIN, HeaderValue::from_static(YOUTUBE_ORIGIN));
        headers.insert("X-Origin", HeaderValue::from_static(YOUTUBE_ORIGIN));
        headers.insert("X-Goog-AuthUser", HeaderValue::from_static("0"));
        headers.insert("X-Youtube-Client-Name", HeaderValue::from_static("1"));
        headers.insert("X-Youtube-Client-Version", HeaderValue::from_static("2.20261015.01.00"));

        let session = self.session.read().await;
        let cookie_str = session.cookie_header();
        if !cookie_str.is_empty() {
            if let Ok(c) = HeaderValue::from_str(&cookie_str) {
                headers.insert(COOKIE, c);
            }
        }
        if let Some(auth_val) = session.get_authorization_header() {
            if let Ok(a) = HeaderValue::from_str(&auth_val) {
                headers.insert(AUTHORIZATION, a);
            }
        }
        headers
    }

    /// Starts background polling for live streams respecting timeoutMs
    pub fn start_polling(&self) {
        let my_gen = self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let gen_clone = self.generation.clone();
        let continuation = self.continuation.clone();
        let messages = self.messages.clone();
        let is_running = self.is_running.clone();
        let client = self.client.clone();
        let session = self.session.clone();

        tokio::spawn(async move {
            {
                let mut run = is_running.write().await;
                *run = true;
            }

            while *is_running.read().await {
                if gen_clone.load(std::sync::atomic::Ordering::Relaxed) != my_gen {
                    break;
                }
                let token = {
                    let cont = continuation.read().await;
                    cont.clone()
                };

                let token_str = match token {
                    Some(t) if !t.is_empty() => t,
                    _ => {
                        sleep(Duration::from_millis(1000)).await;
                        continue;
                    }
                };

                let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/live_chat/get_live_chat?prettyPrint=false");
                let body = serde_json::json!({
                    "context": {
                        "client": {
                            "clientName": "WEB",
                            "clientVersion": "2.20261015.01.00",
                            "hl": "es",
                            "gl": "ES"
                        }
                    },
                    "continuation": token_str
                });

                let mut headers = HeaderMap::new();
                headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_STR));
                headers.insert(ORIGIN, HeaderValue::from_static(YOUTUBE_ORIGIN));
                headers.insert("X-Origin", HeaderValue::from_static(YOUTUBE_ORIGIN));
                headers.insert("X-Youtube-Client-Name", HeaderValue::from_static("1"));
                headers.insert("X-Youtube-Client-Version", HeaderValue::from_static("2.20261015.01.00"));

                {
                    let s = session.read().await;
                    let cookie_str = s.cookie_header();
                    if !cookie_str.is_empty() {
                        if let Ok(c) = HeaderValue::from_str(&cookie_str) {
                            headers.insert(COOKIE, c);
                        }
                    }
                    if let Some(a_str) = s.get_authorization_header() {
                        if let Ok(a) = HeaderValue::from_str(&a_str) {
                            headers.insert(AUTHORIZATION, a);
                        }
                    }
                }

                let mut poll_delay_ms = 2500u64;

                if let Ok(resp) = client.post(&url).headers(headers).json(&body).send().await {
                    if let Ok(json_val) = resp.json::<Value>().await {
                        // Extract new actions (messages)
                        if let Some(actions) = json_val
                            .get("continuationContents")
                            .and_then(|c| c.get("liveChatContinuation"))
                            .and_then(|l| l.get("actions"))
                            .and_then(|a| a.as_array())
                        {
                            let mut queue = messages.lock().await;
                            for action in actions {
                                if let Some(item) = action
                                    .get("addChatItemAction")
                                    .and_then(|a| a.get("item"))
                                    .and_then(|i| i.get("liveChatTextMessageRenderer"))
                                {
                                    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let author_name = item
                                        .get("authorName")
                                        .and_then(|a| a.get("simpleText"))
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("Usuario")
                                        .to_string();

                                    let author_avatar = item
                                        .get("authorPhoto")
                                        .and_then(|p| p.get("thumbnails"))
                                        .and_then(|arr| arr.as_array())
                                        .and_then(|arr| arr.last())
                                        .and_then(|t| t.get("url"))
                                        .and_then(|u| u.as_str())
                                        .unwrap_or("")
                                        .to_string();

                                    let message = item
                                        .get("message")
                                        .and_then(|m| m.get("runs"))
                                        .and_then(|arr| arr.as_array())
                                        .map(|runs| {
                                            runs.iter()
                                                .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                                                .collect::<Vec<_>>()
                                                .join("")
                                        })
                                        .unwrap_or_default();

                                    let timestamp_str = item
                                        .get("timestampText")
                                        .and_then(|t| t.get("simpleText"))
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("")
                                        .to_string();

                                    let is_owner = item.get("authorBadges").and_then(|b| b.as_array()).map(|arr| {
                                        arr.iter().any(|badge| {
                                            crate::innertube::json_contains_str(badge, "OWNER")
                                        })
                                    }).unwrap_or(false);

                                    let is_moderator = item.get("authorBadges").and_then(|b| b.as_array()).map(|arr| {
                                        arr.iter().any(|badge| {
                                            crate::innertube::json_contains_str(badge, "MODERATOR")
                                        })
                                    }).unwrap_or(false);

                                    if !message.is_empty() {
                                        if queue.len() >= MAX_CHAT_MESSAGES {
                                            queue.pop_front();
                                        }
                                        queue.push_back(ChatMessage {
                                            id,
                                            author_name,
                                            author_avatar,
                                            message,
                                            timestamp_str,
                                            is_owner,
                                            is_moderator,
                                        });
                                    }
                                }
                            }
                        }

                        // Next continuation token and timeout
                        if let Some(next_cont) = json_val
                            .get("continuationContents")
                            .and_then(|c| c.get("liveChatContinuation"))
                            .and_then(|l| l.get("continuations"))
                            .and_then(|arr| arr.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|c| c.get("timedContinuationData").or_else(|| c.get("invalidationContinuationData")))
                        {
                            if let Some(new_token) = next_cont.get("continuation").and_then(|t| t.as_str()) {
                                let mut cont_lock = continuation.write().await;
                                *cont_lock = Some(new_token.to_string());
                            }
                            if let Some(timeout) = next_cont.get("timeoutMs").and_then(|t| t.as_u64()) {
                                poll_delay_ms = timeout.max(500);
                            }
                        }
                    }
                }

                sleep(Duration::from_millis(poll_delay_ms)).await;
            }
        });
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let is_running = self.is_running.clone();
        let messages = self.messages.clone();
        let continuation = self.continuation.clone();
        tokio::spawn(async move {
            *is_running.write().await = false;
            *continuation.write().await = None;
            messages.lock().await.clear();
        });
    }

    pub async fn stop_polling(&self) {
        self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut run = self.is_running.write().await;
        *run = false;
        let mut cont = self.continuation.write().await;
        *cont = None;
        let mut msgs = self.messages.lock().await;
        msgs.clear();
    }
}
