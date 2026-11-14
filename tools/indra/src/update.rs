//! `indra update`: replace this binary with the newest release (or a chosen
//! one), after checking its SHA-256, the same way the install scripts do.

use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderValue};
use clap::Args;
use serde_json::Value;
use std::process::ExitCode;

const REPO: &str = "dmdhrumilmistry/indra";

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Only say whether a newer release exists
    #[arg(long)]
    check: bool,
    /// Install this release (e.g. indra-v0.2.0, or 0.2.0) instead of the newest
    #[arg(long)]
    version: Option<String>,
    /// Reinstall even when this is already the newest
    #[arg(long)]
    force: bool,
}

/// The release asset built for this machine (see .github/workflows/indra.yml).
pub fn asset() -> Option<&'static str> {
    let a = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "indra-x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "indra-aarch64-unknown-linux-musl",
        ("macos", "x86_64") => "indra-x86_64-apple-darwin",
        ("macos", "aarch64") => "indra-aarch64-apple-darwin",
        ("windows", "x86_64") => "indra-x86_64-pc-windows-msvc.exe",
        _ => return None,
    };
    Some(a)
}

/// `0.2.10` as (0, 2, 10); anything after a `-` is ignored.
fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim_start_matches("indra-v").trim_start_matches('v');
    let mut it = v
        .split('-')
        .next()?
        .split('.')
        .map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
}

/// GET a URL, following redirects (release downloads redirect to a CDN).
async fn get(url: &str) -> Result<Bytes, String> {
    let mut url = url.to_string();
    for _ in 0..6 {
        let uri: axum::http::Uri = url.parse().map_err(|_| format!("bad URL {}", url))?;
        let base = format!(
            "{}://{}",
            uri.scheme_str().unwrap_or("https"),
            uri.authority().map(|a| a.as_str()).unwrap_or("")
        );
        let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
        let mut h = HeaderMap::new();
        h.insert("user-agent", HeaderValue::from_static("indra-update"));
        h.insert("accept", HeaderValue::from_static("*/*"));
        let resp = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            crate::deck::router::forward(&base, "GET", path, &h, Bytes::new(), true),
        )
        .await
        .map_err(|_| format!("{}: timed out", url))??;
        let st = resp.status();
        if st.is_redirection() {
            let next = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or("a redirect without a location")?
                .to_string();
            url = if next.starts_with('/') {
                format!("{}{}", base, next)
            } else {
                next
            };
            continue;
        }
        let body = axum::body::to_bytes(resp.into_body(), 1 << 30)
            .await
            .map_err(|e| e.to_string())?;
        if !st.is_success() {
            return Err(format!("{} answered {}", url, st));
        }
        return Ok(body);
    }
    Err("too many redirects".into())
}

/// The newest indra release's tag (the repository has other releases too).
async fn newest(api: &str) -> Result<String, String> {
    let body = get(&format!("{}?per_page=50", api)).await?;
    let list: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    list.as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["draft"] != true && r["prerelease"] != true)
        .filter_map(|r| r["tag_name"].as_str())
        .find(|t| t.starts_with("indra-v"))
        .map(String::from)
        .ok_or_else(|| "no indra release found".to_string())
}

pub fn run(a: UpdateArgs) -> Result<ExitCode, String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(go(a))
}

async fn go(a: UpdateArgs) -> Result<ExitCode, String> {
    let api = std::env::var("INDRA_RELEASES_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{}/releases", REPO));
    let downloads = std::env::var("INDRA_DOWNLOAD_URL")
        .unwrap_or_else(|_| format!("https://github.com/{}/releases/download", REPO));
    let asset =
        asset().ok_or("there is no prebuilt indra for this machine; build it from source")?;
    let tag = match &a.version {
        Some(v) if v.starts_with("indra-v") => v.clone(),
        Some(v) => format!("indra-v{}", v.trim_start_matches('v')),
        None => newest(&api).await?,
    };
    let here = env!("CARGO_PKG_VERSION");
    let newer = match (parse(&tag), parse(here)) {
        (Some(t), Some(h)) => t > h,
        _ => true,
    };
    if a.check {
        println!(
            "{}",
            if newer {
                format!(
                    "indra {} is installed; {} is available: run `indra update`",
                    here, tag
                )
            } else {
                format!("indra {} is the newest ({})", here, tag)
            }
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !newer && !a.force && a.version.is_none() {
        println!("indra {} is already the newest ({}).", here, tag);
        return Ok(ExitCode::SUCCESS);
    }
    eprintln!("indra: downloading {} {} ...", tag, asset);
    let bin = get(&format!("{}/{}/{}", downloads, tag, asset)).await?;
    let sum = get(&format!("{}/{}/{}.sha256", downloads, tag, asset)).await?;
    let want = String::from_utf8_lossy(&sum)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    let got = {
        use sha2::{Digest, Sha256};
        Sha256::digest(&bin)
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    };
    if want.len() != 64 || want != got {
        return Err(format!(
            "checksum mismatch (expected {}, got {}); nothing was changed",
            want, got
        ));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let dir = exe.parent().ok_or("cannot tell where indra is installed")?;
    let fresh = dir.join(format!(".indra-update-{}", std::process::id()));
    std::fs::write(&fresh, &bin).map_err(|e| {
        format!(
            "cannot write to {} ({}); reinstall with the install script, or run this with permission to write there",
            dir.display(),
            e
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    // A running program can be renamed (Windows) or replaced (Unix) in place.
    #[cfg(windows)]
    {
        let old = exe.with_extension("old.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    }
    if let Err(e) = std::fs::rename(&fresh, &exe) {
        let _ = std::fs::remove_file(&fresh);
        return Err(format!("cannot replace {}: {}", exe.display(), e));
    }
    println!("Updated indra {} -> {} ({})", here, tag, exe.display());
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_assets() {
        assert_eq!(parse("indra-v0.2.10"), Some((0, 2, 10)));
        assert_eq!(parse("0.3"), Some((0, 3, 0)));
        assert_eq!(parse("v1.0.0-rc1"), Some((1, 0, 0)));
        assert!(parse("indra-v0.10.0") > parse("0.9.9"));
        assert_eq!(parse("nope"), None);
        assert!(asset().is_some(), "every CI platform has a release asset");
    }

    #[tokio::test]
    async fn finds_the_newest_indra_release_and_follows_redirects() {
        let app = axum::Router::new()
            .route(
                "/releases",
                axum::routing::get(|| async {
                    axum::Json(serde_json::json!([
                        { "tag_name": "indra-v0.4.0", "draft": true },
                        { "tag_name": "v0.2.0" },
                        { "tag_name": "indra-v0.3.0-rc1", "prerelease": true },
                        { "tag_name": "indra-v0.3.0" },
                        { "tag_name": "indra-v0.2.0" }
                    ]))
                }),
            )
            .route(
                "/moved",
                axum::routing::get(|| async { axum::response::Redirect::temporary("/file") }),
            )
            .route("/file", axum::routing::get(|| async { "binary" }));
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        assert_eq!(
            newest(&format!("{}/releases", url)).await.unwrap(),
            "indra-v0.3.0"
        );
        assert_eq!(
            &get(&format!("{}/moved", url)).await.unwrap()[..],
            b"binary"
        );
        assert!(get(&format!("{}/missing", url)).await.is_err());
    }
}
