use super::{normalize_scopes, AuthError, GhCli};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhDetectedToken {
    pub host: String,
    pub login: String,
    pub scopes: Vec<String>,
    pub token: String,
}

#[derive(Debug, Default)]
struct GhStatusSection {
    host: Option<String>,
    login: Option<String>,
    scopes: Vec<String>,
    token: Option<String>,
}

pub fn detect_gh_scopes(gh_cli: &dyn GhCli) -> Result<GhDetectedToken, AuthError> {
    let token_output = gh_cli.run(&["auth", "token"])?;
    if !token_output.success() {
        return Err(AuthError::GhNotLoggedIn);
    }
    let token = token_output.stdout.trim();
    if token.is_empty() {
        return Err(AuthError::GhParseFailed);
    }

    let status_output = gh_cli.run(&["auth", "status", "--show-token"])?;
    if !status_output.success() {
        return Err(AuthError::GhNotLoggedIn);
    }

    let status_blob = format!("{}\n{}", status_output.stdout, status_output.stderr);
    let section = parse_status(&status_blob, token).ok_or(AuthError::GhParseFailed)?;
    Ok(GhDetectedToken {
        host: section.host.ok_or(AuthError::GhParseFailed)?,
        login: section.login.ok_or(AuthError::GhParseFailed)?,
        scopes: section.scopes,
        token: token.to_string(),
    })
}

fn parse_status(output: &str, target_token: &str) -> Option<GhStatusSection> {
    let mut sections = Vec::<GhStatusSection>::new();
    let mut current = GhStatusSection::default();
    for raw_line in output.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(stripped) = line.strip_prefix('-') {
            line = stripped.trim();
        }
        if let Some((host, login)) = parse_logged_in_line(line) {
            if current.host.is_some() || current.login.is_some() || current.token.is_some() {
                sections.push(current);
                current = GhStatusSection::default();
            }
            current.host = Some(host.to_string());
            current.login = Some(login.to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Token scopes:") {
            current.scopes = normalize_scopes(rest);
            continue;
        }
        if let Some(rest) = line.strip_prefix("Token:") {
            let token = unquote(rest.trim());
            if !token.is_empty() {
                current.token = Some(token.to_string());
            }
            continue;
        }
    }
    if current.host.is_some() || current.login.is_some() || current.token.is_some() {
        sections.push(current);
    }

    sections
        .into_iter()
        .find(|section| section.token.as_deref() == Some(target_token))
}

fn parse_logged_in_line(line: &str) -> Option<(&str, &str)> {
    let logged_in_fragment = if let Some(index) = line.find("Logged in to ") {
        &line[index..]
    } else {
        line
    };
    if let Some(prefix) = logged_in_fragment.strip_prefix("Logged in to ") {
        if let Some((host, tail)) = prefix.split_once(" as ") {
            return Some((host.trim(), trim_login_tail(tail)));
        }
        if let Some((host, tail)) = prefix.split_once(" account ") {
            return Some((host.trim(), trim_login_tail(tail)));
        }
    }
    None
}

fn trim_login_tail(raw: &str) -> &str {
    raw.trim()
        .trim_end_matches(')')
        .split_whitespace()
        .next()
        .unwrap_or_default()
}

fn unquote(raw: &str) -> &str {
    raw.trim_matches('\'').trim_matches('"')
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn parses_status_sections_by_token_match() {
        let status = r#"
github.com
  ✓ Logged in to github.com as mona (keyring)
  - Token: ghp_first
  - Token scopes: 'repo', 'read:org'
enterprise.internal
  ✓ Logged in to enterprise.internal account octocat (oauth_token)
  - Token: gho_second
  - Token scopes: repo,admin:repo_hook
"#;
        let parsed = parse_status(status, "gho_second").expect("should parse");
        assert_eq!(parsed.host.as_deref(), Some("enterprise.internal"));
        assert_eq!(parsed.login.as_deref(), Some("octocat"));
        assert_eq!(parsed.scopes, vec!["admin:repo_hook", "repo"]);
    }
}
