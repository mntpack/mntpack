use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SearchResponse {
    items: Vec<SearchRepo>,
}

#[derive(Debug, Deserialize)]
struct SearchRepo {
    full_name: String,
    stargazers_count: u64,
    language: Option<String>,
}

pub async fn execute(query_parts: &[String]) -> Result<()> {
    if query_parts.is_empty() {
        bail!("search query cannot be empty");
    }

    let query = query_parts.join(" ");
    let encoded = query.replace(' ', "+");
    let url = format!("https://api.github.com/search/repositories?q={encoded}&per_page=15");
    let client = reqwest::Client::builder()
        .user_agent("mntpack/0.1")
        .build()
        .context("failed to create http client")?;

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("failed to query github search api: {url}"))?
        .error_for_status()
        .context("github search request failed")?
        .json::<SearchResponse>()
        .await
        .context("failed to parse github search response")?;

    if response.items.is_empty() {
        println!("no repositories found for '{query}'");
        return Ok(());
    }

    println!("owner/repo\tstars\tlang");
    println!("----------------------------------------");
    for item in response.items {
        let language = item.language.unwrap_or_else(|| "-".to_string());
        println!(
            "{}\t{}\t{}",
            item.full_name,
            format_stars(item.stargazers_count),
            language
        );
    }

    Ok(())
}

fn format_stars(stars: u64) -> String {
    if stars >= 1_000_000 {
        return format!("{:.1}m", stars as f64 / 1_000_000.0);
    }
    if stars >= 1_000 {
        return format!("{:.1}k", stars as f64 / 1_000.0);
    }
    stars.to_string()
}
