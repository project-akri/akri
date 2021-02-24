use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use akri_shared::akri::configuration::KubeAkriConfig;
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
        return Err(None.ok_or(format!("no matching value in `deserialized`"))?);
    }

    match v {
        serde_json::Value::Object(o) => {
            for (key, value) in o {
                if let Err(e) = check(&value, &deserialized[key]) {
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
    *generation = json!(generation.as_f64().unwrap());

    v
}
fn validate_configuration(rqst: &AdmissionRequest) -> AdmissionResponse {
    println!("Validating Configuration");
    match &rqst.object {
        Some(raw) => {
            let x: RawExtension = serde_json::from_value(raw.clone())
                .expect("Could not parse as Kubernetes RawExtension");
            let y = serde_json::to_string(&x).unwrap();
            let c: KubeAkriConfig =
                serde_json::from_str(y.as_str()).expect("Could not parse as Akri Configuration");
            let reserialized = serde_json::to_string(&c).unwrap();
            let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");

            let v: Value = filter_configuration(raw.clone());

            // Do they match?
            match check(&v, &deserialized) {
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
            let resp = validate_configuration(&rqst);
            let resp: AdmissionReview = AdmissionReview {
                api_version: Some("admission.k8s.io/v1".to_owned()),
                kind: Some("AdmissionReview".to_owned()),
                request: None,
                response: Some(resp),
            };
            let body = serde_json::to_string(&resp).expect("Valid AdmissionReview");
            return HttpResponse::Ok().body(body);
        }
        None => {
            println!("Handler received: Nothing");
            return HttpResponse::BadRequest().body("");
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("Akri Webhook")
        .arg(
            Arg::new("crt_file")
                .long("tls-crt-file")
                .takes_value(true)
                .required(true)
                .about("TLS certificate file"),
        )
        .arg(
            Arg::new("key_file")
                .long("tls-key-file")
                .takes_value(true)
                .required(true)
                .about("TLS private key file"),
        )
        .arg(
            Arg::new("port")
                .long("port")
                .takes_value(true)
                .required(true)
                .about("port"),
        )
        .get_matches();

    let crt_file = matches.value_of("crt_file").expect("TLS certificate file");
    let key_file = matches.value_of("key_file").expect("TLS private key file");

    let port = matches
        .value_of("port")
        .unwrap_or("8443")
        .parse::<u16>()
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
    use actix_web::test;

    const VALID: &str = r#"
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
                    "protocol": {
                        "debugEcho": {
                            "descriptions": ["foo","bar"],
                            "shared": true
                        }
                    },
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
    }
    "#;

    // Valid JSON but invalid akri.sh/v0/Configuration
    // Misplaced `resources`
    //   Valid: .request.object.spec.brokerPodSpec.containers[*].resources
    // Invalid: .request.object.spec.brokerPodSpec.resources
    const INVALID: &str = r#"
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
                    "protocol": {
                        "debugEcho": {
                            "descriptions": ["foo","bar"],
                            "shared": true
                        }
                    },
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
    }
    "#;

    const EXTENDED: &str = r#"
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
                    "protocol": {
                        "debugEcho": {
                            "descriptions": ["foo","bar"],
                            "shared": true
                        }
                    },
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
    }
    "#;

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
    }
    "#;

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
    use kube::api::{Object, Void};
    #[test]
    fn test_creationtimestamp_is_filtered() {
        let t: Object<Void, Void> = serde_json::from_str(METADATA).expect("Valid Metadata");
        let reserialized = serde_json::to_string(&t).expect("bytes");
        let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
        let v = filter_configuration(deserialized);
        assert_eq!(v["metadata"].get("creationTimestamp"), None);
    }

    #[test]
    fn test_deletiontimestamp_is_filtered() {
        let t: Object<Void, Void> = serde_json::from_str(METADATA).expect("Valid Metadata");
        let reserialized = serde_json::to_string(&t).expect("bytes");
        let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
        let v = filter_configuration(deserialized);
        assert_eq!(v["metadata"].get("deletionTimestamp"), None);
    }

    #[test]
    fn test_managedfields_is_filtered() {
        let t: Object<Void, Void> = serde_json::from_str(METADATA).expect("Valid Metadata");
        let reserialized = serde_json::to_string(&t).expect("bytes");
        let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
        let v = filter_configuration(deserialized);
        assert_eq!(v["metadata"].get("managedFields"), None);
    }

    #[test]
    fn test_generation_becomes_f64() {
        let t: Object<Void, Void> = serde_json::from_str(METADATA).expect("Valid Metadata");
        let reserialized = serde_json::to_string(&t).expect("bytes");
        let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");
        let v = filter_configuration(deserialized);
        assert!(v["metadata"].get("generation").unwrap().is_f64());
    }

    #[test]
    fn test_validate_configuration_valid() {
        let valid: AdmissionReview = serde_json::from_str(VALID).expect("v1.AdmissionReview JSON");
        let rqst = valid.request.expect("v1.AdmissionRequest JSON");
        let resp = validate_configuration(&rqst);
        assert_eq!(resp.allowed, true);
    }

    #[test]
    fn test_validate_configuration_invalid() {
        let invalid: AdmissionReview =
            serde_json::from_str(INVALID).expect("v1.AdmissionReview JSON");
        let rqst = invalid.request.expect("v1.AdmissionRequest JSON");
        let resp = validate_configuration(&rqst);
        assert_eq!(resp.allowed, false);
    }

    #[test]
    fn test_validate_configuration_extended() {
        let valid: AdmissionReview =
            serde_json::from_str(EXTENDED).expect("v1.AdmissionReview JSON");
        let rqst = valid.request.expect("v1.AdmissionRequest JSON");
        let resp = validate_configuration(&rqst);
        assert_eq!(resp.allowed, true);
    }

    #[actix_rt::test]
    async fn test_validate_valid() {
        let mut app = test::init_service(App::new().service(validate)).await;
        let valid: AdmissionReview = serde_json::from_str(VALID).expect("v1.AdmissionReview JSON");
        let rqst = test::TestRequest::post()
            .uri("/validate")
            .set_json(&valid)
            .to_request();
        let resp = test::call_service(&mut app, rqst).await;
        assert_eq!(resp.status().is_success(), true);
    }

    #[actix_rt::test]
    async fn test_validate_invalid() {
        let mut app = test::init_service(App::new().service(validate)).await;
        let invalid: AdmissionReview =
            serde_json::from_str(INVALID).expect("v1.AdmissionReview JSON");
        let rqst = test::TestRequest::post()
            .uri("/validate")
            .set_json(&invalid)
            .to_request();
        let resp = test::call_service(&mut app, rqst).await;
        assert_eq!(resp.status().is_success(), true);
    }
}
