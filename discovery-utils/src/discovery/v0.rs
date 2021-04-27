#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RegisterDiscoveryHandlerRequest {
    /// Name of the `DiscoveryHandler`. This name is specified in an
    /// Akri Configuration, to request devices discovered by this `DiscoveryHandler`.
    #[prost(string, tag = "1")]
    pub name: std::string::String,
    /// Endpoint for the registering `DiscoveryHandler`
    #[prost(string, tag = "2")]
    pub endpoint: std::string::String,
    #[prost(
        enumeration = "register_discovery_handler_request::EndpointType",
        tag = "3"
    )]
    pub endpoint_type: i32,
    /// Specifies whether this device could be used by multiple nodes (e.g. an IP camera)
    /// or can only be ever be discovered by a single node (e.g. a local USB device)
    #[prost(bool, tag = "4")]
    pub shared: bool,
}
pub mod register_discovery_handler_request {
    /// Specifies the type of endpoint.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum EndpointType {
        Uds = 0,
        Network = 1,
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Empty {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DiscoverRequest {
    /// String containing all the details (such as filtering options)
    /// the `DiscoveryHandler` needs to find a set of devices.
    #[prost(string, tag = "1")]
    pub discovery_details: std::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DiscoverResponse {
    /// List of discovered devices
    #[prost(message, repeated, tag = "1")]
    pub devices: ::std::vec::Vec<Device>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Device {
    /// Identifier for this device
    #[prost(string, tag = "1")]
    pub id: std::string::String,
    /// Properties that identify the device. These are stored in the device's instance
    /// and set as environment variables in the device's broker Pods. May be information
    /// about where to find the device such as an RTSP URL or a device node (e.g. `/dev/video1`)
    #[prost(map = "string, string", tag = "2")]
    pub properties: ::std::collections::HashMap<std::string::String, std::string::String>,
    /// Optionally specify mounts for Pods that request this device as a resource
    #[prost(message, repeated, tag = "3")]
    pub mounts: ::std::vec::Vec<Mount>,
    /// Optionally specify device information to be mounted for Pods that request this device as a resource
    #[prost(message, repeated, tag = "4")]
    pub device_specs: ::std::vec::Vec<DeviceSpec>,
}
/// From Device Plugin  API
/// Mount specifies a host volume to mount into a container.
/// where device library or tools are installed on host and container
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Mount {
    /// Path of the mount within the container.
    #[prost(string, tag = "1")]
    pub container_path: std::string::String,
    /// Path of the mount on the host.
    #[prost(string, tag = "2")]
    pub host_path: std::string::String,
    /// If set, the mount is read-only.
    #[prost(bool, tag = "3")]
    pub read_only: bool,
}
/// From Device Plugin API
/// DeviceSpec specifies a host device to mount into a container.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeviceSpec {
    /// Path of the device within the container.
    #[prost(string, tag = "1")]
    pub container_path: std::string::String,
    /// Path of the device on the host.
    #[prost(string, tag = "2")]
    pub host_path: std::string::String,
    /// Cgroups permissions of the device, candidates are one or more of
    /// * r - allows container to read from the specified device.
    /// * w - allows container to write to the specified device.
    /// * m - allows container to create device files that do not yet exist.
    #[prost(string, tag = "3")]
    pub permissions: std::string::String,
}
#[doc = r" Generated client implementations."]
pub mod registration_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " Registration is the service advertised by the Akri Agent."]
    #[doc = " Any `DiscoveryHandler` can register with the Akri Agent."]
    pub struct RegistrationClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl RegistrationClient<tonic::transport::Channel> {
        #[doc = r" Attempt to create a new client by connecting to a given endpoint."]
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> RegistrationClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::ResponseBody: Body + HttpBody + Send + 'static,
        T::Error: Into<StdError>,
        <T::ResponseBody as HttpBody>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor(inner: T, interceptor: impl Into<tonic::Interceptor>) -> Self {
            let inner = tonic::client::Grpc::with_interceptor(inner, interceptor);
            Self { inner }
        }
        pub async fn register_discovery_handler(
            &mut self,
            request: impl tonic::IntoRequest<super::RegisterDiscoveryHandlerRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/v0.Registration/RegisterDiscoveryHandler");
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
    impl<T: Clone> Clone for RegistrationClient<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }
}
#[doc = r" Generated client implementations."]
pub mod discovery_handler_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    pub struct DiscoveryHandlerClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl DiscoveryHandlerClient<tonic::transport::Channel> {
        #[doc = r" Attempt to create a new client by connecting to a given endpoint."]
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> DiscoveryHandlerClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::ResponseBody: Body + HttpBody + Send + 'static,
        T::Error: Into<StdError>,
        <T::ResponseBody as HttpBody>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor(inner: T, interceptor: impl Into<tonic::Interceptor>) -> Self {
            let inner = tonic::client::Grpc::with_interceptor(inner, interceptor);
            Self { inner }
        }
        pub async fn discover(
            &mut self,
            request: impl tonic::IntoRequest<super::DiscoverRequest>,
        ) -> Result<tonic::Response<tonic::codec::Streaming<super::DiscoverResponse>>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/v0.DiscoveryHandler/Discover");
            self.inner
                .server_streaming(request.into_request(), path, codec)
                .await
        }
    }
    impl<T: Clone> Clone for DiscoveryHandlerClient<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }
}
#[doc = r" Generated server implementations."]
pub mod registration_server {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with RegistrationServer."]
    #[async_trait]
    pub trait Registration: Send + Sync + 'static {
        async fn register_discovery_handler(
            &self,
            request: tonic::Request<super::RegisterDiscoveryHandlerRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
    }
    #[doc = " Registration is the service advertised by the Akri Agent."]
    #[doc = " Any `DiscoveryHandler` can register with the Akri Agent."]
    #[derive(Debug)]
    #[doc(hidden)]
    pub struct RegistrationServer<T: Registration> {
        inner: _Inner<T>,
    }
    struct _Inner<T>(Arc<T>, Option<tonic::Interceptor>);
    impl<T: Registration> RegistrationServer<T> {
        pub fn new(inner: T) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner, None);
            Self { inner }
        }
        pub fn with_interceptor(inner: T, interceptor: impl Into<tonic::Interceptor>) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner, Some(interceptor.into()));
            Self { inner }
        }
    }
    impl<T: Registration> Service<http::Request<HyperBody>> for RegistrationServer<T> {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = Never;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<HyperBody>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/v0.Registration/RegisterDiscoveryHandler" => {
                    struct RegisterDiscoveryHandlerSvc<T: Registration>(pub Arc<T>);
                    impl<T: Registration>
                        tonic::server::UnaryService<super::RegisterDiscoveryHandlerRequest>
                        for RegisterDiscoveryHandlerSvc<T>
                    {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RegisterDiscoveryHandlerRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut =
                                async move { inner.register_discovery_handler(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = RegisterDiscoveryHandlerSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .body(tonic::body::BoxBody::empty())
                        .unwrap())
                }),
            }
        }
    }
    impl<T: Registration> Clone for RegistrationServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self { inner }
        }
    }
    impl<T: Registration> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone(), self.1.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Registration> tonic::transport::NamedService for RegistrationServer<T> {
        const NAME: &'static str = "v0.Registration";
    }
}
#[doc = r" Generated server implementations."]
pub mod discovery_handler_server {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with DiscoveryHandlerServer."]
    #[async_trait]
    pub trait DiscoveryHandler: Send + Sync + 'static {
        #[doc = "Server streaming response type for the Discover method."]
        type DiscoverStream: Stream<Item = Result<super::DiscoverResponse, tonic::Status>>
            + Send
            + Sync
            + 'static;
        async fn discover(
            &self,
            request: tonic::Request<super::DiscoverRequest>,
        ) -> Result<tonic::Response<Self::DiscoverStream>, tonic::Status>;
    }
    #[derive(Debug)]
    #[doc(hidden)]
    pub struct DiscoveryHandlerServer<T: DiscoveryHandler> {
        inner: _Inner<T>,
    }
    struct _Inner<T>(Arc<T>, Option<tonic::Interceptor>);
    impl<T: DiscoveryHandler> DiscoveryHandlerServer<T> {
        pub fn new(inner: T) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner, None);
            Self { inner }
        }
        pub fn with_interceptor(inner: T, interceptor: impl Into<tonic::Interceptor>) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner, Some(interceptor.into()));
            Self { inner }
        }
    }
    impl<T: DiscoveryHandler> Service<http::Request<HyperBody>> for DiscoveryHandlerServer<T> {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = Never;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<HyperBody>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/v0.DiscoveryHandler/Discover" => {
                    struct DiscoverSvc<T: DiscoveryHandler>(pub Arc<T>);
                    impl<T: DiscoveryHandler>
                        tonic::server::ServerStreamingService<super::DiscoverRequest>
                        for DiscoverSvc<T>
                    {
                        type Response = super::DiscoverResponse;
                        type ResponseStream = T::DiscoverStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::DiscoverRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.discover(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1;
                        let inner = inner.0;
                        let method = DiscoverSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .body(tonic::body::BoxBody::empty())
                        .unwrap())
                }),
            }
        }
    }
    impl<T: DiscoveryHandler> Clone for DiscoveryHandlerServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self { inner }
        }
    }
    impl<T: DiscoveryHandler> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone(), self.1.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: DiscoveryHandler> tonic::transport::NamedService for DiscoveryHandlerServer<T> {
        const NAME: &'static str = "v0.DiscoveryHandler";
    }
}
