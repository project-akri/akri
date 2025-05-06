use std::fmt::Debug;

use async_trait::async_trait;
use either::Either;
use kube::{
    api::{ListParams, Patch, PatchParams},
    core::{ObjectList, ObjectMeta, PartialObjectMetaExt, Status},
    Error, Resource, ResourceExt,
};
use mockall::automock;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[automock]
#[async_trait]
pub trait Api<T: Clone + Send + Sync + Resource>: Send + Sync {
    fn as_inner(&self) -> kube::Api<T>;
    async fn apply(&self, obj: T, field_manager: &str) -> Result<T, Error>;
    async fn raw_patch(
        &self,
        name: &str,
        patch: &Patch<Value>,
        pp: &PatchParams,
    ) -> Result<T, Error>;
    async fn create(&self, obj: &T) -> Result<T, Error>;
    async fn delete(&self, name: &str) -> Result<Either<T, Status>, Error>;
    async fn delete_collection(
        &self,
        lp: &ListParams,
    ) -> Result<Either<ObjectList<T>, Status>, Error>;
    async fn get(&self, name: &str) -> Result<Option<T>, Error>;
    async fn list(&self, lp: &ListParams) -> Result<ObjectList<T>, Error>;
    async fn add_finalizer(&self, obj: &T, finalizer: &str) -> Result<(), Error> {
        self.set_finalizers(
            &obj.name_any(),
            Some(vec![finalizer.to_string()]),
            &format!("{}-fin", finalizer),
        )
        .await
    }
    async fn remove_finalizer(&self, obj: &T, finalizer: &str) -> Result<(), Error> {
        self.set_finalizers(&obj.name_any(), None, &format!("{}-fin", finalizer))
            .await
    }
    async fn set_finalizers(
        &self,
        name: &str,
        finalizers: Option<Vec<String>>,
        field_manager: &str,
    ) -> Result<(), Error>;
}

#[async_trait]
impl<T> Api<T> for kube::Api<T>
where
    T: Clone
        + DeserializeOwned
        + Debug
        + Resource<DynamicType = ()>
        + serde::Serialize
        + Send
        + Sync,
{
    fn as_inner(&self) -> kube::Api<T> {
        self.to_owned()
    }
    async fn apply(&self, obj: T, field_manager: &str) -> Result<T, Error> {
        let name = obj.name_any();
        let pp = PatchParams::apply(field_manager);
        let patch = kube::api::Patch::Apply(obj);
        self.patch(&name, &pp, &patch).await
    }
    async fn raw_patch(
        &self,
        name: &str,
        patch: &Patch<Value>,
        pp: &PatchParams,
    ) -> Result<T, Error> {
        self.patch(name, pp, patch).await
    }
    async fn create(&self, obj: &T) -> Result<T, Error> {
        self.create(&Default::default(), obj).await
    }
    async fn delete(&self, name: &str) -> Result<Either<T, Status>, Error> {
        self.delete(name, &Default::default()).await
    }
    async fn delete_collection(
        &self,
        lp: &ListParams,
    ) -> Result<Either<ObjectList<T>, Status>, Error> {
        self.delete_collection(&Default::default(), lp).await
    }
    async fn get(&self, name: &str) -> Result<Option<T>, Error> {
        self.get_opt(name).await
    }
    async fn list(&self, lp: &ListParams) -> Result<ObjectList<T>, Error> {
        self.list(lp).await
    }
    async fn set_finalizers(
        &self,
        name: &str,
        finalizers: Option<Vec<String>>,
        field_manager: &str,
    ) -> Result<(), Error> {
        let metadata = ObjectMeta {
            finalizers,
            ..Default::default()
        }
        .into_request_partial::<T>();
        self.patch_metadata(
            name,
            &PatchParams::apply(field_manager),
            &Patch::Apply(&metadata),
        )
        .await?;
        Ok(())
    }
}

#[automock]
#[allow(clippy::multiple_bound_locations)]
pub trait IntoApi<T: Resource + 'static + Send + Sync>: Send + Sync {
    fn all(&self) -> Box<dyn Api<T>>;
    fn namespaced(&self, namespace: &str) -> Box<dyn Api<T>>
    where
        T: Resource<Scope = k8s_openapi::NamespaceResourceScope>;
    fn default_namespaced(&self) -> Box<dyn Api<T>>
    where
        T: Resource<Scope = k8s_openapi::NamespaceResourceScope>;
}

impl<T> IntoApi<T> for kube::Client
where
    T: Resource<DynamicType = ()>
        + Clone
        + DeserializeOwned
        + Debug
        + serde::Serialize
        + Send
        + Sync
        + 'static,
{
    fn all(&self) -> Box<dyn Api<T>> {
        Box::new(kube::Api::all(self.clone()))
    }

    fn namespaced(&self, namespace: &str) -> Box<dyn Api<T>>
    where
        T: Resource<Scope = k8s_openapi::NamespaceResourceScope>,
    {
        Box::new(kube::Api::namespaced(self.clone(), namespace))
    }

    fn default_namespaced(&self) -> Box<dyn Api<T>>
    where
        T: Resource<Scope = k8s_openapi::NamespaceResourceScope>,
    {
        Box::new(kube::Api::default_namespaced(self.clone()))
    }
}
