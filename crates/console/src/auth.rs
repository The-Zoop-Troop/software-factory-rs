//! Bearer tokens → principals. Tokens are stored hashed; comparison is constant-time.

use domain::Principal;
use sha2::{Digest as _, Sha256};

use crate::config::TokenSpec;

/// Hex sha256 of a token, as the token file stores it.
pub(crate) fn hash(token: &str) -> String {
    use core::fmt::Write as _;
    Sha256::digest(token.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            // Writing to a String cannot fail.
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn eq_constant_time(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

#[derive(Debug)]
pub(crate) struct TokenAuth {
    tokens: Vec<TokenSpec>,
}

impl TokenAuth {
    pub(crate) fn new(tokens: Vec<TokenSpec>) -> Self {
        Self { tokens }
    }
}

impl app::Authenticator for TokenAuth {
    fn authenticate(&self, bearer: &str) -> Option<Principal> {
        let digest = hash(bearer);
        self.tokens
            .iter()
            .find(|t| eq_constant_time(&t.sha256, &digest))
            .map(|t| Principal {
                client: t.client.clone(),
                grants: t.grants.clone(),
            })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use app::Authenticator as _;
    use domain::ClientId;

    use super::*;

    #[test]
    fn hashes_match_and_unknown_tokens_are_refused() {
        let auth = TokenAuth::new(vec![TokenSpec {
            client: ClientId::try_new("phone").expect("c"),
            sha256: hash("s3cret"),
            grants: BTreeMap::new(),
        }]);
        assert_eq!(
            auth.authenticate("s3cret").map(|p| p.client.to_string()),
            Some("phone".to_owned())
        );
        assert!(auth.authenticate("s3cre").is_none());
        assert!(auth.authenticate("").is_none());
        assert_eq!(hash("").len(), 64);
        assert!(!eq_constant_time("ab", "abc"));
    }
}
