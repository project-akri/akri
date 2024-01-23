use akri_discovery_utils::discovery::v0::ByteData;
use std::collections::HashMap;

/// Key name of device credential list in discoveryProperties
pub const DEVICE_CREDENTIAL_LIST: &str = "device_credential_list";
/// Key name of device credential ref list in discoveryProperties
pub const DEVICE_CREDENTIAL_REF_LIST: &str = "device_credential_ref_list";
/// Key name prefix of username credential list in discoveryProperties
pub const DEVICE_CREDENTIAL_USERNAME_PREFIX: &str = "username_";
/// Key name prefix of password credential list in discoveryProperties
pub const DEVICE_CREDENTIAL_PASSWORD_PREFIX: &str = "password_";
/// Key name of default username
pub const DEVICE_CREDENTIAL_DEFAULT_USERNAME: &str = "username_default";
/// Key name of default password
pub const DEVICE_CREDENTIAL_DEFAULT_PASSWORD: &str = "password_default";
/// Name of default credential for querying CredentialStore
pub const DEFAULT_CREDENTIAL_ID: &str = "default";

#[derive(Serialize, Deserialize, Clone, Debug)]
struct CredentialData {
    username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(default)]
    base64encoded: bool,
}

impl CredentialData {
    fn get_username(&self) -> String {
        self.username.clone()
    }

    fn get_password(&self) -> Option<String> {
        if self.base64encoded {
            self.password
                .as_ref()
                .and_then(|encoded_data| decode_base64_str(encoded_data))
        } else {
            self.password.clone()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct CredentialRefData {
    username_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password_ref: Option<String>,
}

#[derive(Default)]
pub struct CredentialStore {
    credentials: HashMap<String, (String, Option<String>)>,
}

impl CredentialStore {
    pub fn new(credential_data: &HashMap<String, ByteData>) -> Self {
        let mut store = Self::default();
        store.process_credential_list(credential_data);
        store.process_credential_ref_list(credential_data);
        store.process_username_password(credential_data);
        store.process_default_username_password(credential_data);
        store
    }

    pub fn get(&self, uuid: &str) -> Option<(String, Option<String>)> {
        self.credentials
            .get(uuid)
            .or_else(|| self.credentials.get(DEFAULT_CREDENTIAL_ID))
            .map(|(n, p)| (n.to_string(), p.as_ref().map(|p| p.to_string())))
    }

    fn process_credential_list(&mut self, credential_data: &HashMap<String, ByteData>) {
        let result = self.process_list_data(
            DEVICE_CREDENTIAL_LIST,
            credential_data,
            |list_content, _credential_data| parse_credential_list(list_content),
        );
        self.credentials.extend(result);
    }

    fn process_credential_ref_list(&mut self, credential_data: &HashMap<String, ByteData>) {
        let result = self.process_list_data(
            DEVICE_CREDENTIAL_REF_LIST,
            credential_data,
            parse_credential_ref_list,
        );
        self.credentials.extend(result);
    }

    fn process_list_data<F>(
        &mut self,
        key: &str,
        credential_data: &HashMap<String, ByteData>,
        parse: F,
    ) -> HashMap<String, (String, Option<String>)>
    where
        F: Fn(&str, &HashMap<String, ByteData>) -> HashMap<String, (String, Option<String>)>,
    {
        parse_list_data(key, credential_data)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|list| credential_data.get(&list).and_then(byte_data_to_str))
            .flat_map(|list_content| parse(list_content, credential_data))
            .collect()
    }

    fn process_username_password(&mut self, credential_data: &HashMap<String, ByteData>) {
        let username_list = credential_data
            .iter()
            .filter_map(|(key, value)| {
                key.strip_prefix(DEVICE_CREDENTIAL_USERNAME_PREFIX)
                    .and_then(|device_uuid| {
                        let username = byte_data_to_str(value)?;
                        if !device_uuid.is_empty() {
                            Some((device_uuid, username))
                        } else {
                            None
                        }
                    })
            })
            .collect::<HashMap<&str, &str>>();
        if username_list.is_empty() {
            return;
        }

        let mut password_list = credential_data
            .iter()
            .filter_map(|(key, value)| {
                key.strip_prefix(DEVICE_CREDENTIAL_PASSWORD_PREFIX)
                    .and_then(|device_uuid| {
                        let password = byte_data_to_str(value);
                        if !device_uuid.is_empty() {
                            Some((device_uuid, password))
                        } else {
                            None
                        }
                    })
            })
            .collect::<HashMap<&str, Option<&str>>>();

        let result = username_list
            .into_iter()
            .map(|(device_uuid, username)| {
                let password = password_list
                    .remove(&device_uuid)
                    .and_then(|opt_s| opt_s.map(|s| s.to_string()));
                // Credential data key is in C_IDENTIFIER format
                // convert it back to uuid string format by replacing "_" with "-"
                (
                    device_uuid.replace('_', "-"),
                    (username.to_string(), password),
                )
            })
            .collect::<HashMap<String, (String, Option<String>)>>();
        self.credentials.extend(result);
    }

    fn process_default_username_password(&mut self, credential_data: &HashMap<String, ByteData>) {
        let default_credential = credential_data
            .get(DEVICE_CREDENTIAL_DEFAULT_USERNAME)
            .and_then(byte_data_to_str)
            .map(|username| username.to_string())
            .map(|username| {
                let password = credential_data
                    .get(DEVICE_CREDENTIAL_DEFAULT_PASSWORD)
                    .and_then(byte_data_to_str)
                    .map(|password| password.to_string());
                (username, password)
            });
        if let Some(credential) = default_credential {
            self.credentials
                .insert(DEFAULT_CREDENTIAL_ID.to_string(), credential);
        }
    }
}

fn parse_credential_list(credential_list: &str) -> HashMap<String, (String, Option<String>)> {
    serde_json::from_str::<HashMap<String, CredentialData>>(credential_list)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, cred_data)| (id, (cred_data.get_username(), cred_data.get_password())))
        .collect()
}

fn parse_credential_ref_list(
    credential_ref_list: &str,
    credential_data: &HashMap<String, ByteData>,
) -> HashMap<String, (String, Option<String>)> {
    serde_json::from_str::<HashMap<String, CredentialRefData>>(credential_ref_list)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(id, cred_ref)| {
            let username = credential_data
                .get(&cred_ref.username_ref)
                .and_then(byte_data_to_str)
                .map(|n| n.to_string())?;
            if username.is_empty() {
                return None;
            }

            let password = cred_ref
                .password_ref
                .map(|pwd| {
                    credential_data
                        .get(&pwd)
                        .and_then(byte_data_to_str)
                        .map(|p| p.to_string())
                })
                .unwrap_or_default();
            Some((id, (username, password)))
        })
        .collect()
}

fn parse_list_data(key: &str, credential_data: &HashMap<String, ByteData>) -> Option<Vec<String>> {
    credential_data
        .get(key)
        .and_then(byte_data_to_str)
        .and_then(|list_json_str| serde_json::from_str::<Vec<String>>(list_json_str).ok())
}

fn byte_data_to_str(byte_data: &ByteData) -> Option<&str> {
    byte_data
        .vec
        .as_ref()
        .and_then(|s| std::str::from_utf8(s).ok())
}

fn decode_base64_str(encoded_data: &str) -> Option<String> {
    let decoded_data = base64::decode(encoded_data).ok()?;
    std::str::from_utf8(&decoded_data)
        .map(|s| s.to_string())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct DeviceCredentialData<'a> {
        pub id: String,
        pub username: Option<&'a [u8]>,
        pub password: Option<&'a [u8]>,
    }

    fn generate_list_credential_data(
        list_name: &str,
        entries: Vec<(&str, Option<&[u8]>)>,
    ) -> HashMap<String, ByteData> {
        let credential_list_value = format!(
            "[{}]",
            entries
                .iter()
                .map(|(k, _v)| format!(r#""{}""#, k))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let list_data = HashMap::from([generate_credential_data_entry(
            list_name,
            Some(credential_list_value.as_bytes()),
        )]);

        entries
            .into_iter()
            .map(|(k, v)| generate_credential_data_entry(k, v))
            .chain(list_data)
            .collect::<HashMap<String, ByteData>>()
    }

    fn generate_username_password_credential_data(
        entries: Vec<DeviceCredentialData>,
    ) -> HashMap<String, ByteData> {
        entries
            .into_iter()
            .flat_map(|entry| {
                let username_key = format!("{}{}", DEVICE_CREDENTIAL_USERNAME_PREFIX, entry.id);
                let password_key = format!("{}{}", DEVICE_CREDENTIAL_PASSWORD_PREFIX, entry.id);
                HashMap::from([
                    generate_credential_data_entry(&username_key, entry.username),
                    generate_credential_data_entry(&password_key, entry.password),
                ])
            })
            .collect::<HashMap<String, ByteData>>()
    }

    fn generate_credential_data_entry(key: &str, value: Option<&[u8]>) -> (String, ByteData) {
        (
            key.to_string(),
            ByteData {
                vec: value.map(|p| p.to_vec()),
            },
        )
    }

    #[test]
    fn test_credential_store_empty() {
        let _ = env_logger::builder().is_test(true).try_init();
        let credential_data = HashMap::new();

        let credential_store = CredentialStore::new(&credential_data);
        assert!(credential_store.credentials.is_empty());
    }

    #[test]
    fn test_credential_store_non_utf8_username() {
        let _ = env_logger::builder().is_test(true).try_init();
        let test_data = vec![("deviceid-1", vec![200u8, 200u8, 200u8], "password_1")];
        let test_entries = test_data
            .iter()
            .map(|(id, uname, pwd)| DeviceCredentialData {
                id: id.replace('-', "_"),
                username: Some(uname as &[u8]),
                password: Some(pwd.as_bytes()),
            })
            .collect::<Vec<_>>();
        let credential_data = generate_username_password_credential_data(test_entries);

        let credential_store = CredentialStore::new(&credential_data);
        assert!(credential_store.credentials.is_empty());
    }

    #[test]
    fn test_credential_store_non_utf8_password() {
        let _ = env_logger::builder().is_test(true).try_init();
        let test_data = vec![("deviceid-1", "username_1", vec![200u8, 200u8, 200u8])];
        let expected_result = test_data
            .iter()
            .map(|(id, uname, pwd)| {
                (
                    id.to_string(),
                    (
                        uname.to_string(),
                        std::str::from_utf8(pwd).map(|s| s.to_string()).ok(),
                    ),
                )
            })
            .collect::<HashMap<String, (String, Option<String>)>>();
        let test_entries = test_data
            .iter()
            .map(|(id, uname, pwd)| DeviceCredentialData {
                id: id.replace('-', "_"),
                username: Some(uname.as_bytes()),
                password: Some(pwd as &[u8]),
            })
            .collect::<Vec<_>>();
        let credential_data = generate_username_password_credential_data(test_entries);

        let credential_store = CredentialStore::new(&credential_data);
        assert_eq!(credential_store.credentials, expected_result);
    }

    fn build_default_username_password_data() -> HashMap<String, ByteData> {
        let secret_data = [("default", "default_username", "default_password")];
        let secret_test_data = secret_data
            .iter()
            .map(|(id, uname, pwd)| DeviceCredentialData {
                id: id.replace('-', "_"),
                username: Some(uname.as_bytes()),
                password: Some(pwd.as_bytes()),
            })
            .collect::<Vec<_>>();
        generate_username_password_credential_data(secret_test_data)
    }

    fn build_username_password_data() -> HashMap<String, ByteData> {
        let secret_data = [(
            "5f5a69c2-e0ae-504f-829b-00fcdab169cc",
            "username_5f",
            "password_5f",
        )];
        let secret_test_data = secret_data
            .iter()
            .map(|(id, uname, pwd)| DeviceCredentialData {
                id: id.replace('-', "_"),
                username: Some(uname.as_bytes()),
                password: Some(pwd.as_bytes()),
            })
            .collect::<Vec<_>>();
        generate_username_password_credential_data(secret_test_data)
    }

    fn build_device_credential_list_data() -> HashMap<String, ByteData> {
        let credential_list_key = "device_credential_list";
        let secret_list1_key = "secret_list1";
        let secret_list1_value = r#"
        {
            "5f5a69c2-e0ae-504f-829b-00fcdab169cc" :
                {
                    "username" : "uname_1",
                    "password" : "password_1"
                },
            "6a67158b-42b1-400b-8afe-1bec9a5d7909": 
                {
                    "username" : "uname_3",
                    "password" : "YWRtaW4=",
                    "base64encoded": true
                }
        }"#;

        let secret_list2_key = "secret_list2";
        let secret_list2_value = r#"
        {
            "7a21dc67-8438-5588-1547-4d1349048438" :
                {
                    "username" : "uname_2",
                    "password" : "password_2"
                }
        }
        "#;
        let secret_list_test_data = vec![
            (secret_list1_key, Some(secret_list1_value.as_bytes())),
            (secret_list2_key, Some(secret_list2_value.as_bytes())),
        ];
        generate_list_credential_data(credential_list_key, secret_list_test_data)
    }

    fn build_device_credential_ref_list_data() -> HashMap<String, ByteData> {
        let credential_ref_list_key = "device_credential_ref_list";
        let secret_ref_list1_key = "secret_ref_list1";
        let secret_ref_list1_value = r#"
        {
            "5f5a69c2-e0ae-504f-829b-00fcdab169cc" :
                {
                    "username_ref" : "device_1_username",
                    "password_ref" : "device_1_password"
                }
        }
        "#;

        let secret_ref_list2_key = "secret_ref_list2";
        let secret_ref_list2_value = r#"
        {
            "7a21dc67-8438-5588-1547-4d1349048438" :
                {
                    "username_ref" : "device_2_username",
                    "password_ref" : "device_2_password"
                }
        }
        "#;
        let secret_ref_username_password = [
            ("device_1_username", "user_foo"),
            ("device_1_password", "password_foo"),
            ("device_2_username", "user_bar"),
            ("device_2_password", "password_bar"),
        ];

        let secret_ref_list_test_data = vec![
            (
                secret_ref_list1_key,
                Some(secret_ref_list1_value.as_bytes()),
            ),
            (
                secret_ref_list2_key,
                Some(secret_ref_list2_value.as_bytes()),
            ),
        ];
        let credential_ref_list_data =
            generate_list_credential_data(credential_ref_list_key, secret_ref_list_test_data);
        secret_ref_username_password
            .iter()
            .map(|(k, v)| generate_credential_data_entry(k, Some(v.as_bytes())))
            .chain(credential_ref_list_data)
            .collect::<HashMap<String, ByteData>>()
    }

    #[test]
    fn test_credential_store_default_username_password() {
        let _ = env_logger::builder().is_test(true).try_init();
        let expected_result = (
            "default_username".to_string(),
            Some("default_password".to_string()),
        );
        let credential_data = build_default_username_password_data();

        let credential_store = CredentialStore::new(&credential_data);
        assert_eq!(credential_store.get("any_id"), Some(expected_result));
    }

    #[test]
    fn test_credential_store_username_password() {
        let _ = env_logger::builder().is_test(true).try_init();
        let expected_result = HashMap::from([(
            "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
            ("username_5f".to_string(), Some("password_5f".to_string())),
        )]);
        let credential_data = build_username_password_data();

        let credential_store = CredentialStore::new(&credential_data);
        assert_eq!(credential_store.credentials, expected_result);
    }

    #[test]
    fn test_credential_store_device_credential_list() {
        let _ = env_logger::builder().is_test(true).try_init();
        let expected_result = HashMap::from([
            (
                "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
                ("uname_1".to_string(), Some("password_1".to_string())),
            ),
            (
                "6a67158b-42b1-400b-8afe-1bec9a5d7909".to_string(),
                ("uname_3".to_string(), Some("admin".to_string())),
            ),
            (
                "7a21dc67-8438-5588-1547-4d1349048438".to_string(),
                ("uname_2".to_string(), Some("password_2".to_string())),
            ),
        ]);
        let credential_data = build_device_credential_list_data();

        let credential_store = CredentialStore::new(&credential_data);
        assert_eq!(credential_store.credentials, expected_result);
    }

    #[test]
    fn test_credential_store_device_credential_ref_list() {
        let _ = env_logger::builder().is_test(true).try_init();
        let expected_result = HashMap::from([
            (
                "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
                ("user_foo".to_string(), Some("password_foo".to_string())),
            ),
            (
                "7a21dc67-8438-5588-1547-4d1349048438".to_string(),
                ("user_bar".to_string(), Some("password_bar".to_string())),
            ),
        ]);
        let credential_data = build_device_credential_ref_list_data();
        let credential_store = CredentialStore::new(&credential_data);

        assert_eq!(credential_store.credentials, expected_result);
    }

    #[test]
    fn test_credential_store_device_credential_list_and_username_password() {
        let _ = env_logger::builder().is_test(true).try_init();
        let expected_result = HashMap::from([
            (
                "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
                ("username_5f".to_string(), Some("password_5f".to_string())),
            ),
            (
                "6a67158b-42b1-400b-8afe-1bec9a5d7909".to_string(),
                ("uname_3".to_string(), Some("admin".to_string())),
            ),
            (
                "7a21dc67-8438-5588-1547-4d1349048438".to_string(),
                ("uname_2".to_string(), Some("password_2".to_string())),
            ),
        ]);
        let credential_list_data = build_device_credential_list_data();
        let username_password_data = build_username_password_data();
        let credential_data = HashMap::new()
            .into_iter()
            .chain(credential_list_data)
            .chain(username_password_data)
            .collect();
        let credential_store = CredentialStore::new(&credential_data);

        assert_eq!(credential_store.credentials, expected_result);
    }

    #[test]
    fn test_credential_store_device_credential_list_and_credential_ref_list() {
        let _ = env_logger::builder().is_test(true).try_init();
        let expected_result = HashMap::from([
            (
                "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
                ("user_foo".to_string(), Some("password_foo".to_string())),
            ),
            (
                "6a67158b-42b1-400b-8afe-1bec9a5d7909".to_string(),
                ("uname_3".to_string(), Some("admin".to_string())),
            ),
            (
                "7a21dc67-8438-5588-1547-4d1349048438".to_string(),
                ("user_bar".to_string(), Some("password_bar".to_string())),
            ),
        ]);
        let credential_list_data = build_device_credential_list_data();
        let credential_ref_list_data = build_device_credential_ref_list_data();
        let credential_data = HashMap::new()
            .into_iter()
            .chain(credential_list_data)
            .chain(credential_ref_list_data)
            .collect();
        let credential_store = CredentialStore::new(&credential_data);

        assert_eq!(credential_store.credentials, expected_result);
    }

    #[test]
    fn test_credential_store_device_credential_ref_list_and_username_password() {
        let _ = env_logger::builder().is_test(true).try_init();
        let expected_result = HashMap::from([
            (
                "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
                ("username_5f".to_string(), Some("password_5f".to_string())),
            ),
            (
                "7a21dc67-8438-5588-1547-4d1349048438".to_string(),
                ("user_bar".to_string(), Some("password_bar".to_string())),
            ),
        ]);
        let credential_ref_list_data = build_device_credential_ref_list_data();
        let username_password_data = build_username_password_data();
        let credential_data = HashMap::new()
            .into_iter()
            .chain(credential_ref_list_data)
            .chain(username_password_data)
            .collect();
        let credential_store = CredentialStore::new(&credential_data);

        assert_eq!(credential_store.credentials, expected_result);
    }

    #[test]
    fn test_credential_store_device_credential_all() {
        let _ = env_logger::builder().is_test(true).try_init();
        let expected_result = HashMap::from([
            (
                "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
                ("username_5f".to_string(), Some("password_5f".to_string())),
            ),
            (
                "6a67158b-42b1-400b-8afe-1bec9a5d7909".to_string(),
                ("uname_3".to_string(), Some("admin".to_string())),
            ),
            (
                "7a21dc67-8438-5588-1547-4d1349048438".to_string(),
                ("user_bar".to_string(), Some("password_bar".to_string())),
            ),
        ]);
        let credential_list_data = build_device_credential_list_data();
        let credential_ref_list_data = build_device_credential_ref_list_data();
        let username_password_data = build_username_password_data();
        let credential_data = HashMap::new()
            .into_iter()
            .chain(credential_list_data)
            .chain(credential_ref_list_data)
            .chain(username_password_data)
            .collect();
        let credential_store = CredentialStore::new(&credential_data);

        assert_eq!(credential_store.credentials, expected_result);
    }

    #[test]
    fn test_get_credential_found_no_default() {
        let _ = env_logger::builder().is_test(true).try_init();
        let credential = (
            "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
            ("username_5f".to_string(), Some("password_5f".to_string())),
        );
        let credentials = HashMap::from([credential.clone()]);
        let credential_store = CredentialStore { credentials };
        let result = credential_store.get(&credential.0);
        assert_eq!(result, Some(credential.1));
    }

    #[test]
    fn test_get_credential_not_found_no_default() {
        let _ = env_logger::builder().is_test(true).try_init();
        let credential = (
            "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
            ("username_5f".to_string(), Some("password_5f".to_string())),
        );
        let credentials = HashMap::from([credential]);
        let credential_store = CredentialStore { credentials };
        let result = credential_store.get("not-exist-uuid");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_credential_found_with_default() {
        let _ = env_logger::builder().is_test(true).try_init();
        let credential = (
            "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
            ("username_5f".to_string(), Some("password_5f".to_string())),
        );
        let default_credential = (
            "default".to_string(),
            (
                "default_username".to_string(),
                Some("default_password".to_string()),
            ),
        );
        let credentials = HashMap::from([credential.clone(), default_credential]);
        let credential_store = CredentialStore { credentials };
        let result = credential_store.get(&credential.0);
        assert_eq!(result, Some(credential.1));
    }

    #[test]
    fn test_get_credential_not_found_with_default() {
        let _ = env_logger::builder().is_test(true).try_init();
        let credential = (
            "5f5a69c2-e0ae-504f-829b-00fcdab169cc".to_string(),
            ("username_5f".to_string(), Some("password_5f".to_string())),
        );
        let default_credential = (
            "default".to_string(),
            (
                "default_username".to_string(),
                Some("default_password".to_string()),
            ),
        );
        let credentials = HashMap::from([credential, default_credential.clone()]);
        let credential_store = CredentialStore { credentials };
        let result = credential_store.get("not-exist-uuid");
        assert_eq!(result, Some(default_credential.1));
    }
}
