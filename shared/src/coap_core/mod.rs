use std::time::Duration;
use coap_lite::CoapResponse;

pub trait CoAPClient {
    fn get_with_timeout(&self, url: &str, timeout: Duration) -> std::io::Result<CoapResponse>;
}

pub struct CoAPClientImpl {}

impl CoAPClient for CoAPClientImpl {
    fn get_with_timeout(&self, url: &str, timeout: Duration) -> std::io::Result<CoapResponse> {
        use coap::CoAPClient;

        CoAPClient::get_with_timeout(url, timeout)
    }
}

pub mod test_coap_core {
    use super::*;
    use mockall::predicate::*;
    use mockall::*;

    mock! {
        pub CoAPClient {
            fn get_with_timeout(&self, url: &str, timeout: Duration) -> std::io::Result<CoapResponse>;
        }
    }

    impl super::CoAPClient for MockCoAPClient {
        fn get_with_timeout(&self, url: &str, timeout: Duration) -> std::io::Result<CoapResponse> {
            self.get_with_timeout(url, timeout)
        }
    }
}
