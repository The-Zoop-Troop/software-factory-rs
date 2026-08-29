//! MCP server configuration a project ships in `.factory/mcp.json`, in the de-facto
//! `{"mcpServers": {name: {command, args, env} | {url}}}` shape. Parsed once here; each harness
//! adapter renders it into its own format.

use std::collections::BTreeMap;
use std::path::Path;

/// One MCP server: a local process or a remote endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum McpServer {
    Local {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    Remote {
        url: String,
        headers: BTreeMap<String, String>,
    },
}

/// Named servers, in a stable order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McpConfig {
    pub servers: BTreeMap<String, McpServer>,
}

/// Why `.factory/mcp.json` was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum McpConfigError {
    #[error("mcp.json is not valid JSON: {detail}")]
    Json { detail: String },
    #[error("server `{name}` needs either `command` or `url`")]
    Shape { name: String },
}

#[derive(serde::Deserialize)]
struct RawFile {
    #[serde(default, rename = "mcpServers")]
    servers: BTreeMap<String, RawServer>,
}

#[derive(serde::Deserialize)]
struct RawServer {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

impl McpConfig {
    /// Parse the file's text.
    ///
    /// # Errors
    /// `Json` for malformed input, `Shape` for a server with neither `command` nor `url`.
    pub fn parse(text: &str) -> Result<Self, McpConfigError> {
        let raw: RawFile = serde_json::from_str(text).map_err(|e| McpConfigError::Json {
            detail: e.to_string(),
        })?;
        let mut servers = BTreeMap::new();
        for (name, s) in raw.servers {
            let server = match (s.command, s.url) {
                (Some(command), _) => McpServer::Local {
                    command,
                    args: s.args,
                    env: s.env,
                },
                (None, Some(url)) => McpServer::Remote {
                    url,
                    headers: s.headers,
                },
                (None, None) => return Err(McpConfigError::Shape { name }),
            };
            servers.insert(name, server);
        }
        Ok(Self { servers })
    }

    /// `.factory/mcp.json` under `repo`, or an empty config when absent.
    ///
    /// # Errors
    /// As `parse`; an unreadable-but-present file is `Json`.
    pub fn load(repo: &Path) -> Result<Self, McpConfigError> {
        let path = repo.join(".factory/mcp.json");
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            // fp-allow: reading the project's own config file is the boundary; translated to McpConfigError here
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(McpConfigError::Json {
                detail: e.to_string(),
            }),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }

    /// Hosts a remote server will contact (for the egress allowlist and `doctor`).
    #[must_use]
    pub fn remote_hosts(&self) -> Vec<String> {
        self.servers
            .values()
            .filter_map(|s| match s {
                McpServer::Remote { url, .. } => url
                    .split("://")
                    .nth(1)
                    .and_then(|r| r.split('/').next())
                    .map(str::to_owned),
                McpServer::Local { .. } => None,
            })
            .collect()
    }
}

impl McpConfig {
    /// The `{"mcpServers": …}` JSON Claude Code accepts via `--mcp-config`.
    #[must_use]
    pub fn to_claude_json(&self) -> serde_json::Value {
        let servers: serde_json::Map<String, serde_json::Value> = self
            .servers
            .iter()
            .map(|(name, s)| {
                let v = match s {
                    McpServer::Local { command, args, env } => {
                        serde_json::json!({ "command": command, "args": args, "env": env })
                    }
                    McpServer::Remote { url, headers } => {
                        serde_json::json!({ "type": "http", "url": url, "headers": headers })
                    }
                };
                (name.clone(), v)
            })
            .collect();
        serde_json::json!({ "mcpServers": servers })
    }

    /// `OpenCode`'s `mcp` config block.
    #[must_use]
    pub fn to_opencode_json(&self) -> serde_json::Value {
        let servers: serde_json::Map<String, serde_json::Value> = self
            .servers
            .iter()
            .map(|(name, s)| {
                let v = match s {
                    McpServer::Local { command, args, env } => {
                        let mut cmd = vec![command.clone()];
                        cmd.extend(args.iter().cloned());
                        serde_json::json!({ "type": "local", "command": cmd, "environment": env })
                    }
                    McpServer::Remote { url, headers } => {
                        serde_json::json!({ "type": "remote", "url": url, "headers": headers })
                    }
                };
                (name.clone(), v)
            })
            .collect();
        serde_json::json!({ "mcp": servers })
    }

    /// Codex `-c mcp_servers.<name>.<key>=<toml value>` overrides.
    #[must_use]
    pub fn to_codex_overrides(&self) -> Vec<String> {
        let q = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
        let mut out = Vec::new();
        for (name, s) in &self.servers {
            match s {
                McpServer::Local { command, args, env } => {
                    out.push(format!("mcp_servers.{name}.command={}", q(command)));
                    out.push(format!(
                        "mcp_servers.{name}.args=[{}]",
                        args.iter().map(|a| q(a)).collect::<Vec<_>>().join(",")
                    ));
                    if !env.is_empty() {
                        out.push(format!(
                            "mcp_servers.{name}.env={{{}}}",
                            env.iter()
                                .map(|(k, v)| format!("{}={}", q(k), q(v)))
                                .collect::<Vec<_>>()
                                .join(",")
                        ));
                    }
                }
                McpServer::Remote { url, .. } => {
                    out.push(format!("mcp_servers.{name}.url={}", q(url)));
                }
            }
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_and_remote() {
        let c = McpConfig::parse(r#"{"mcpServers":{"fs":{"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","."],"env":{"A":"1"}},"docs":{"url":"https://mcp.example.com/sse","headers":{"Authorization":"Bearer x"}}}}"#).unwrap();
        assert_eq!(c.servers.len(), 2);
        assert!(
            matches!(&c.servers["fs"], McpServer::Local { command, args, .. } if command == "npx" && args.len() == 3)
        );
        assert_eq!(c.remote_hosts(), vec!["mcp.example.com"]);
        assert!(matches!(
            McpConfig::parse(r#"{"mcpServers":{"bad":{}}}"#),
            Err(McpConfigError::Shape { .. })
        ));
        assert!(matches!(
            McpConfig::parse("nope"),
            Err(McpConfigError::Json { .. })
        ));
        assert!(McpConfig::parse("{}").unwrap().is_empty());
        assert!(McpConfig::load(&std::env::temp_dir()).unwrap().is_empty());
        let claude = c.to_claude_json();
        assert_eq!(claude["mcpServers"]["fs"]["command"], "npx");
        assert_eq!(claude["mcpServers"]["docs"]["type"], "http");
        let oc = c.to_opencode_json();
        assert_eq!(oc["mcp"]["fs"]["command"][0], "npx");
        assert_eq!(oc["mcp"]["docs"]["type"], "remote");
        let cx = c.to_codex_overrides();
        assert!(cx.iter().any(|o| o == "mcp_servers.fs.command=\"npx\""));
        assert!(cx.iter().any(|o| o.starts_with("mcp_servers.fs.args=[")));
        assert!(
            cx.iter()
                .any(|o| o == "mcp_servers.docs.url=\"https://mcp.example.com/sse\"")
        );
    }
}
