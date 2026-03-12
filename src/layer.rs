use crate::guard::RequestGuard;
use crate::service::RequestGuardService;
use std::sync::Arc;
use tower_layer::Layer;

/// Tower Layer that applies request validation.
#[derive(Clone)]
pub struct RequestGuardLayer {
    pub(crate) guard: Arc<RequestGuard>,
}

impl RequestGuardLayer {
    pub fn new(guard: RequestGuard) -> Self {
        Self {
            guard: Arc::new(guard),
        }
    }
}

impl<S> Layer<S> for RequestGuardLayer {
    type Service = RequestGuardService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestGuardService {
            inner,
            guard: self.guard.clone(),
        }
    }
}
