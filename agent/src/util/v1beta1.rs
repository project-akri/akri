#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DevicePluginOptions {
    /// Indicates if PreStartContainer call is required before each container start
    #[prost(bool, tag = "1")]
    pub pre_start_required: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RegisterRequest {
    /// Version of the API the Device Plugin was built against
    #[prost(string, tag = "1")]
    pub version: std::string::String,
    /// Name of the unix socket the device plugin is listening on
    /// PATH = path.Join(DevicePluginPath, endpoint)
    #[prost(string, tag = "2")]
    pub endpoint: std::string::String,
    /// Schedulable resource name. As of now it's expected to be a DNS Label
    #[prost(string, tag = "3")]
    pub resource_name: std::string::String,
    /// Options to be communicated with Device Manager
    #[prost(message, optional, tag = "4")]
    pub options: ::std::option::Option<DevicePluginOptions>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Empty {}
/// ListAndWatch returns a stream of List of Devices
/// Whenever a Device state change or a Device disapears, ListAndWatch
/// returns the new list
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListAndWatchResponse {
    #[prost(message, repeated, tag = "1")]
    pub devices: ::std::vec::Vec<Device>,
}
/// E.g:
/// struct Device {
///    ID: "GPU-fef8089b-4820-abfc-e83e-94318197576e",
///    State: "Healthy",
///}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Device {
    /// A unique ID assigned by the device plugin used
    /// to identify devices during the communication
    /// Max length of this field is 63 characters
    #[prost(string, tag = "1")]
    pub id: std::string::String,
    /// Health of the device, can be healthy or unhealthy, see constants.go
    #[prost(string, tag = "2")]
    pub health: std::string::String,
}
/// - PreStartContainer is expected to be called before each container start if indicated by plugin during registration phase.
/// - PreStartContainer allows kubelet to pass reinitialized devices to containers.
/// - PreStartContainer allows Device Plugin to run device specific operations on
///   the Devices requested
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PreStartContainerRequest {
    #[prost(string, repeated, tag = "1")]
    pub devices_i_ds: ::std::vec::Vec<std::string::String>,
}
/// PreStartContainerResponse will be send by plugin in response to PreStartContainerRequest
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PreStartContainerResponse {}
/// - Allocate is expected to be called during pod creation since allocation
///   failures for any container would result in pod startup failure.
/// - Allocate allows kubelet to exposes additional artifacts in a pod's
///   environment as directed by the plugin.
/// - Allocate allows Device Plugin to run device specific operations on
///   the Devices requested
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AllocateRequest {
    #[prost(message, repeated, tag = "1")]
    pub container_requests: ::std::vec::Vec<ContainerAllocateRequest>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContainerAllocateRequest {
    #[prost(string, repeated, tag = "1")]
    pub devices_i_ds: ::std::vec::Vec<std::string::String>,
}
/// AllocateResponse includes the artifacts that needs to be injected into
/// a container for accessing 'deviceIDs' that were mentioned as part of
/// 'AllocateRequest'.
/// Failure Handling:
/// if Kubelet sends an allocation request for dev1 and dev2.
/// Allocation on dev1 succeeds but allocation on dev2 fails.
/// The Device plugin should send a ListAndWatch update and fail the
/// Allocation request
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AllocateResponse {
    #[prost(message, repeated, tag = "1")]
    pub container_responses: ::std::vec::Vec<ContainerAllocateResponse>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContainerAllocateResponse {
    /// List of environment variable to be set in the container to access one of more devices.
    #[prost(map = "string, string", tag = "1")]
    pub envs: ::std::collections::HashMap<std::string::String, std::string::String>,
    /// Mounts for the container.
    #[prost(message, repeated, tag = "2")]
    pub mounts: ::std::vec::Vec<Mount>,
    /// Devices for the container.
    #[prost(message, repeated, tag = "3")]
    pub devices: ::std::vec::Vec<DeviceSpec>,
    /// Container annotations to pass to the container runtime
    #[prost(map = "string, string", tag = "4")]
    pub annotations: ::std::collections::HashMap<std::string::String, std::string::String>,
}
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
    #[doc = " Registration is the service advertised by the Kubelet"]
    #[doc = " Only when Kubelet answers with a success code to a Register Request"]
    #[doc = " may Device Plugins start their service"]
    #[doc = " Registration may fail when device plugin version is not supported by"]
    #[doc = " Kubelet or the registered resourceName is already taken by another"]
    #[doc = " active device plugin. Device plugin is expected to terminate upon registration failure"]
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
        pub async fn register(
            &mut self,
            request: impl tonic::IntoRequest<super::RegisterRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/v1beta1.Registration/Register");
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
pub mod device_plugin_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " DevicePlugin is the service advertised by Device Plugins"]
    pub struct DevicePluginClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl DevicePluginClient<tonic::transport::Channel> {
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
    impl<T> DevicePluginClient<T>
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
        #[doc = " GetDevicePluginOptions returns options to be communicated with Device"]
        #[doc = " Manager"]
        pub async fn get_device_plugin_options(
            &mut self,
            request: impl tonic::IntoRequest<super::Empty>,
        ) -> Result<tonic::Response<super::DevicePluginOptions>, tonic::Status> {
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
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ListAndWatch returns a stream of List of Devices"]
        #[doc = " Whenever a Device state change or a Device disapears, ListAndWatch"]
        #[doc = " returns the new list"]
        pub async fn list_and_watch(
            &mut self,
            request: impl tonic::IntoRequest<super::Empty>,
        ) -> Result<
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
            self.inner
                .server_streaming(request.into_request(), path, codec)
                .await
        }
        #[doc = " Allocate is called during container creation so that the Device"]
        #[doc = " Plugin can run device specific operations and instruct Kubelet"]
        #[doc = " of the steps to make the Device available in the container"]
        pub async fn allocate(
            &mut self,
            request: impl tonic::IntoRequest<super::AllocateRequest>,
        ) -> Result<tonic::Response<super::AllocateResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/v1beta1.DevicePlugin/Allocate");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " PreStartContainer is called, if indicated by Device Plugin during registeration phase,"]
        #[doc = " before each container start. Device plugin can run device specific operations"]
        #[doc = " such as reseting the device before making devices available to the container"]
        pub async fn pre_start_container(
            &mut self,
            request: impl tonic::IntoRequest<super::PreStartContainerRequest>,
        ) -> Result<tonic::Response<super::PreStartContainerResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/v1beta1.DevicePlugin/PreStartContainer");
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
    impl<T: Clone> Clone for DevicePluginClient<T> {
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
        async fn register(
            &self,
            request: tonic::Request<super::RegisterRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
    }
    #[doc = " Registration is the service advertised by the Kubelet"]
    #[doc = " Only when Kubelet answers with a success code to a Register Request"]
    #[doc = " may Device Plugins start their service"]
    #[doc = " Registration may fail when device plugin version is not supported by"]
    #[doc = " Kubelet or the registered resourceName is already taken by another"]
    #[doc = " active device plugin. Device plugin is expected to terminate upon registration failure"]
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
                "/v1beta1.Registration/Register" => {
                    struct RegisterSvc<T: Registration>(pub Arc<T>);
                    impl<T: Registration> tonic::server::UnaryService<super::RegisterRequest> for RegisterSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RegisterRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.register(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = RegisterSvc(inner);
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
        const NAME: &'static str = "v1beta1.Registration";
    }
}
#[doc = r" Generated server implementations."]
pub mod device_plugin_server {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with DevicePluginServer."]
    #[async_trait]
    pub trait DevicePlugin: Send + Sync + 'static {
        #[doc = " GetDevicePluginOptions returns options to be communicated with Device"]
        #[doc = " Manager"]
        async fn get_device_plugin_options(
            &self,
            request: tonic::Request<super::Empty>,
        ) -> Result<tonic::Response<super::DevicePluginOptions>, tonic::Status>;
        #[doc = "Server streaming response type for the ListAndWatch method."]
        type ListAndWatchStream: Stream<Item = Result<super::ListAndWatchResponse, tonic::Status>>
            + Send
            + Sync
            + 'static;
        #[doc = " ListAndWatch returns a stream of List of Devices"]
        #[doc = " Whenever a Device state change or a Device disapears, ListAndWatch"]
        #[doc = " returns the new list"]
        async fn list_and_watch(
            &self,
            request: tonic::Request<super::Empty>,
        ) -> Result<tonic::Response<Self::ListAndWatchStream>, tonic::Status>;
        #[doc = " Allocate is called during container creation so that the Device"]
        #[doc = " Plugin can run device specific operations and instruct Kubelet"]
        #[doc = " of the steps to make the Device available in the container"]
        async fn allocate(
            &self,
            request: tonic::Request<super::AllocateRequest>,
        ) -> Result<tonic::Response<super::AllocateResponse>, tonic::Status>;
        #[doc = " PreStartContainer is called, if indicated by Device Plugin during registeration phase,"]
        #[doc = " before each container start. Device plugin can run device specific operations"]
        #[doc = " such as reseting the device before making devices available to the container"]
        async fn pre_start_container(
            &self,
            request: tonic::Request<super::PreStartContainerRequest>,
        ) -> Result<tonic::Response<super::PreStartContainerResponse>, tonic::Status>;
    }
    #[doc = " DevicePlugin is the service advertised by Device Plugins"]
    #[derive(Debug)]
    #[doc(hidden)]
    pub struct DevicePluginServer<T: DevicePlugin> {
        inner: _Inner<T>,
    }
    struct _Inner<T>(Arc<T>, Option<tonic::Interceptor>);
    impl<T: DevicePlugin> DevicePluginServer<T> {
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
    impl<T: DevicePlugin> Service<http::Request<HyperBody>> for DevicePluginServer<T> {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = Never;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<HyperBody>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/v1beta1.DevicePlugin/GetDevicePluginOptions" => {
                    struct GetDevicePluginOptionsSvc<T: DevicePlugin>(pub Arc<T>);
                    impl<T: DevicePlugin> tonic::server::UnaryService<super::Empty> for GetDevicePluginOptionsSvc<T> {
                        type Response = super::DevicePluginOptions;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(&mut self, request: tonic::Request<super::Empty>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_device_plugin_options(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetDevicePluginOptionsSvc(inner);
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
                "/v1beta1.DevicePlugin/ListAndWatch" => {
                    struct ListAndWatchSvc<T: DevicePlugin>(pub Arc<T>);
                    impl<T: DevicePlugin> tonic::server::ServerStreamingService<super::Empty> for ListAndWatchSvc<T> {
                        type Response = super::ListAndWatchResponse;
                        type ResponseStream = T::ListAndWatchStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(&mut self, request: tonic::Request<super::Empty>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.list_and_watch(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1;
                        let inner = inner.0;
                        let method = ListAndWatchSvc(inner);
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
                "/v1beta1.DevicePlugin/Allocate" => {
                    struct AllocateSvc<T: DevicePlugin>(pub Arc<T>);
                    impl<T: DevicePlugin> tonic::server::UnaryService<super::AllocateRequest> for AllocateSvc<T> {
                        type Response = super::AllocateResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::AllocateRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.allocate(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = AllocateSvc(inner);
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
                "/v1beta1.DevicePlugin/PreStartContainer" => {
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
                            let inner = self.0.clone();
                            let fut = async move { inner.pre_start_container(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = PreStartContainerSvc(inner);
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
    impl<T: DevicePlugin> Clone for DevicePluginServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self { inner }
        }
    }
    impl<T: DevicePlugin> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone(), self.1.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: DevicePlugin> tonic::transport::NamedService for DevicePluginServer<T> {
        const NAME: &'static str = "v1beta1.DevicePlugin";
    }
}
