#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DevicePluginOptions {
    /// Indicates if PreStartContainer call is required before each container start
    #[prost(bool, tag = "1")]
    pub pre_start_required: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RegisterRequest {
    /// Version of the API the Device Plugin was built against
    #[prost(string, tag = "1")]
    pub version: ::prost::alloc::string::String,
    /// Name of the unix socket the device plugin is listening on
    /// PATH = path.Join(DevicePluginPath, endpoint)
    #[prost(string, tag = "2")]
    pub endpoint: ::prost::alloc::string::String,
    /// Schedulable resource name. As of now it's expected to be a DNS Label
    #[prost(string, tag = "3")]
    pub resource_name: ::prost::alloc::string::String,
    /// Options to be communicated with Device Manager
    #[prost(message, optional, tag = "4")]
    pub options: ::core::option::Option<DevicePluginOptions>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Empty {}
/// ListAndWatch returns a stream of List of Devices
/// Whenever a Device state change or a Device disapears, ListAndWatch
/// returns the new list
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListAndWatchResponse {
    #[prost(message, repeated, tag = "1")]
    pub devices: ::prost::alloc::vec::Vec<Device>,
}
/// E.g:
/// struct Device {
///     ID: "GPU-fef8089b-4820-abfc-e83e-94318197576e",
///     State: "Healthy",
/// }
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Device {
    /// A unique ID assigned by the device plugin used
    /// to identify devices during the communication
    /// Max length of this field is 63 characters
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    /// Health of the device, can be healthy or unhealthy, see constants.go
    #[prost(string, tag = "2")]
    pub health: ::prost::alloc::string::String,
}
/// - PreStartContainer is expected to be called before each container start if indicated by plugin during registration phase.
/// - PreStartContainer allows kubelet to pass reinitialized devices to containers.
/// - PreStartContainer allows Device Plugin to run device specific operations on
///    the Devices requested
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PreStartContainerRequest {
    #[prost(string, repeated, tag = "1")]
    pub devices_i_ds: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// PreStartContainerResponse will be send by plugin in response to PreStartContainerRequest
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PreStartContainerResponse {}
/// - Allocate is expected to be called during pod creation since allocation
///    failures for any container would result in pod startup failure.
/// - Allocate allows kubelet to exposes additional artifacts in a pod's
///    environment as directed by the plugin.
/// - Allocate allows Device Plugin to run device specific operations on
///    the Devices requested
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AllocateRequest {
    #[prost(message, repeated, tag = "1")]
    pub container_requests: ::prost::alloc::vec::Vec<ContainerAllocateRequest>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContainerAllocateRequest {
    #[prost(string, repeated, tag = "1")]
    pub devices_i_ds: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// AllocateResponse includes the artifacts that needs to be injected into
/// a container for accessing 'deviceIDs' that were mentioned as part of
/// 'AllocateRequest'.
/// Failure Handling:
/// if Kubelet sends an allocation request for dev1 and dev2.
/// Allocation on dev1 succeeds but allocation on dev2 fails.
/// The Device plugin should send a ListAndWatch update and fail the
/// Allocation request
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AllocateResponse {
    #[prost(message, repeated, tag = "1")]
    pub container_responses: ::prost::alloc::vec::Vec<ContainerAllocateResponse>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContainerAllocateResponse {
    /// List of environment variable to be set in the container to access one of more devices.
    #[prost(map = "string, string", tag = "1")]
    pub envs:
        ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
    /// Mounts for the container.
    #[prost(message, repeated, tag = "2")]
    pub mounts: ::prost::alloc::vec::Vec<Mount>,
    /// Devices for the container.
    #[prost(message, repeated, tag = "3")]
    pub devices: ::prost::alloc::vec::Vec<DeviceSpec>,
    /// Container annotations to pass to the container runtime
    #[prost(map = "string, string", tag = "4")]
    pub annotations:
        ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
}
/// Mount specifies a host volume to mount into a container.
/// where device library or tools are installed on host and container
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Mount {
    /// Path of the mount within the container.
    #[prost(string, tag = "1")]
    pub container_path: ::prost::alloc::string::String,
    /// Path of the mount on the host.
    #[prost(string, tag = "2")]
    pub host_path: ::prost::alloc::string::String,
    /// If set, the mount is read-only.
    #[prost(bool, tag = "3")]
    pub read_only: bool,
}
/// DeviceSpec specifies a host device to mount into a container.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeviceSpec {
    /// Path of the device within the container.
    #[prost(string, tag = "1")]
    pub container_path: ::prost::alloc::string::String,
    /// Path of the device on the host.
    #[prost(string, tag = "2")]
    pub host_path: ::prost::alloc::string::String,
    /// Cgroups permissions of the device, candidates are one or more of
    /// * r - allows container to read from the specified device.
    /// * w - allows container to write to the specified device.
    /// * m - allows container to create device files that do not yet exist.
    #[prost(string, tag = "3")]
    pub permissions: ::prost::alloc::string::String,
}
/// Generated client implementations.
pub mod registration_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::http::Uri;
    use tonic::codegen::*;
    /// Registration is the service advertised by the Kubelet
    /// Only when Kubelet answers with a success code to a Register Request
    /// may Device Plugins start their service
    /// Registration may fail when device plugin version is not supported by
    /// Kubelet or the registered resourceName is already taken by another
    /// active device plugin. Device plugin is expected to terminate upon registration failure
    #[derive(Debug, Clone)]
    pub struct RegistrationClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl RegistrationClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> RegistrationClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> RegistrationClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<http::Request<tonic::body::BoxBody>>>::Error:
                Into<StdError> + Send + Sync,
        {
            RegistrationClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_decoding_message_size(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_encoding_message_size(limit);
            self
        }
        pub async fn register(
            &mut self,
            request: impl tonic::IntoRequest<super::RegisterRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/v1beta1.Registration/Register");
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("v1beta1.Registration", "Register"));
            self.inner.unary(req, path, codec).await
        }
    }
}
/// Generated client implementations.
pub mod device_plugin_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::http::Uri;
    use tonic::codegen::*;
    /// DevicePlugin is the service advertised by Device Plugins
    #[derive(Debug, Clone)]
    pub struct DevicePluginClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl DevicePluginClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> DevicePluginClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> DevicePluginClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<http::Request<tonic::body::BoxBody>>>::Error:
                Into<StdError> + Send + Sync,
        {
            DevicePluginClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_decoding_message_size(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_encoding_message_size(limit);
            self
        }
        /// GetDevicePluginOptions returns options to be communicated with Device
        /// Manager
        pub async fn get_device_plugin_options(
            &mut self,
            request: impl tonic::IntoRequest<super::Empty>,
        ) -> std::result::Result<tonic::Response<super::DevicePluginOptions>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/v1beta1.DevicePlugin/GetDevicePluginOptions",
            );
            let mut req = request.into_request();
            req.extensions_mut().insert(GrpcMethod::new(
                "v1beta1.DevicePlugin",
                "GetDevicePluginOptions",
            ));
            self.inner.unary(req, path, codec).await
        }
        /// ListAndWatch returns a stream of List of Devices
        /// Whenever a Device state change or a Device disapears, ListAndWatch
        /// returns the new list
        pub async fn list_and_watch(
            &mut self,
            request: impl tonic::IntoRequest<super::Empty>,
        ) -> std::result::Result<
            tonic::Response<tonic::codec::Streaming<super::ListAndWatchResponse>>,
            tonic::Status,
        > {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/v1beta1.DevicePlugin/ListAndWatch");
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("v1beta1.DevicePlugin", "ListAndWatch"));
            self.inner.server_streaming(req, path, codec).await
        }
        /// Allocate is called during container creation so that the Device
        /// Plugin can run device specific operations and instruct Kubelet
        /// of the steps to make the Device available in the container
        pub async fn allocate(
            &mut self,
            request: impl tonic::IntoRequest<super::AllocateRequest>,
        ) -> std::result::Result<tonic::Response<super::AllocateResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/v1beta1.DevicePlugin/Allocate");
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("v1beta1.DevicePlugin", "Allocate"));
            self.inner.unary(req, path, codec).await
        }
        /// PreStartContainer is called, if indicated by Device Plugin during registeration phase,
        /// before each container start. Device plugin can run device specific operations
        /// such as reseting the device before making devices available to the container
        pub async fn pre_start_container(
            &mut self,
            request: impl tonic::IntoRequest<super::PreStartContainerRequest>,
        ) -> std::result::Result<tonic::Response<super::PreStartContainerResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/v1beta1.DevicePlugin/PreStartContainer");
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("v1beta1.DevicePlugin", "PreStartContainer"));
            self.inner.unary(req, path, codec).await
        }
    }
}
/// Generated server implementations.
pub mod registration_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with RegistrationServer.
    #[async_trait]
    pub trait Registration: Send + Sync + 'static {
        async fn register(
            &self,
            request: tonic::Request<super::RegisterRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
    }
    /// Registration is the service advertised by the Kubelet
    /// Only when Kubelet answers with a success code to a Register Request
    /// may Device Plugins start their service
    /// Registration may fail when device plugin version is not supported by
    /// Kubelet or the registered resourceName is already taken by another
    /// active device plugin. Device plugin is expected to terminate upon registration failure
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
                "/v1beta1.Registration/Register" => {
                    #[allow(non_camel_case_types)]
                    struct RegisterSvc<T: Registration>(pub Arc<T>);
                    impl<T: Registration> tonic::server::UnaryService<super::RegisterRequest> for RegisterSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RegisterRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut =
                                async move { <T as Registration>::register(&inner, request).await };
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
                        let method = RegisterSvc(inner);
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
        const NAME: &'static str = "v1beta1.Registration";
    }
}
/// Generated server implementations.
pub mod device_plugin_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with DevicePluginServer.
    #[async_trait]
    pub trait DevicePlugin: Send + Sync + 'static {
        /// GetDevicePluginOptions returns options to be communicated with Device
        /// Manager
        async fn get_device_plugin_options(
            &self,
            request: tonic::Request<super::Empty>,
        ) -> std::result::Result<tonic::Response<super::DevicePluginOptions>, tonic::Status>;
        /// Server streaming response type for the ListAndWatch method.
        type ListAndWatchStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<super::ListAndWatchResponse, tonic::Status>,
            > + Send
            + 'static;
        /// ListAndWatch returns a stream of List of Devices
        /// Whenever a Device state change or a Device disapears, ListAndWatch
        /// returns the new list
        async fn list_and_watch(
            &self,
            request: tonic::Request<super::Empty>,
        ) -> std::result::Result<tonic::Response<Self::ListAndWatchStream>, tonic::Status>;
        /// Allocate is called during container creation so that the Device
        /// Plugin can run device specific operations and instruct Kubelet
        /// of the steps to make the Device available in the container
        async fn allocate(
            &self,
            request: tonic::Request<super::AllocateRequest>,
        ) -> std::result::Result<tonic::Response<super::AllocateResponse>, tonic::Status>;
        /// PreStartContainer is called, if indicated by Device Plugin during registeration phase,
        /// before each container start. Device plugin can run device specific operations
        /// such as reseting the device before making devices available to the container
        async fn pre_start_container(
            &self,
            request: tonic::Request<super::PreStartContainerRequest>,
        ) -> std::result::Result<tonic::Response<super::PreStartContainerResponse>, tonic::Status>;
    }
    /// DevicePlugin is the service advertised by Device Plugins
    #[derive(Debug)]
    pub struct DevicePluginServer<T: DevicePlugin> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: DevicePlugin> DevicePluginServer<T> {
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
    impl<T, B> tonic::codegen::Service<http::Request<B>> for DevicePluginServer<T>
    where
        T: DevicePlugin,
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
                "/v1beta1.DevicePlugin/GetDevicePluginOptions" => {
                    #[allow(non_camel_case_types)]
                    struct GetDevicePluginOptionsSvc<T: DevicePlugin>(pub Arc<T>);
                    impl<T: DevicePlugin> tonic::server::UnaryService<super::Empty> for GetDevicePluginOptionsSvc<T> {
                        type Response = super::DevicePluginOptions;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(&mut self, request: tonic::Request<super::Empty>) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as DevicePlugin>::get_device_plugin_options(&inner, request)
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
                        let method = GetDevicePluginOptionsSvc(inner);
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
                "/v1beta1.DevicePlugin/ListAndWatch" => {
                    #[allow(non_camel_case_types)]
                    struct ListAndWatchSvc<T: DevicePlugin>(pub Arc<T>);
                    impl<T: DevicePlugin> tonic::server::ServerStreamingService<super::Empty> for ListAndWatchSvc<T> {
                        type Response = super::ListAndWatchResponse;
                        type ResponseStream = T::ListAndWatchStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(&mut self, request: tonic::Request<super::Empty>) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as DevicePlugin>::list_and_watch(&inner, request).await
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
                        let method = ListAndWatchSvc(inner);
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
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/v1beta1.DevicePlugin/Allocate" => {
                    #[allow(non_camel_case_types)]
                    struct AllocateSvc<T: DevicePlugin>(pub Arc<T>);
                    impl<T: DevicePlugin> tonic::server::UnaryService<super::AllocateRequest> for AllocateSvc<T> {
                        type Response = super::AllocateResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::AllocateRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut =
                                async move { <T as DevicePlugin>::allocate(&inner, request).await };
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
                        let method = AllocateSvc(inner);
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
                "/v1beta1.DevicePlugin/PreStartContainer" => {
                    #[allow(non_camel_case_types)]
                    struct PreStartContainerSvc<T: DevicePlugin>(pub Arc<T>);
                    impl<T: DevicePlugin>
                        tonic::server::UnaryService<super::PreStartContainerRequest>
                        for PreStartContainerSvc<T>
                    {
                        type Response = super::PreStartContainerResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::PreStartContainerRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as DevicePlugin>::pre_start_container(&inner, request).await
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
                        let method = PreStartContainerSvc(inner);
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
    impl<T: DevicePlugin> Clone for DevicePluginServer<T> {
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
    impl<T: DevicePlugin> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(Arc::clone(&self.0))
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: DevicePlugin> tonic::server::NamedService for DevicePluginServer<T> {
        const NAME: &'static str = "v1beta1.DevicePlugin";
    }
}
