use anyhow::{anyhow, Result};
use desktop_lib::ipc::upload_image_to_github_user_content_impl;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "support/mod.rs"]
mod support;

#[tokio::test]
async fn upload_reuses_cached_url_for_identical_image_bytes() -> Result<()> {
    let server = MockServer::start().await;
    let login = "img-cache-user";
    let harness = support::build_harness(&server.uri(), "token", login).await?;

    Mock::given(method("POST"))
        .and(path(format!("/upload/assets/users/{login}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "url": "https://user-images.githubusercontent.com/42/cache-hit.png",
            "alt": "cache-hit"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let image = vec![137, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3, 4];
    let first = upload_image_to_github_user_content_impl(
        harness.db.as_ref(),
        &harness.github,
        harness.account_id.clone(),
        image.clone(),
        "image/png".to_string(),
    )
    .await
    .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;
    let second = upload_image_to_github_user_content_impl(
        harness.db.as_ref(),
        &harness.github,
        harness.account_id.clone(),
        image,
        "image/png".to_string(),
    )
    .await
    .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;

    assert_eq!(first.url, second.url);
    assert_eq!(first.content_hash, second.content_hash);
    assert_eq!(first.size_bytes, second.size_bytes);

    Ok(())
}

#[tokio::test]
async fn upload_rejects_non_github_urls() -> Result<()> {
    let server = MockServer::start().await;
    let login = "img-invalid-user";
    let harness = support::build_harness(&server.uri(), "token", login).await?;

    Mock::given(method("POST"))
        .and(path(format!("/upload/assets/users/{login}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "url": "https://evil.example.com/steal.png"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let error = upload_image_to_github_user_content_impl(
        harness.db.as_ref(),
        &harness.github,
        harness.account_id.clone(),
        vec![1, 2, 3, 4, 5],
        "image/png".to_string(),
    )
    .await
    .expect_err("invalid upload URL should be rejected");

    assert_eq!(error.code, "InvalidUploadUrl");
    Ok(())
}
