use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use akri_shared::akri::configuration::KubeAkriConfig;
use clap::Arg;
use k8s_openapi::apimachinery::pkg::runtime::RawExtension;
use openapi::models::{
    V1AdmissionRequest as AdmissionRequest, V1AdmissionResponse as AdmissionResponse,
    V1AdmissionReview as AdmissionReview, V1Status as Status,
};
use openssl::ssl::{SslAcceptor, SslAcceptorBuilder, SslFiletype, SslMethod};
use serde_json::Value;

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
    if deserialized == &serde_json::Value::Null {
        return Err(None.ok_or(format!("no matching value in `deserialized`"))?);
    }

    match v {
        serde_json::Value::Object(o) => {
            for (key, value) in o {
                // TODO(dazwilkin) Issue serde'ing `creationTimestamp`
                if key == "creationTimestamp" {
                    return Ok(());
                }
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
        _ => Err(None.ok_or(format!("what is this? {:?}", "boooo!"))?),
    }
}

fn validate_configuration(rqst: &AdmissionRequest) -> AdmissionResponse {
    match &rqst.object {
        Some(raw) => {
            let x: RawExtension = serde_json::from_value(raw.clone()).expect("RawExtension");
            let y = serde_json::to_string(&x).expect("success");
            let c: KubeAkriConfig = serde_json::from_str(y.as_str()).expect("success");
            let reserialized = serde_json::to_string(&c).expect("bytes");
            let deserialized: Value = serde_json::from_str(&reserialized).expect("untyped JSON");

            let v: Value = serde_json::from_value(raw.clone()).expect("RawExtension");

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
    match &rqst.request {
        Some(rqst) => {
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
                    "capacity": 1,
                    "protocol": {
                        "debugEcho": {
                            "descriptions": ["foo","bar"],
                            "shared": true
                        }
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
                    "capacity": 1,
                    "protocol": {
                        "debugEcho": {
                            "descriptions": ["foo","bar"],
                            "shared": true
                        }
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
    }
    "#;

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
