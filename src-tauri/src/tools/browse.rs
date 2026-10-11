//! Internet tools: plain fetch, JS-rendered fetch, and SearXNG search.
//!
//! The rendered fetch deliberately reuses the Chromium-family browser already
//! installed on the machine (`--headless=new --dump-dom`) instead of shipping
//! one: no dependency, no 150 MB download, and Edge is present on every
//! Windows box. If no browser is found the tool says so and `fetch_url` still
//! works.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;

use super::exec::{run_process, truncate};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) DeskSeek/0.1";
const MAX_FETCH_BYTES: usize = 512 * 1024;
const BROWSER_TIMEOUT_SECS: u64 = 45;
/// How long the page's JavaScript may run before the DOM is dumped.
const VIRTUAL_TIME_BUDGET_MS: &str = "5000";

fn tidy(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            // At most one blank line between blocks.
            if lines.last().map(|last| !last.is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
        } else {
            lines.push(collapsed);
        }
    }
    while lines.last().map(String::is_empty).unwrap_or(false) {
        lines.pop();
    }
    lines.join("\n")
}

fn decode_entities(text: &str) -> String {
    // `&amp;` goes last so `&amp;lt;` does not decode twice into `<`.
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&rsquo;", "’")
        .replace("&lsquo;", "‘")
        .replace("&ldquo;", "“")
        .replace("&rdquo;", "”")
        .replace("&amp;", "&")
}

fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "br"
            | "li"
            | "tr"
            | "td"
            | "th"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "nav"
            | "aside"
            | "main"
            | "figure"
            | "figcaption"
            | "blockquote"
            | "pre"
            | "table"
            | "ul"
            | "ol"
            | "hr"
            | "form"
            | "dt"
            | "dd"
    )
}

/// Strips tags to readable text: drops `<script>`/`<style>` bodies entirely and
/// turns block-level tags into line breaks.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut skipping: Option<&'static str> = None;
    let mut i = 0usize;

    while i < html.len() {
        if html.as_bytes()[i] == b'<' {
            let Some(end) = html[i..].find('>') else { break };
            let inner = &html[i + 1..i + end];
            let closing = inner.starts_with('/');
            let name = inner
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();

            match skipping {
                Some(skip) if closing && name == skip => skipping = None,
                Some(_) => {}
                None => {
                    if !closing && (name == "script" || name == "style") {
                        skipping = Some(if name == "script" { "script" } else { "style" });
                    } else if is_block_tag(&name) {
                        if !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                }
            }
            i += end + 1;
            continue;
        }

        match skipping {
            Some(_) => i += 1,
            None => {
                let ch = html[i..].chars().next().expect("char boundary");
                // Source-level line wrapping is whitespace, not a line break —
                // only block tags introduce one. ponytail: this also flattens
                // <pre> indentation; upgrade to a pre-preserving mode if code
                // samples ever matter.
                match ch {
                    '\n' | '\r' | '\t' => out.push(' '),
                    _ => out.push(ch),
                }
                i += ch.len_utf8();
            }
        }
    }

    tidy(&decode_entities(&out))
}

/// Adds `https://` when the user or the model omits a scheme.
pub fn normalise_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        #[cfg(windows)]
        {
            let with_exe = dir.join(format!("{name}.exe"));
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
    }
    None
}

/// Explicit setting first, then the usual install locations, then PATH.
pub fn find_browser(explicit: &str) -> Option<PathBuf> {
    let explicit = explicit.trim();
    if !explicit.is_empty() {
        let path = PathBuf::from(explicit);
        return path.is_file().then_some(path);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        let roots = [
            std::env::var("ProgramFiles(x86)").ok(),
            std::env::var("ProgramFiles").ok(),
            std::env::var("LOCALAPPDATA").ok(),
        ];
        for root in roots.into_iter().flatten() {
            for relative in [
                r"Microsoft\Edge\Application\msedge.exe",
                r"Google\Chrome\Application\chrome.exe",
                r"Chromium\Application\chrome.exe",
            ] {
                candidates.push(PathBuf::from(&root).join(relative));
            }
        }
    } else if cfg!(target_os = "macos") {
        for path in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ] {
            candidates.push(PathBuf::from(path));
        }
    } else {
        for name in ["google-chrome", "chromium", "chromium-browser", "microsoft-edge"] {
            if let Some(found) = which(name) {
                candidates.push(found);
            }
        }
    }

    candidates.into_iter().find(|candidate| candidate.is_file())
}

pub async fn fetch_url(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let url = normalise_url(url);
    let response = http
        .get(&url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("could not fetch {url}: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("{url} returned {status}"));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let slice = &bytes[..bytes.len().min(MAX_FETCH_BYTES)];
    let body = String::from_utf8_lossy(slice);

    let text = if content_type.contains("html") || body.trim_start().starts_with('<') {
        html_to_text(&body)
    } else {
        tidy(&body)
    };

    if text.is_empty() {
        return Ok(format!("{url} returned no readable text"));
    }
    Ok(truncate(format!("{url}\n\n{text}")))
}

/// Renders the page with the machine's Chromium-family browser, so
/// JavaScript-built content is present.
pub async fn browser_read(explicit_path: &str, url: &str) -> Result<String, String> {
    let browser = find_browser(explicit_path).ok_or_else(|| {
        "no Chromium-family browser found — install Chrome/Edge or set the path in \
         Settings → Browsing"
            .to_string()
    })?;
    let url = normalise_url(url);

    let mut cmd = Command::new(&browser);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--disable-extensions")
        .arg("--mute-audio")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg(format!("--virtual-time-budget={VIRTUAL_TIME_BUDGET_MS}"))
        .arg("--dump-dom")
        .arg(&url)
        .stdin(Stdio::null());

    let raw = run_process(cmd, None, BROWSER_TIMEOUT_SECS, false).await?;
    // A launch that produced no HTML at all is a failure, not an empty page —
    // say so rather than handing the model a blank result.
    if !raw.contains('<') {
        return Err(format!(
            "the browser launched but rendered nothing for {url} — check the browser path in \
             Settings → Browsing"
        ));
    }
    let text = html_to_text(&raw);

    if text.trim().is_empty() {
        return Ok(format!(
            "{url} produced no readable text after rendering (the page may need a login)"
        ));
    }
    Ok(truncate(format!("{url}\n\n{text}")))
}

#[derive(serde::Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchResult>,
}

#[derive(serde::Deserialize)]
struct SearchResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

pub async fn web_search(
    http: &reqwest::Client,
    searxng_url: &str,
    query: &str,
    limit: usize,
) -> Result<String, String> {
    let base = searxng_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(
            "web search needs a SearXNG instance URL — set it in Settings → Browsing".to_string(),
        );
    }

    let mut target = reqwest::Url::parse(&format!("{base}/search"))
        .map_err(|e| format!("'{base}' is not a usable SearXNG URL: {e}"))?;
    target.query_pairs_mut().append_pair("q", query).append_pair("format", "json");

    let response = http
        .get(target)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("could not reach SearXNG at {base}: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("SearXNG returned {}", response.status()));
    }

    let body = response.text().await.map_err(|e| e.to_string())?;
    let parsed: SearchResponse = serde_json::from_str(&body).map_err(|_| {
        "SearXNG did not return JSON — add `json` to the `formats` list in its settings.yml"
            .to_string()
    })?;

    if parsed.results.is_empty() {
        return Ok(format!("no results for '{query}'"));
    }

    let mut out = format!("Results for '{query}':\n\n");
    for (index, result) in parsed.results.iter().take(limit).enumerate() {
        out.push_str(&format!("{}. {}\n   {}\n", index + 1, result.title, result.url));
        let snippet = result.content.trim();
        if !snippet.is_empty() {
            out.push_str(&format!("   {snippet}\n"));
        }
    }
    Ok(truncate(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_and_style_bodies_are_dropped() {
        let html = "<p>keep me</p><script>var x = '<p>nope</p>';</script><style>p{color:red}</style><p>and me</p>";
        let text = html_to_text(html);
        assert!(text.contains("keep me"));
        assert!(text.contains("and me"));
        assert!(!text.contains("var x"));
        assert!(!text.contains("color:red"));
    }

    #[test]
    fn block_tags_become_line_breaks() {
        let text = html_to_text("<div>one</div><div>two</div>");
        assert_eq!(text, "one\ntwo");
    }

    #[test]
    fn entities_are_decoded_without_double_decoding() {
        assert_eq!(html_to_text("<p>a &amp; b</p>"), "a & b");
        assert_eq!(html_to_text("<p>&amp;lt;</p>"), "&lt;");
        assert_eq!(html_to_text("<p>caf&eacute;</p>"), "caf&eacute;");
    }

    #[test]
    fn source_line_wrapping_does_not_become_a_line_break() {
        assert_eq!(html_to_text("<p>one\n     two</p>"), "one two");
    }

    #[test]
    fn whitespace_is_collapsed_and_empty_blocks_add_nothing() {
        // Consecutive block tags share one break on purpose: a real DOM nests
        // divs constantly and blank lines either side of every one would leave
        // the extracted text full of holes.
        let text = html_to_text("<p>  lots   of\n\n space </p><p></p><p>next</p>");
        assert_eq!(text, "lots of space\nnext");
    }

    #[test]
    fn urls_gain_a_scheme_when_missing() {
        assert_eq!(normalise_url("example.com/a"), "https://example.com/a");
        assert_eq!(normalise_url("http://x.test"), "http://x.test");
        assert_eq!(normalise_url("  example.com  "), "https://example.com");
    }

    #[test]
    fn an_explicit_browser_path_is_used_when_it_exists() {
        let existing = std::env::current_exe().unwrap();
        let found = find_browser(existing.to_string_lossy().as_ref());
        assert_eq!(found.as_deref(), Some(existing.as_path()));
    }

    #[test]
    fn a_bogus_explicit_path_falls_through_to_auto_detection() {
        // Must not panic; the result depends on the machine.
        let _ = find_browser("Z:\\definitely\\not\\here.exe");
    }
}
