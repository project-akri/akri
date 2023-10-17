use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use akri_shared::akri::configuration::Configuration;
use clap::Arg;
use k8s_openapi::apimachinery::pkg::runtime::RawExtension;
use openapi::models::{
    V1AdmissionRequest as AdmissionRequest, V1AdmissionResponse as AdmissionResponse,
    V1AdmissionReview as AdmissionReview, V1Status as Status,
};
use openssl::ssl::{SslAcceptor, SslAcceptorBuilder, SslFiletype, SslMethod};
use serde_json::{json, Value};

fn get_builder(key: &str, crt: &str) -> SslAcceptorBuilder {
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    builder.set_private_key_file(key, SslFiletype::PEM).unwrap();
    builder.set_certificate_chain_file(crt).unwrap();

    builder
}
fn check(
    v: &serde_json::Value,
    deserialized: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    if v != &serde_json::Value::Null && deserialized == &serde_json::Value::Null {
        return Err(None.ok_or_else(|| "no matching value in `deserialized`".to_string())?);
    }

    match v {
        serde_json::Value::Object(o) => {
            for (key, value) in o {
                if let Err(e) = check(value, &deserialized[key]) {
                    return Err(None.ok_or(format!(
                        "input key ({:?}) not equal to parsed: ({:?})",
                        key, e
                    ))?);
                }
            }
            Ok(())
        }
        serde_json::Value::Array(s) => {
            for (pos, _e) in s.iter().enumerate() {
                if let Err(e) = check(&s[pos], &deserialized[pos]) {
                    return Err(None.ok_or(format!(
                        "input index ({:?}) not equal to parsed: ({:?})",
                        pos, e
                    ))?);
                }
            }
            Ok(())
        }
        serde_json::Value::String(s) => match deserialized {
            serde_json::Value::String(ds) => {
                if s != ds {
                    Err(None.ok_or(format!("input ({:?}) not equal to parsed ({:?})", s, ds))?)
                } else {
                    Ok(())
                }
            }
            _ => Err(None.ok_or(format!(
                "input ({:?}) not equal to parsed ({:?})",
                s, deserialized
            ))?),
        },
        serde_json::Value::Bool(b) => match deserialized {
            serde_json::Value::Bool(db) => {
                if b != db {
                    Err(None.ok_or(format!("input ({:?}) not equal to parsed ({:?})", b, db))?)
                } else {
                    Ok(())
                }
            }
            _ => Err(None.ok_or(format!(
                "input ({:?}) not equal to parsed ({:?})",
                b, deserialized
            ))?),
        },
        serde_json::Value::Number(n) => match deserialized {
            serde_json::Value::Number(dn) => {
                if n != dn {
                    Err(None.ok_or(format!("input ({:?}) not equal to parsed ({:?})", n, dn))?)
                } else {
                    Ok(())
                }
            }
            _ => Err(None.ok_or(format!(
                "input ({:?}) not equal to parsed ({:?})",
                n, deserialized
            ))?),
        },
        serde_json::Value::Null => match deserialized {
            serde_json::Value::Null => Ok(()),
            _ => Err(None.ok_or(format!(
                "input (Null) not equal to parsed ({:?})",
                deserialized
            ))?),
        },
    }
}

fn filter_configuration(mut v: Value) -> Value {
    let metadata = v["metadata"].as_object_mut().unwrap();
    metadata.remove("creationTimestamp");
    metadata.remove("deletionTimestamp");
    metadata.remove("managedFields");

    let generation = metadata.get_mut("generation").unwrap();
    *generation = json!(generation.as_u64().unwrap());

    v
}

fn validate_configuration(rqst: &AdmissionRequest) -> AdmissionResponse {
    println!("Validating Configuration");
    match &rqst.object {
        Some(raw) => {
            let x: RawExtension = serde_json::from_value(raw.clone())
                .expect("Could not parse as Kubernetes RawExtension");
            let y = serde_json::to_string(&x).unwrap();
            let config: Configuration =
                serde_json::from_str(y.as_str()).expect("Could not parse as Akri Configuration");
            let reserialized = serde_json::to_string(&config).unwrap();
            let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
            println!(
                "validate_configuration - deserialized Configuration: {:?}",
                deserialized
            );
            let val: Value = filter_configuration(raw.clone());
            println!(
                "validate_configuration - expected deserialized format: {:?}",
                val
            );

            // Do they match?
            match check(&val, &deserialized) {
                Ok(_) => AdmissionResponse::new(true, rqst.uid.to_owned()),
                Err(e) => AdmissionResponse {
                    allowed: false,
                    audit_annotations: None,
                    patch: None,
                    patch_type: None,
                    status: Some(Status {
                        api_version: None,
                        code: None,
                        details: None,
                        kind: None,
                        message: Some(e.to_string()),
                        metadata: None,
                        reason: None,
                        status: None,
                    }),
                    uid: rqst.uid.to_owned(),
                    warnings: None,
                },
            }
        }
        None => AdmissionResponse {
            allowed: false,
            audit_annotations: None,
            patch: None,
            patch_type: None,
            status: Some(Status {
                api_version: None,
                code: None,
                details: None,
                kind: None,
                message: Some("AdmissionRequest object contains no data".to_owned()),
                metadata: None,
                reason: None,
                status: None,
            }),
            uid: rqst.uid.to_owned(),
            warnings: None,
        },
    }
}

#[post("/validate")]
async fn validate(rqst: web::Json<AdmissionReview>) -> impl Responder {
    println!("Handler invoked");
    match &rqst.request {
        Some(rqst) => {
            println!("Handler received: AdmissionRequest");
            let resp = validate_configuration(rqst);
            let resp: AdmissionReview = AdmissionReview {
                api_version: Some("admission.k8s.io/v1".to_owned()),
                kind: Some("AdmissionReview".to_owned()),
                request: None,
                response: Some(resp),
            };
            let body = serde_json::to_string(&resp).expect("Valid AdmissionReview");
            HttpResponse::Ok().body(body)
        }
        None => {
            println!("Handler received: Nothing");
            HttpResponse::BadRequest().body("")
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::Command::new("Akri Webhook")
        .arg(
            Arg::new("crt_file")
                .long("tls-crt-file")
                .required(true)
                .help("TLS certificate file"),
        )
        .arg(
            Arg::new("key_file")
                .long("tls-key-file")
                .required(true)
                .help("TLS private key file"),
        )
        .arg(
            Arg::new("port")
                .long("port")
                .value_parser(clap::value_parser!(u16))
                .default_value("8443")
                .required(true)
                .help("port"),
        )
        .get_matches();

    let crt_file = matches
        .get_one::<String>("crt_file")
        .map(|v| v.as_str())
        .expect("TLS certificate file");
    let key_file = matches
        .get_one::<String>("key_file")
        .map(|v| v.as_str())
        .expect("TLS private key file");

    let port = matches
        .get_one::<u16>("port")
        .expect("valid port [0-65535]");

    let endpoint = format!("0.0.0.0:{}", port);
    println!("Started Webhook server: {}", endpoint);

    let builder = get_builder(key_file, crt_file);
    HttpServer::new(|| App::new().service(validate))
        .bind_openssl(endpoint, builder)?
        .run()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    const BROKER_SPEC_INSERTION_KEYWORD: &str = "INSERT_BROKER_SPEC_HERE";
    const DISCOVERY_PROPERTIES_INSERTION_KEYWORD: &str = "INSERT_DISCOVERY_PROPERTIES_HERE";
    const ADMISSION_REVIEW: &str = r#"
    {
        "kind": "AdmissionReview",
        "apiVersion": "admission.k8s.io/v1",
        "request": {
            "uid": "00000000-0000-0000-0000-000000000000",
            "kind": {
                "group": "akri.sh",
                "version": "v0",
                "kind": "Configuration"
            },
            "resource": {
                "group": "akri.sh",
                "version": "v0",
                "resource": "configurations"
            },
            "requestKind": {
                "group": "akri.sh",
                "version": "v0",
                "kind": "Configuration"
            },
            "requestResource": {
                "group": "akri.sh",
                "version": "v0",
                "resource": "configurations"
            },
            "name": "name",
            "namespace": "default",
            "operation": "CREATE",
            "userInfo": {
                "username": "admin",
                "uid": "admin",
                "groups": []
            },
            "object": {
                "apiVersion": "akri.sh/v0",
                "kind": "Configuration",
                "metadata": {
                    "annotations": {
                        "kubectl.kubernetes.io/last-applied-configuration": ""
                    },
                    "creationTimestamp": "2021-01-01T00:00:00Z",
                    "generation": 1,
                    "managedFields": [],
                    "name": "name",
                    "namespace": "default",
                    "uid": "00000000-0000-0000-0000-000000000000"
                },
                "spec": {
                    "discoveryHandler": {
                        "name": "debugEcho",
                        "discoveryDetails": "descriptions:\n- \"foo0\"\n- \"foo1\"\n"
                    },
                    "brokerSpec": {
                        INSERT_BROKER_SPEC_HERE
                    }
                }
            },
            "oldObject": null,
            "dryRun": false,
            "options": {
                "kind": "CreateOptions",
                "apiVersion": "meta.k8s.io/v1"
            }
        }
    }"#;

    const EXTENDED_ADMISSION_REVIEW: &str = r#"
    {
        "kind": "AdmissionReview",
        "apiVersion": "admission.k8s.io/v1",
        "request": {
            "uid": "00000000-0000-0000-0000-000000000000",
            "kind": {
                "group": "akri.sh",
                "version": "v0",
                "kind": "Configuration"
            },
            "resource": {
                "group": "akri.sh",
                "version": "v0",
                "resource": "configurations"
            },
            "requestKind": {
                "group": "akri.sh",
                "version": "v0",
                "kind": "Configuration"
            },
            "requestResource": {
                "group": "akri.sh",
                "version": "v0",
                "resource": "configurations"
            },
            "name": "name",
            "namespace": "default",
            "operation": "CREATE",
            "userInfo": {
                "username": "admin",
                "uid": "admin",
                "groups": []
            },
            "object": {
                "apiVersion": "akri.sh/v0",
                "kind": "Configuration",
                "metadata": {
                    "annotations": {
                        "kubectl.kubernetes.io/last-applied-configuration": ""
                    },
                    "creationTimestamp": "2021-01-01T00:00:00Z",
                    "generation": 1,
                    "managedFields": [],
                    "name": "name",
                    "namespace": "default",
                    "uid": "00000000-0000-0000-0000-000000000000"
                },
                "spec": {
                    "discoveryHandler": {
                        "name": "debugEcho",
                        "discoveryDetails": "{\"descriptions\": [\"foo\",\"bar\"]}"
                    },
                    "brokerSpec": {
                        INSERT_BROKER_SPEC_HERE
                    },
                    "instanceServiceSpec": {
                        "type": "ClusterIP",
                        "ports": [{
                            "name": "name",
                            "port": 0,
                            "targetPort": 0,
                            "protocol": "TCP"
                        }]
                    },
                    "configurationServiceSpec": {
                        "type": "ClusterIP",
                        "ports": [{
                            "name": "name",
                            "port": 0,
                            "targetPort": 0,
                            "protocol": "TCP"
                        }]
                    },
                    "capacity": 1
                }
            },
            "oldObject": null,
            "dryRun": false,
            "options": {
                "kind": "CreateOptions",
                "apiVersion": "meta.k8s.io/v1"
            }
        }
    }"#;

    const ADMISSION_REVIEW_FOR_DISCOVERY_PROPERTIES: &str = r#"
    {
        "kind": "AdmissionReview",
        "apiVersion": "admission.k8s.io/v1",
        "request": {
            "uid": "00000000-0000-0000-0000-000000000000",
            "kind": {
                "group": "akri.sh",
                "version": "v0",
                "kind": "Configuration"
            },
            "resource": {
                "group": "akri.sh",
                "version": "v0",
                "resource": "configurations"
            },
            "requestKind": {
                "group": "akri.sh",
                "version": "v0",
                "kind": "Configuration"
            },
            "requestResource": {
                "group": "akri.sh",
                "version": "v0",
                "resource": "configurations"
            },
            "name": "name",
            "namespace": "default",
            "operation": "CREATE",
            "userInfo": {
                "username": "admin",
                "uid": "admin",
                "groups": []
            },
            "object": {
                "apiVersion": "akri.sh/v0",
                "kind": "Configuration",
                "metadata": {
                    "annotations": {
                        "kubectl.kubernetes.io/last-applied-configuration": ""
                    },
                    "creationTimestamp": "2021-01-01T00:00:00Z",
                    "generation": 1,
                    "managedFields": [],
                    "name": "name",
                    "namespace": "default",
                    "uid": "00000000-0000-0000-0000-000000000000"
                },
                "spec": {
                    "discoveryHandler": {
                        INSERT_DISCOVERY_PROPERTIES_HERE
                        "name": "debugEcho",
                        "discoveryDetails": "descriptions:\n- \"foo0\"\n- \"foo1\"\n"
                    }
                }
            },
            "oldObject": null,
            "dryRun": false,
            "options": {
                "kind": "CreateOptions",
                "apiVersion": "meta.k8s.io/v1"
            }
        }
    }"#;

    const VALID_BROKER_POD_SPEC: &str = r#"
    "brokerPodSpec": {
        "containers": [
            {
                "image": "image",
                "name": "name",
                "resources": {
                    "limits": {
                        "{{PLACEHOLDER}}": "1"
                    }
                }
            }
        ],
        "imagePullSecrets": [
            {
                "name": "name"
            }
        ]
    }"#;

    // Valid JSON but invalid akri.sh/v0/Configuration when inserted into
    // brokerSpec of ADMISSION_REVIEW OR EXTENDED_ADMISSION_REVIEW constants.
    // Misplaced `resources`
    //   Valid: .request.object.spec.brokerSpec.brokerPodSpec.containers[*].resources
    // Invalid: .request.object.spec.brokerSpec.brokerPodSpec.resources
    const INVALID_BROKER_POD_SPEC: &str = r#"
    "brokerPodSpec": {
        "containers": [
            {
                "image": "image",
                "name": "name"
            }
        ],
        "resources": {
            "limits": {
                "{{PLACEHOLDER}}": "1"
            }
        },
        "imagePullSecrets": [
            {
                "name": "name"
            }
        ]
    }"#;

    const VALID_BROKER_JOB_SPEC: &str = r#"
    "brokerJobSpec": {
        "template": {
            "spec": {
                "containers": [
                    {
                        "image": "image",
                        "name": "name",
                        "resources": {
                            "limits": {
                                "{{PLACEHOLDER}}": "1"
                            }
                        }
                    }
                ],
                "imagePullSecrets": [
                    {
                        "name": "name"
                    }
                ]
            }
        }
    }"#;

    // Valid JSON but invalid akri.sh/v0/Configuration when inserted into
    // brokerSpec of ADMISSION_REVIEW OR EXTENDED_ADMISSION_REVIEW constants.
    // Misplaced `resources`
    //   Valid: .request.object.spec.brokerSpec.brokerJobSpec.template.spec.containers[*].resources
    // Invalid: .request.object.spec.brokerSpec.brokerJobSpec.template.spec.resources
    const INVALID_BROKER_JOB_SPEC: &str = r#"
    "brokerJobSpec": {
        "template": {
            "spec": {
                "containers": [
                    {
                        "image": "image",
                        "name": "name",
                        "resources": {
                            "limits": {
                                "{{PLACEHOLDER}}": "1"
                            }
                        }
                    }
                ],
                "resources": {
                    "limits": {
                        "{{PLACEHOLDER}}": "1"
                    }
                },
                "imagePullSecrets": [
                    {
                        "name": "name"
                    }
                ]
            }
        }
    }"#;

    const METADATA: &str = r#"
    {
        "apiVersion": "akri.sh/v0",
        "kind": "Configuration",
        "metadata": {
            "annotations": {
                "kubectl.kubernetes.io/last-applied-configuration": ""
            },
            "creationTimestamp": "2021-01-01T00:00:00Z",
            "generation": 1,
            "managedFields": [],
            "name": "name",
            "namespace": "default",
            "uid": "00000000-0000-0000-0000-000000000000"
        },
        "spec": {}
    }"#;

    fn get_valid_admission_review_with_broker_pod_spec() -> String {
        ADMISSION_REVIEW.replace(BROKER_SPEC_INSERTION_KEYWORD, VALID_BROKER_POD_SPEC)
    }

    fn get_invalid_admission_review_with_broker_pod_spec() -> String {
        ADMISSION_REVIEW.replace(BROKER_SPEC_INSERTION_KEYWORD, INVALID_BROKER_POD_SPEC)
    }

    fn get_extended_admission_review_with_broker_pod_spec() -> String {
        EXTENDED_ADMISSION_REVIEW.replace(BROKER_SPEC_INSERTION_KEYWORD, VALID_BROKER_POD_SPEC)
    }

    fn get_valid_admission_review_with_broker_job_spec() -> String {
        ADMISSION_REVIEW.replace(BROKER_SPEC_INSERTION_KEYWORD, VALID_BROKER_JOB_SPEC)
    }

    fn get_invalid_admission_review_with_broker_job_spec() -> String {
        ADMISSION_REVIEW.replace(BROKER_SPEC_INSERTION_KEYWORD, INVALID_BROKER_JOB_SPEC)
    }

    fn get_invalid_admission_review_with_broker_job_and_pod_spec() -> String {
        let invalid_setting_both_broker_job_and_pod =
            format!("{},\n{}", VALID_BROKER_JOB_SPEC, VALID_BROKER_POD_SPEC);
        ADMISSION_REVIEW.replace(
            BROKER_SPEC_INSERTION_KEYWORD,
            &invalid_setting_both_broker_job_and_pod,
        )
    }

    fn get_admission_review_with_discovery_properties(discovery_properties: &str) -> String {
        ADMISSION_REVIEW_FOR_DISCOVERY_PROPERTIES
            .replace(DISCOVERY_PROPERTIES_INSERTION_KEYWORD, discovery_properties)
    }

    // JSON Syntax Tests
    #[test]
    fn test_both_null() {
        assert!(check(&serde_json::Value::Null, &serde_json::Value::Null).is_ok());
    }

    #[test]
    fn test_value_is_null() {
        let deserialized: Value = serde_json::from_str("{}").unwrap();
        assert!(check(&serde_json::Value::Null, &deserialized).is_err());
    }

    #[test]
    fn test_deserialized_is_null() {
        let v: Value = serde_json::from_str("{}").unwrap();
        assert!(check(&v, &serde_json::Value::Null).is_err());
    }

    #[test]
    fn test_both_empty() {
        let deserialized: Value = serde_json::from_str("{}").unwrap();
        let v: Value = serde_json::from_str("{}").unwrap();
        assert!(check(&v, &deserialized).is_ok());
    }

    #[test]
    fn test_both_same() {
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": true, "d": "hi", "e": [ 1, 2, 3 ] } }"#)
                .unwrap();
        let v: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": true, "d": "hi", "e": [ 1, 2, 3 ] } }"#)
                .unwrap();
        assert!(check(&v, &deserialized).is_ok());
    }

    #[test]
    fn test_deserialized_has_extra() {
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": "hi" } }"#).unwrap();
        let v: Value = serde_json::from_str(r#"{ "a": 1, "b": { "c": 2 } }"#).unwrap();
        assert!(check(&v, &deserialized).is_ok());
    }

    #[test]
    fn test_value_has_extra() {
        let deserialized: Value = serde_json::from_str(r#"{ "a": 1, "b": { "c": 2 } }"#).unwrap();
        let v: Value = serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": "hi" } }"#).unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_value_has_different_types_int_to_str() {
        // value=#, deser=str
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": "hi" } }"#).unwrap();
        let v: Value = serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": 3 } }"#).unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_value_has_different_types_str_to_bool() {
        // value=str, deser=bool
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": true } }"#).unwrap();
        let v: Value = serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": "3" } }"#).unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_value_has_different_types_bool_to_int() {
        // value=bool, deser=#
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": 2 } }"#).unwrap();
        let v: Value = serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": true } }"#).unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_value_has_different_strings() {
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": "hi" } }"#).unwrap();
        let v: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": "hello" } }"#).unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_value_has_different_numbers() {
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": 2, "d": "hi" } }"#).unwrap();
        let v: Value = serde_json::from_str(r#"{ "a": 2, "b": { "c": 2, "d": "hi" } }"#).unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_value_has_different_bools() {
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": true, "d": "hi" } }"#).unwrap();
        let v: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": false, "d": "hi" } }"#).unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_value_has_different_array_element() {
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": true, "d": "hi", "e": [ 1, 2, 3 ] } }"#)
                .unwrap();
        let v: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": true, "d": "hi", "e": [ 1, 5, 3 ] } }"#)
                .unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_value_has_extra_array_element() {
        let deserialized: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": true, "d": "hi", "e": [ 1, 2, 3 ] } }"#)
                .unwrap();
        let v: Value = serde_json::from_str(
            r#"{ "a": 1, "b": { "c": true, "d": "hi", "e": [ 1, 2, 3, 4 ] } }"#,
        )
        .unwrap();
        assert!(check(&v, &deserialized).is_err());
    }

    #[test]
    fn test_deserialized_has_extra_array_element() {
        let deserialized: Value = serde_json::from_str(
            r#"{ "a": 1, "b": { "c": true, "d": "hi", "e": [ 1, 2, 3, 4 ] } }"#,
        )
        .unwrap();
        let v: Value =
            serde_json::from_str(r#"{ "a": 1, "b": { "c": true, "d": "hi", "e": [ 1, 2, 3 ] } }"#)
                .unwrap();
        assert!(check(&v, &deserialized).is_ok());
    }

    // Akri Configuration schema tests
    use kube::api::{NotUsed, Object};
    #[test]
    fn test_creationtimestamp_is_filtered() {
        let t: Object<NotUsed, NotUsed> = serde_json::from_str(METADATA).expect("Valid Metadata");
        let reserialized = serde_json::to_string(&t).expect("bytes");
        let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
        let v = filter_configuration(deserialized);
        assert_eq!(v["metadata"].get("creationTimestamp"), None);
    }

    #[test]
    fn test_deletiontimestamp_is_filtered() {
        let t: Object<NotUsed, NotUsed> = serde_json::from_str(METADATA).expect("Valid Metadata");
        let reserialized = serde_json::to_string(&t).expect("bytes");
        let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
        let v = filter_configuration(deserialized);
        assert_eq!(v["metadata"].get("deletionTimestamp"), None);
    }

    #[test]
    fn test_managedfields_is_filtered() {
        let t: Object<NotUsed, NotUsed> = serde_json::from_str(METADATA).expect("Valid Metadata");
        let reserialized = serde_json::to_string(&t).expect("bytes");
        let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
        let v = filter_configuration(deserialized);
        assert_eq!(v["metadata"].get("managedFields"), None);
    }

    #[test]
    fn test_generation_becomes_u64() {
        let t: Object<NotUsed, NotUsed> = serde_json::from_str(METADATA).expect("Valid Metadata");
        let reserialized = serde_json::to_string(&t).expect("bytes");
        let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
        let v = filter_configuration(deserialized);
        assert!(v["metadata"].get("generation").unwrap().is_u64());
    }

    #[test]
    fn test_validate_configuration_valid_podspec() {
        let valid: AdmissionReview =
            serde_json::from_str(&get_valid_admission_review_with_broker_pod_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = valid.request.expect("v1.AdmissionRequest JSON");
        let resp = validate_configuration(&rqst);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_valid_jobspec() {
        let valid: AdmissionReview =
            serde_json::from_str(&get_valid_admission_review_with_broker_job_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = valid.request.expect("v1.AdmissionRequest JSON");
        let resp = validate_configuration(&rqst);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_invalid_podspec() {
        let invalid: AdmissionReview =
            serde_json::from_str(&get_invalid_admission_review_with_broker_pod_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = invalid.request.expect("v1.AdmissionRequest JSON");
        let resp = validate_configuration(&rqst);
        assert!(!resp.allowed);
    }

    #[test]
    fn test_validate_configuration_invalid_jobspec() {
        let invalid: AdmissionReview =
            serde_json::from_str(&get_invalid_admission_review_with_broker_job_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = invalid.request.expect("v1.AdmissionRequest JSON");
        let resp = validate_configuration(&rqst);
        assert!(!resp.allowed);
    }

    #[test]
    #[should_panic(expected = "Could not parse as Akri Configuration")]
    fn test_validate_configuration_invalid_jobspec_and_podspec() {
        let invalid: AdmissionReview =
            serde_json::from_str(&get_invalid_admission_review_with_broker_job_and_pod_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = invalid.request.expect("v1.AdmissionRequest JSON");
        validate_configuration(&rqst);
    }

    #[test]
    fn test_validate_configuration_extended() {
        let valid: AdmissionReview =
            serde_json::from_str(&get_extended_admission_review_with_broker_pod_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = valid.request.expect("v1.AdmissionRequest JSON");
        let resp = validate_configuration(&rqst);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_discovery_properties_empty() {
        let discovery_properties = "";

        // no discovery properties specified should success
        let resp = run_validate_configuration_discovery_properties(discovery_properties);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_discovery_properties_empty_list() {
        let discovery_properties = r#"
        "discoveryProperties": [],
        "#;

        // empty discovery properties array should success
        let resp = run_validate_configuration_discovery_properties(discovery_properties);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_discovery_properties_plain_text() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "nnnn",
                "value":"vvvv"
            }
        ],
        "#;

        // plain text discovery properties specified should success
        let resp = run_validate_configuration_discovery_properties(discovery_properties);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_discovery_properties_plain_text_empty_value() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "nnnn",
                "value": ""
            }
        ],"#;

        // plain text discovery properties, empty value should success
        let resp = run_validate_configuration_discovery_properties(discovery_properties);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_discovery_properties_plain_text_no_value() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "nnnn"
            }
        ],"#;

        // plain text discovery properties, value not specified should success
        let resp = run_validate_configuration_discovery_properties(discovery_properties);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_discovery_properties_plain_text_empty_name() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": ""
            }
        ],"#;

        // plain text discovery properties, empty name should success
        let resp = run_validate_configuration_discovery_properties(discovery_properties);
        assert!(resp.allowed);
    }

    #[test]
    #[should_panic(expected = "Could not parse as Akri Configuration")]
    fn test_validate_configuration_discovery_properties_plain_text_no_name() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "value":"vvvv"
            }
        ],
        "#;

        // plain text discovery properties without name should fail, missing field 'name'
        run_validate_configuration_discovery_properties(discovery_properties);
    }

    #[test]
    #[should_panic(expected = "Could not parse as Akri Configuration")]
    fn test_validate_configuration_discovery_properties_empty_value_from() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "nnnn",
                "valueFrom": {
                }
            }
        ],"#;

        // valueFrom discovery properties, not specified should fail, missing content of 'valueFrom'
        run_validate_configuration_discovery_properties(discovery_properties);
    }

    #[test]
    #[should_panic(expected = "Could not parse as Akri Configuration")]
    fn test_validate_configuration_discovery_properties_unknown_value_from() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "nnnn",
                "valueFrom": {
                    "fieldRef": {
                        "fieldPath": "ffff"
                    }
                }
            }
        ],"#;

        // valueFrom discovery properties, unknown ref should fail
        run_validate_configuration_discovery_properties(discovery_properties);
    }

    #[test]
    fn test_validate_configuration_discovery_properties_secret() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "nnnn",
                "valueFrom": {
                    "secretKeyRef": {
                        "name": "nnnn1",
                        "key": "kkk",
                        "optional": false
                    }
                }
            }
        ],"#;

        // valueFrom discovery properties, secretKeyRef should success
        let resp = run_validate_configuration_discovery_properties(discovery_properties);
        assert!(resp.allowed);
    }

    #[test]
    fn test_validate_configuration_discovery_properties_config_map() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "nnnn",
                "valueFrom": {
                    "configMapKeyRef": {
                        "name": "nnnn1",
                        "key": "kkk",
                        "optional": false
                    }
                }
            }
        ],"#;

        // valueFrom discovery properties, configMapKeyRef should success
        let resp = run_validate_configuration_discovery_properties(discovery_properties);
        assert!(resp.allowed);
    }

    #[test]
    #[should_panic(expected = "Could not parse as Akri Configuration")]
    fn test_validate_configuration_discovery_properties_config_map_invalid_ref_name_xxx() {
        // invalid "name1" in configMapKeyRef
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "nnnn",
                "valueFrom": {
                    "configMapKeyRef": {
                        "name1": "nnnn1",
                        "key": "kkk",
                        "optional": false
                    }
                }
            }
        ],"#;

        run_validate_configuration_discovery_properties(discovery_properties);
    }

    #[test]
    #[should_panic(expected = "Could not parse as Akri Configuration")]
    fn test_validate_configuration_discovery_properties_config_map_multiple_value_from() {
        let discovery_properties = r#"
        "discoveryProperties": [
            {
                "name": "name1",
                "valueFrom": {
                    "configMapKeyRef": {
                        "name": "nnnn1",
                        "key": "kkk",
                        "optional": false
                    },
                    "secretKeyRef": {
                        "name": "nnnn1",
                        "key": "kkk",
                        "optional": false
                    }
                }
            }
        ],"#;

        run_validate_configuration_discovery_properties(discovery_properties);
    }

    fn run_validate_configuration_discovery_properties(
        discovery_properties: &str,
    ) -> AdmissionResponse {
        let valid: AdmissionReview = serde_json::from_str(
            &get_admission_review_with_discovery_properties(discovery_properties),
        )
        .expect("v1.AdmissionReview JSON");
        let rqst = valid.request.expect("v1.AdmissionRequest JSON");
        validate_configuration(&rqst)
    }

    #[actix_web::test]
    async fn test_validate_valid_podspec() {
        let app = actix_web::test::init_service(App::new().service(validate)).await;
        let valid: AdmissionReview =
            serde_json::from_str(&get_valid_admission_review_with_broker_pod_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = actix_web::test::TestRequest::post()
            .uri("/validate")
            .set_json(&valid)
            .to_request();
        let resp = actix_web::test::call_service(&app, rqst).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_validate_valid_jobspec() {
        let app = actix_web::test::init_service(App::new().service(validate)).await;
        let valid: AdmissionReview =
            serde_json::from_str(&get_valid_admission_review_with_broker_job_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = actix_web::test::TestRequest::post()
            .uri("/validate")
            .set_json(&valid)
            .to_request();
        let resp = actix_web::test::call_service(&app, rqst).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_validate_invalid_podspec() {
        let app = actix_web::test::init_service(App::new().service(validate)).await;
        let invalid: AdmissionReview =
            serde_json::from_str(&get_invalid_admission_review_with_broker_pod_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = actix_web::test::TestRequest::post()
            .uri("/validate")
            .set_json(&invalid)
            .to_request();
        let resp = actix_web::test::call_service(&app, rqst).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_validate_invalid_jobspec() {
        let app = actix_web::test::init_service(App::new().service(validate)).await;
        let invalid: AdmissionReview =
            serde_json::from_str(&get_invalid_admission_review_with_broker_job_spec())
                .expect("v1.AdmissionReview JSON");
        let rqst = actix_web::test::TestRequest::post()
            .uri("/validate")
            .set_json(&invalid)
            .to_request();
        let resp = actix_web::test::call_service(&app, rqst).await;
        assert!(resp.status().is_success());
    }
}
