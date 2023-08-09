/// This implements the Username token profile described in ONVIF Core Spec 5.9.4
/// which is based on [WS-UsernameToken]: https://docs.oasis-open.org/wss/v1.1/wss-v1.1-spec-pr-UsernameTokenProfile-01.htm
#[derive(Default, Debug, Clone)]
pub struct UsernameToken {
    pub username: String,
    pub nonce: String,
    pub digest: String,
    pub created: String,
}

impl UsernameToken {
    pub fn new(username: &str, password: &str) -> UsernameToken {
        let nonce = uuid::Uuid::new_v4().to_string();
        let created = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        Self::generate_token(username, password, &nonce, &created)
    }

    fn generate_token(username: &str, password: &str, nonce: &str, created: &str) -> UsernameToken {
        let concat = format!("{}{}{}", nonce, created, password);
        let digest = {
            let mut hasher = sha1::Sha1::new();
            hasher.update(concat.as_bytes());
            hasher.digest().bytes()
        };

        UsernameToken {
            username: username.to_string(),
            nonce: base64::encode(nonce),
            digest: base64::encode(digest),
            created: created.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token() {
        let username = "abcdefe";
        let password = "1234567";
        let nonce = "nonce";
        let created = "2000-01-01T12:34:56:789Z";

        let username_token = UsernameToken::generate_token(username, password, nonce, created);

        assert_eq!(username_token.username, username);
        assert_eq!(username_token.created, created);
        assert_eq!(username_token.nonce, "bm9uY2U=");
        assert_eq!(username_token.digest, "AGsoQQ+qNJu6Ha7h/QAPoQvYcV0=");
    }
}
