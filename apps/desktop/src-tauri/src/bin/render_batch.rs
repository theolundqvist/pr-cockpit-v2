use std::io::{self, Read};

use pr_cockpit_desktop::render::{render_comment, RenderCtx};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct RenderInput {
    id: String,
    body: String,
    repo: Option<String>,
}

#[derive(Debug, Serialize)]
struct RenderOutput {
    id: String,
    html: String,
}

fn main() -> anyhow::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let records: Vec<RenderInput> = serde_json::from_str(&input)?;
    let mut output = Vec::with_capacity(records.len());
    for record in records {
        let ctx = RenderCtx {
            repo: record.repo.as_deref(),
            cache: None,
        };
        let rendered = render_comment(&record.body, &ctx);
        output.push(RenderOutput {
            id: record.id,
            html: rendered.html,
        });
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
