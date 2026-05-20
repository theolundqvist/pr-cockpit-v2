use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use desktop_lib::relay::{start_receiver, RefetchDispatcher};
use desktop_lib::sync::reconcile::RefetchTarget;
use hmac::{Hmac, Mac};
use tokio::sync::Mutex;

#[derive(Default)]
struct RecordingDispatcher {
    calls: Mutex<Vec<Vec<RefetchTarget>>>,
}

#[async_trait]
impl RefetchDispatcher for RecordingDispatcher {
    async fn enqueue_refetch(&self, targets: Vec<RefetchTarget>) -> Result<()> {
        self.calls.lock().await.push(targets);
        Ok(())
    }
}

fn sign(secret: &str, body: &str, nonce: &str, timestamp: &str) -> String {
    type HmacSha256 = Hmac<sha2::Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("valid hmac key");
    mac.update(body.as_bytes());
    mac.update(b".");
    mac.update(nonce.as_bytes());
    mac.update(b".");
    mac.update(timestamp.as_bytes());
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[tokio::test]
async fn relay_receiver_accepts_valid_and_rejects_stale_or_bad_signatures() -> Result<()> {
    let forward_secret = "relay-forward-secret";
    let dispatcher = Arc::new(RecordingDispatcher::default());
    let receiver = start_receiver(forward_secret.to_string(), dispatcher.clone()).await?;
    let client = reqwest::Client::new();

    let payload = serde_json::json!({
        "repository": {
            "owner": { "login": "octo" },
            "name": "hello-world"
        },
        "pull_request": {
            "number": 42
        }
    })
    .to_string();

    let nonce = "nonce-valid-1";
    let timestamp = (chrono::Utc::now().timestamp()).to_string();
    let signature = sign(forward_secret, &payload, nonce, &timestamp);
    let ok_response = client
        .post(receiver.local_url())
        .header("x-github-event", "pull_request")
        .header("x-relay-signature-256", signature)
        .header("x-relay-nonce", nonce)
        .header("x-relay-timestamp", timestamp)
        .body(payload.clone())
        .send()
        .await?;
    assert_eq!(ok_response.status(), reqwest::StatusCode::OK);

    let calls = dispatcher.calls.lock().await.clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].len(), 1);
    assert_eq!(calls[0][0].owner, "octo");
    assert_eq!(calls[0][0].repo, "hello-world");
    assert_eq!(calls[0][0].number, 42);
    drop(calls);

    let stale_nonce = "nonce-stale-1";
    let stale_timestamp = (chrono::Utc::now().timestamp() - (6 * 60)).to_string();
    let stale_signature = sign(forward_secret, &payload, stale_nonce, &stale_timestamp);
    let stale_response = client
        .post(receiver.local_url())
        .header("x-github-event", "pull_request")
        .header("x-relay-signature-256", stale_signature)
        .header("x-relay-nonce", stale_nonce)
        .header("x-relay-timestamp", stale_timestamp)
        .body(payload.clone())
        .send()
        .await?;
    assert_eq!(stale_response.status(), reqwest::StatusCode::UNAUTHORIZED);

    let bad_nonce = "nonce-bad-1";
    let bad_timestamp = chrono::Utc::now().timestamp().to_string();
    let bad_response = client
        .post(receiver.local_url())
        .header("x-github-event", "pull_request")
        .header("x-relay-signature-256", "sha256=deadbeef")
        .header("x-relay-nonce", bad_nonce)
        .header("x-relay-timestamp", bad_timestamp)
        .body(payload)
        .send()
        .await?;
    assert_eq!(bad_response.status(), reqwest::StatusCode::UNAUTHORIZED);

    assert_eq!(dispatcher.calls.lock().await.len(), 1);
    receiver.shutdown().await;
    Ok(())
}
