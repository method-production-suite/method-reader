//! The release tools' reads from GitHub: bump-tap, check-tap and
//! check-latest. (The shell tools used `gh api`; the Rust ones speak to
//! the same REST API directly, so nothing has to be installed.)
//!
//! Every address starts from a base the environment can move, so a test
//! points the tools at a fake GitHub on 127.0.0.1:
//!
//! - `METHOD_READER_GITHUB_API`, default `https://api.github.com`;
//! - `METHOD_READER_GITHUB_WEB`, default `https://github.com` (where the
//!   updater's `releases/latest/download/latest.json` redirect lives).
//!
//! `GH_TOKEN` (else `GITHUB_TOKEN`) is sent to the API when set, as `gh`
//! did, for the rate limit; it is never sent anywhere else.
//!
//! TLS is ureq's rustls with the ring provider, the stack sp-update and
//! sp-release use (one TLS stack for the port; rust/PARITY.md, Decisions).

/// What one GET answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub status: u16,
    pub text: String,
}

/// The network, injected: a GET that follows redirects. An Err is a
/// failure to get any answer at all (no route, TLS, a dropped connection).
pub type Fetcher<'a> = dyn FnMut(&str) -> Result<Fetched, String> + 'a;

/// The two bases every address starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bases {
    pub api: String,
    pub web: String,
}

impl Default for Bases {
    fn default() -> Bases {
        Bases {
            api: "https://api.github.com".to_string(),
            web: "https://github.com".to_string(),
        }
    }
}

impl Bases {
    /// The defaults, moved by `METHOD_READER_GITHUB_API` / `METHOD_READER_GITHUB_WEB`.
    pub fn from_env() -> Bases {
        let d = Bases::default();
        let pick = |name: &str, default: String| {
            std::env::var(name)
                .ok()
                .filter(|v| !v.is_empty())
                .map(|v| v.trim_end_matches('/').to_string())
                .unwrap_or(default)
        };
        Bases {
            api: pick("METHOD_READER_GITHUB_API", d.api),
            web: pick("METHOD_READER_GITHUB_WEB", d.web),
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .user_agent("method-reader-xtask")
        .build()
        .new_agent()
}

/// A real GET. API calls (`api` prefix) carry the GitHub Accept header and
/// the token when there is one.
pub fn fetch(url: &str, api: &str) -> Result<Fetched, String> {
    let mut req = agent().get(url);
    if url.starts_with(api) {
        req = req
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        let token = std::env::var("GH_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
            .or_else(|| std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()));
        if let Some(t) = token {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
    }
    let mut res = req.call().map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let text = res.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok(Fetched { status, text })
}

/// [`fetch`] against `bases`, as a [`Fetcher`].
pub fn real_fetcher(bases: &Bases) -> impl FnMut(&str) -> Result<Fetched, String> + '_ {
    move |url| fetch(url, &bases.api)
}

/// Decode base64 the way the contents API and Node's Buffer accept it:
/// whitespace ignored, padding optional, stray trailing bits tolerated.
pub fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    const LENIENT: GeneralPurpose = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    let mut clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=')
        .collect();
    // A lone trailing character holds no whole byte; Node drops it.
    if clean.len() % 4 == 1 {
        clean.pop();
    }
    LENIENT.decode(clean).map_err(|e| e.to_string())
}

/// One release's assets: name and the `sha256:<hex>` digest GitHub
/// records for each (empty when it records none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAssets {
    pub tag: String,
    pub assets: Vec<(String, String)>,
}

impl ReleaseAssets {
    /// The hex digest of one asset, `sha256:` stripped; "" when absent.
    pub fn digest(&self, name: &str) -> String {
        self.assets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d.strip_prefix("sha256:").unwrap_or(d).to_string())
            .unwrap_or_default()
    }

    pub fn has(&self, name: &str) -> bool {
        self.assets.iter().any(|(n, _)| n == name)
    }
}

/// `GET /repos/{repo}/releases/tags/{tag}`, its asset list; Err on any
/// answer that is not a release (GitHub's 404 body is not a digest).
pub fn release_assets(
    fetcher: &mut Fetcher<'_>,
    bases: &Bases,
    repo: &str,
    tag: &str,
) -> Result<ReleaseAssets, String> {
    let url = format!("{}/repos/{repo}/releases/tags/{tag}", bases.api);
    let got = fetcher(&url)?;
    if got.status != 200 {
        return Err(format!("{url} answered HTTP {}", got.status));
    }
    let v: serde_json::Value =
        serde_json::from_str(&got.text).map_err(|e| format!("{url} did not answer JSON: {e}"))?;
    let Some(list) = v.get("assets").and_then(|a| a.as_array()) else {
        return Err(format!("{url} did not describe a release"));
    };
    let assets = list
        .iter()
        .map(|a| {
            let s = |k: &str| {
                a.get(k)
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            (s("name"), s("digest"))
        })
        .collect();
    Ok(ReleaseAssets {
        tag: v
            .get("tag_name")
            .and_then(|t| t.as_str())
            .unwrap_or(tag)
            .to_string(),
        assets,
    })
}

/// `GET /repos/{repo}/releases/latest`: the newest published,
/// non-prerelease, non-draft release's tag (what `gh release list
/// --exclude-drafts --exclude-pre-releases --limit 1` answered). Ok(None)
/// when there is none.
pub fn newest_release_tag(
    fetcher: &mut Fetcher<'_>,
    bases: &Bases,
    repo: &str,
) -> Result<Option<String>, String> {
    let url = format!("{}/repos/{repo}/releases/latest", bases.api);
    let got = fetcher(&url)?;
    if got.status == 404 {
        return Ok(None);
    }
    if got.status != 200 {
        return Err(format!("{url} answered HTTP {}", got.status));
    }
    let v: serde_json::Value =
        serde_json::from_str(&got.text).map_err(|e| format!("{url} did not answer JSON: {e}"))?;
    Ok(v.get("tag_name")
        .and_then(|t| t.as_str())
        .map(str::to_string))
}

/// `GET /repos/{repo}/contents/{path}`: one file of a repository's default
/// branch, decoded.
pub fn repo_file(
    fetcher: &mut Fetcher<'_>,
    bases: &Bases,
    repo: &str,
    path: &str,
) -> Result<String, String> {
    let url = format!("{}/repos/{repo}/contents/{path}", bases.api);
    let got = fetcher(&url)?;
    if got.status != 200 {
        return Err(format!("{url} answered HTTP {}", got.status));
    }
    let v: serde_json::Value =
        serde_json::from_str(&got.text).map_err(|e| format!("{url} did not answer JSON: {e}"))?;
    let content = v
        .get("content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| format!("{url} carried no content"))?;
    let bytes = decode_base64(content)?;
    String::from_utf8(bytes).map_err(|e| format!("{path} is not UTF-8: {e}"))
}
