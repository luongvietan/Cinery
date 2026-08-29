use super::adapter::GenerationProvider;
use super::dry_run::DryRunProvider;
use super::error::{ProviderError, ProviderErrorKind};
use super::fake_async::FakeAsyncVideoProvider;
use super::mock::MockImageProvider;
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct ProviderRegistry {
    providers: BTreeMap<String, Arc<dyn GenerationProvider>>,
}

impl ProviderRegistry {
    pub fn builtin() -> Self {
        let mut registry = Self {
            providers: BTreeMap::new(),
        };
        registry.register(DryRunProvider);
        registry.register(MockImageProvider::default());
        registry.register(FakeAsyncVideoProvider::default());
        // Register OpenAI for discovery/capability queries. The execution
        // path replaces this placeholder with a credential-backed adapter.
        registry.register_arc(super::service::ProviderService::openai_builtin_adapter(
            String::new(),
        ));
        registry
    }

    pub fn register<P: GenerationProvider + 'static>(&mut self, provider: P) {
        self.providers
            .insert(provider.id().into(), Arc::new(provider));
    }

    pub fn register_arc(&mut self, provider: Arc<dyn GenerationProvider>) {
        self.providers.insert(provider.id().into(), provider);
    }

    pub fn get(&self, provider_id: &str) -> Result<Arc<dyn GenerationProvider>, ProviderError> {
        self.providers.get(provider_id).cloned().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::UnknownProviderError,
                format!("provider {provider_id} is not registered"),
            )
        })
    }

    pub fn ids(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::fake_async::FAKE_ASYNC_VIDEO_PROVIDER_ID;

    #[test]
    fn builtin_registry_resolves_only_explicit_provider_ids() {
        let registry = ProviderRegistry::builtin();
        assert_eq!(
            registry.ids(),
            vec!["dry_run", "fake_async_video", "mock", "openai"]
        );
        assert_eq!(registry.get("mock").unwrap().id(), "mock");
        assert_eq!(registry.get("openai").unwrap().id(), "openai");
        assert_eq!(
            registry.get(FAKE_ASYNC_VIDEO_PROVIDER_ID).unwrap().id(),
            "fake_async_video"
        );
        let error = match registry.get("missing") {
            Ok(_) => panic!("missing provider should not resolve"),
            Err(error) => error,
        };
        assert_eq!(error.kind, ProviderErrorKind::UnknownProviderError);
    }
}
