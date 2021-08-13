#![feature(async_closure)]

use coap::Server;
use coap_lite::{ContentFormat, RequestType as Method, ResponseType as Status};
use tokio::runtime::Runtime;

fn main() {
    let addr = "0.0.0.0:5683";

    Runtime::new().unwrap().block_on(async move {
        let mut server = Server::new(addr).unwrap();
        println!("CoAP server on {}", addr);

        server
            .run(async move |request| {
                println!(
                    "Received request from {} for resource {}",
                    request.source.unwrap(),
                    request.get_path()
                );
                let method = request.get_method().clone();
                let path = request.get_path();
                let mut response = request.response?;

                match (method, path.as_str()) {
                    (Method::Get, "well-known/core") => {
                        response
                            .message
                            .set_content_format(ContentFormat::ApplicationLinkFormat);
                        response.message.payload =
                            br#"</sensors/temp>;rt="oic.r.temperature";if="sensor",
                        </sensors/light>;rt="oic.r.light.brightness";if="sensor""#
                                .to_vec();
                    }
                    (Method::Get, "sensors/temp") => {
                        response
                            .message
                            .set_content_format(ContentFormat::TextPlain);
                        response.message.payload = b"42".to_vec();
                    }
                    (Method::Get, "sensors/light") => {
                        response
                            .message
                            .set_content_format(ContentFormat::TextPlain);
                        response.message.payload = b"100".to_vec();
                    }
                    _ => {
                        response.set_status(Status::NotFound);
                        response.message.payload = b"Not found".to_vec();
                    }
                }

                Some(response)
            })
            .await
            .unwrap();
    });
}
