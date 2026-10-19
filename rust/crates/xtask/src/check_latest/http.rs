//! check-latest's reads from GitHub. (The shell tool used `gh api`; the
//! Rust one speaks to the same REST API directly, so nothing has to be
//! installed.)
//!
//! Every address starts from a base the environment can move, so a test
//! points the tool at a fake GitHub on 127.0.0.1:
//!
//! - `METHOD_READER_GITHUB_API`, default `https://api.github.com`;
//! - `METHOD_READER_GITHUB_WEB`, default `https://github.com` (where the
//!   updater's `releases/latest/download/latest.json` redirect lives).
//!
//! `GH_TOKEN` (else `GITHUB_TOKEN`) is sent to the API when set, as `gh`
//! did, for the rate limit; it is never sent anywhere else.
//!
//! TLS is ureq's rustls with the ring provider, the stack method-update and
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
