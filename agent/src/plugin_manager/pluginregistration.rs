/// PluginInfo is the message sent from a plugin to the Kubelet pluginwatcher for plugin registration
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginInfo {
    /// Type of the Plugin. CSIPlugin or DevicePlugin
    #[prost(string, tag = "1")]
    pub r#type: ::prost::alloc::string::String,
    /// Plugin name that uniquely identifies the plugin for the given plugin type.
    /// For DevicePlugin, this is the resource name that the plugin manages and
    /// should follow the extended resource name convention.
    /// For CSI, this is the CSI driver registrar name.
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    /// Optional endpoint location. If found set by Kubelet component,
    /// Kubelet component will use this endpoint for specific requests.
    /// This allows the plugin to register using one endpoint and possibly use
    /// a different socket for control operations. CSI uses this model to delegate
    /// its registration external from the plugin.
    #[prost(string, tag = "3")]
    pub endpoint: ::prost::alloc::string::String,
    /// Plugin service API versions the plugin supports.
    /// For DevicePlugin, this maps to the deviceplugin API versions the
    /// plugin supports at the given socket.
    /// The Kubelet component communicating with the plugin should be able
    /// to choose any preferred version from this list, or returns an error
    /// if none of the listed versions is supported.
    #[prost(string, repeated, tag = "4")]
    pub supported_versions: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// RegistrationStatus is the message sent from Kubelet pluginwatcher to the plugin for notification on registration status
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RegistrationStatus {
    /// True if plugin gets registered successfully at Kubelet
    #[prost(bool, tag = "1")]
    pub plugin_registered: bool,
    /// Error message in case plugin fails to register, empty string otherwise
    #[prost(string, tag = "2")]
    pub error: ::prost::alloc::string::String,
}
/// RegistrationStatusResponse is sent by plugin to kubelet in response to RegistrationStatus RPC
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RegistrationStatusResponse {}
/// InfoRequest is the empty request message from Kubelet
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InfoRequest {}
/// Generated server implementations.
pub mod registration_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with RegistrationServer.
    #[async_trait]
    pub trait Registration: Send + Sync + 'static {
        async fn get_info(
            &self,
            request: tonic::Request<super::InfoRequest>,
        ) -> std::result::Result<tonic::Response<super::PluginInfo>, tonic::Status>;
        async fn notify_registration_status(
            &self,
            request: tonic::Request<super::RegistrationStatus>,
        ) -> std::result::Result<tonic::Response<super::RegistrationStatusResponse>, tonic::Status>;
    }
    /// Registration is the service advertised by the Plugins.
    #[derive(Debug)]
    pub struct RegistrationServer<T: Registration> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Registration> RegistrationServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
                max_decoding_message_size: None,
                max_encoding_message_size: None,
            }
        }
        pub fn with_interceptor<F>(inner: T, interceptor: F) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.max_decoding_message_size = Some(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.max_encoding_message_size = Some(limit);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for RegistrationServer<T>
    where
        T: Registration,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/pluginregistration.Registration/GetInfo" => {
                    #[allow(non_camel_case_types)]
                    struct GetInfoSvc<T: Registration>(pub Arc<T>);
                    impl<T: Registration> tonic::server::UnaryService<super::InfoRequest> for GetInfoSvc<T> {
                        type Response = super::PluginInfo;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::InfoRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut =
                                async move { <T as Registration>::get_info(&inner, request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetInfoSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/pluginregistration.Registration/NotifyRegistrationStatus" => {
                    #[allow(non_camel_case_types)]
                    struct NotifyRegistrationStatusSvc<T: Registration>(pub Arc<T>);
                    impl<T: Registration> tonic::server::UnaryService<super::RegistrationStatus>
                        for NotifyRegistrationStatusSvc<T>
                    {
                        type Response = super::RegistrationStatusResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RegistrationStatus>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Registration>::notify_registration_status(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = NotifyRegistrationStatusSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .header("content-type", "application/grpc")
                        .body(empty_body())
                        .unwrap())
                }),
            }
        }
    }
    impl<T: Registration> Clone for RegistrationServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
                max_decoding_message_size: self.max_decoding_message_size,
                max_encoding_message_size: self.max_encoding_message_size,
            }
        }
    }
    impl<T: Registration> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(Arc::clone(&self.0))
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Registration> tonic::server::NamedService for RegistrationServer<T> {
        const NAME: &'static str = "pluginregistration.Registration";
    }
}
