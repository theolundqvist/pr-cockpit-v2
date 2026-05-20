use anyhow::Result;
use desktop_lib::api::check_logs::{stream_check_run_log, CheckLogStreamRequest, LogChunk};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "support/mod.rs"]
mod support;

#[tokio::test]
async fn streams_chunks_and_emits_tail_buffer() -> Result<()> {
    let server = MockServer::start().await;
    let harness = support::build_harness(&server.uri(), "token", "check-log").await?;

    let log_body = (1..=1200_i64)
        .map(|line| format!("line-{line:04}\n"))
        .collect::<String>();
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello-world/actions/jobs/7001/logs"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/blob/logs/7001", server.uri())),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/blob/logs/7001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(log_body.clone()))
        .mount(&server)
        .await;

    let mut emitted = Vec::<LogChunk>::new();
    stream_check_run_log(
        &harness.github,
        CheckLogStreamRequest {
            account_id: &harness.account_id,
            owner: "octo",
            repo: "hello-world",
            check_run_id: "run-checks",
            details_url: Some("https://github.com/octo/hello-world/actions/runs/1/jobs/7001"),
            tail_lines: 5,
        },
        |chunk| {
            emitted.push(chunk);
            Ok(())
        },
    )
    .await?;

    assert!(emitted.iter().any(|chunk| chunk.kind == "chunk"));
    let tail = emitted
        .iter()
        .find(|chunk| chunk.kind == "tail")
        .and_then(|chunk| chunk.text.clone())
        .expect("tail chunk must be emitted");
    assert!(tail.contains("line-1196"));
    assert!(tail.contains("line-1200"));
    assert!(emitted.iter().any(|chunk| chunk.kind == "done"));
    Ok(())
}

#[tokio::test]
async fn falls_back_for_non_actions_checks() -> Result<()> {
    let server = MockServer::start().await;
    let harness = support::build_harness(&server.uri(), "token", "check-log-fallback").await?;
    let mut emitted = Vec::<LogChunk>::new();
    stream_check_run_log(
        &harness.github,
        CheckLogStreamRequest {
            account_id: &harness.account_id,
            owner: "octo",
            repo: "hello-world",
            check_run_id: "run-checks",
            details_url: Some("https://ci.example.test/build/123"),
            tail_lines: 100,
        },
        |chunk| {
            emitted.push(chunk);
            Ok(())
        },
    )
    .await?;
    assert!(emitted.iter().any(|chunk| chunk.kind == "fallback"));
    assert!(emitted.iter().any(|chunk| chunk.kind == "done"));
    Ok(())
}
