// This file is @generated by prost-build.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AllocatableResourcesRequest {}
/// AllocatableResourcesResponses contains informations about all the devices known by the kubelet
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AllocatableResourcesResponse {
    #[prost(message, repeated, tag = "1")]
    pub devices: ::prost::alloc::vec::Vec<ContainerDevices>,
    #[prost(int64, repeated, tag = "2")]
    pub cpu_ids: ::prost::alloc::vec::Vec<i64>,
    #[prost(message, repeated, tag = "3")]
    pub memory: ::prost::alloc::vec::Vec<ContainerMemory>,
}
/// ListPodResourcesRequest is the request made to the PodResourcesLister service
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListPodResourcesRequest {}
/// ListPodResourcesResponse is the response returned by List function
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListPodResourcesResponse {
    #[prost(message, repeated, tag = "1")]
    pub pod_resources: ::prost::alloc::vec::Vec<PodResources>,
}
/// PodResources contains information about the node resources assigned to a pod
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PodResources {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub namespace: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "3")]
    pub containers: ::prost::alloc::vec::Vec<ContainerResources>,
}
/// ContainerResources contains information about the resources assigned to a container
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContainerResources {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "2")]
    pub devices: ::prost::alloc::vec::Vec<ContainerDevices>,
    #[prost(int64, repeated, tag = "3")]
    pub cpu_ids: ::prost::alloc::vec::Vec<i64>,
    #[prost(message, repeated, tag = "4")]
    pub memory: ::prost::alloc::vec::Vec<ContainerMemory>,
}
/// ContainerMemory contains information about memory and hugepages assigned to a container
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContainerMemory {
    #[prost(string, tag = "1")]
    pub memory_type: ::prost::alloc::string::String,
    #[prost(uint64, tag = "2")]
    pub size: u64,
    #[prost(message, optional, tag = "3")]
    pub topology: ::core::option::Option<TopologyInfo>,
}
/// ContainerDevices contains information about the devices assigned to a container
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContainerDevices {
    #[prost(string, tag = "1")]
    pub resource_name: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "2")]
    pub device_ids: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "3")]
    pub topology: ::core::option::Option<TopologyInfo>,
}
/// Topology describes hardware topology of the resource
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TopologyInfo {
    #[prost(message, repeated, tag = "1")]
    pub nodes: ::prost::alloc::vec::Vec<NumaNode>,
}
/// NUMA representation of NUMA node
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NumaNode {
    #[prost(int64, tag = "1")]
    pub id: i64,
}
/// Generated client implementations.
pub mod pod_resources_lister_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    /// PodResourcesLister is a service provided by the kubelet that provides information about the
    /// node resources consumed by pods and containers on the node
    #[derive(Debug, Clone)]
    pub struct PodResourcesListerClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl PodResourcesListerClient<tonic::transport::Channel> {
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
    impl<T> PodResourcesListerClient<T>
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
        ) -> PodResourcesListerClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            PodResourcesListerClient::new(InterceptedService::new(inner, interceptor))
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
        pub async fn list(
            &mut self,
            request: impl tonic::IntoRequest<super::ListPodResourcesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListPodResourcesResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/v1.PodResourcesLister/List",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("v1.PodResourcesLister", "List"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn get_allocatable_resources(
            &mut self,
            request: impl tonic::IntoRequest<super::AllocatableResourcesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::AllocatableResourcesResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/v1.PodResourcesLister/GetAllocatableResources",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("v1.PodResourcesLister", "GetAllocatableResources"),
                );
            self.inner.unary(req, path, codec).await
        }
    }
}
/// Generated server implementations.
pub mod pod_resources_lister_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with PodResourcesListerServer.
    #[async_trait]
    pub trait PodResourcesLister: Send + Sync + 'static {
        async fn list(
            &self,
            request: tonic::Request<super::ListPodResourcesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListPodResourcesResponse>,
            tonic::Status,
        >;
        async fn get_allocatable_resources(
            &self,
            request: tonic::Request<super::AllocatableResourcesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::AllocatableResourcesResponse>,
            tonic::Status,
        >;
    }
    /// PodResourcesLister is a service provided by the kubelet that provides information about the
    /// node resources consumed by pods and containers on the node
    #[derive(Debug)]
    pub struct PodResourcesListerServer<T: PodResourcesLister> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: PodResourcesLister> PodResourcesListerServer<T> {
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
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
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
    impl<T, B> tonic::codegen::Service<http::Request<B>> for PodResourcesListerServer<T>
    where
        T: PodResourcesLister,
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
                "/v1.PodResourcesLister/List" => {
                    #[allow(non_camel_case_types)]
                    struct ListSvc<T: PodResourcesLister>(pub Arc<T>);
                    impl<
                        T: PodResourcesLister,
                    > tonic::server::UnaryService<super::ListPodResourcesRequest>
                    for ListSvc<T> {
                        type Response = super::ListPodResourcesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::ListPodResourcesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as PodResourcesLister>::list(&inner, request).await
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
                        let method = ListSvc(inner);
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
                "/v1.PodResourcesLister/GetAllocatableResources" => {
                    #[allow(non_camel_case_types)]
                    struct GetAllocatableResourcesSvc<T: PodResourcesLister>(pub Arc<T>);
                    impl<
                        T: PodResourcesLister,
                    > tonic::server::UnaryService<super::AllocatableResourcesRequest>
                    for GetAllocatableResourcesSvc<T> {
                        type Response = super::AllocatableResourcesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::AllocatableResourcesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as PodResourcesLister>::get_allocatable_resources(
                                        &inner,
                                        request,
                                    )
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
                        let method = GetAllocatableResourcesSvc(inner);
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
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: PodResourcesLister> Clone for PodResourcesListerServer<T> {
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
    impl<T: PodResourcesLister> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(Arc::clone(&self.0))
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: PodResourcesLister> tonic::server::NamedService
    for PodResourcesListerServer<T> {
        const NAME: &'static str = "v1.PodResourcesLister";
    }
}
