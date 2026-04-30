//! S3 cross-region bucket redirect interceptor.
//!
//! Handles 301 PermanentRedirect errors by extracting the correct region
//! from the `x-amz-bucket-region` response header and retrying the request
//! with updated endpoint parameters. Matches Python botocore's
//! `S3RegionRedirectorv2` behavior.
//!
//! TODO: Add integration test using an extended WireMockServer that supports
//! response headers. Requires upstream change to `aws-smithy-http-client`'s
//! `ReplayedEvent` to carry headers (new variant on the `#[non_exhaustive]`
//! enum). Test should verify: request #1 goes to us-east-1 endpoint, gets
//! 301 with `x-amz-bucket-region: us-west-2`, request #2 goes to us-west-2
//! endpoint and succeeds. Assert on the recorded request URIs.

use aws_sdk_s3::config::endpoint::Params as S3Params;
use aws_smithy_runtime_api::client::endpoint::EndpointResolverParams;
use aws_smithy_runtime_api::client::interceptors::context::{
    Error, FinalizerInterceptorContextMut, Input, Output,
};
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::retries::classifiers::{
    ClassifyRetry, RetryAction, RetryReason, SharedRetryClassifier,
};
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::ConfigBag;
use aws_smithy_types::retry::ErrorKind;

/// Interceptor that detects S3 region redirects and rewrites endpoint
/// parameters for the retry attempt.
#[derive(Debug, Default)]
pub struct RegionRedirectInterceptor;

impl RegionRedirectInterceptor {
    pub fn new() -> Self {
        Self
    }
}

impl Intercept for RegionRedirectInterceptor {
    fn name(&self) -> &'static str {
        "S3RegionRedirectInterceptor"
    }

    fn modify_before_attempt_completion(
        &self,
        context: &mut FinalizerInterceptorContextMut<'_, Input, Output, Error>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), aws_smithy_runtime_api::box_error::BoxError> {
        let Some(response) = context.response() else {
            return Ok(());
        };

        let status = response.status().as_u16();
        if status != 301 && status != 307 && status != 400 {
            return Ok(());
        }

        let Some(new_region) = response
            .headers()
            .get("x-amz-bucket-region")
            .map(|s| s.to_string())
        else {
            return Ok(());
        };

        // Get existing params to rebuild with new region.
        let Some(params) = cfg
            .load::<EndpointResolverParams>()
            .and_then(|p| p.get::<S3Params>())
        else {
            return Ok(());
        };

        tracing::debug!(
            bucket = params.bucket().unwrap_or(""),
            new_region = new_region.as_str(),
            "redirecting to correct bucket region"
        );

        // Rebuild params with the correct region.
        let new_params = rebuild_params_with_region(params, &new_region)?;
        cfg.interceptor_state()
            .store_put(EndpointResolverParams::new(new_params));

        Ok(())
    }
}

/// Retry classifier that signals immediate retry on S3 region redirects.
#[derive(Debug)]
pub struct RegionRedirectClassifier;

impl ClassifyRetry for RegionRedirectClassifier {
    fn name(&self) -> &'static str {
        "S3RegionRedirectClassifier"
    }

    fn classify_retry(
        &self,
        ctx: &aws_smithy_runtime_api::client::interceptors::context::InterceptorContext<
            Input,
            Output,
            Error,
        >,
    ) -> RetryAction {
        let Some(response) = ctx.response() else {
            return RetryAction::NoActionIndicated;
        };

        let status = response.status().as_u16();
        let has_region_header = response.headers().get("x-amz-bucket-region").is_some();

        if (status == 301 || status == 307) && has_region_header {
            // Immediate retry — the interceptor already rewrote the params.
            RetryAction::RetryIndicated(RetryReason::RetryableError {
                kind: ErrorKind::ServerError,
                retry_after: None,
            })
        } else {
            RetryAction::NoActionIndicated
        }
    }
}

/// Create a `SharedRetryClassifier` for region redirect handling.
pub fn region_redirect_classifier() -> SharedRetryClassifier {
    SharedRetryClassifier::new(RegionRedirectClassifier)
}

/// Rebuild S3 endpoint params with a new region, copying all other fields.
fn rebuild_params_with_region(
    old: &S3Params,
    new_region: &str,
) -> Result<S3Params, aws_sdk_s3::config::endpoint::InvalidParams> {
    S3Params::builder()
        .set_region(Some(new_region.to_string()))
        .set_bucket(old.bucket().map(|s| s.to_owned()))
        .set_use_fips(old.use_fips())
        .set_use_dual_stack(old.use_dual_stack())
        .set_endpoint(old.endpoint().map(|s| s.to_owned()))
        .set_force_path_style(old.force_path_style())
        .set_accelerate(old.accelerate())
        .set_use_global_endpoint(old.use_global_endpoint())
        .set_use_object_lambda_endpoint(old.use_object_lambda_endpoint())
        .set_key(old.key().map(|s| s.to_owned()))
        .set_prefix(old.prefix().map(|s| s.to_owned()))
        .set_copy_source(old.copy_source().map(|s| s.to_owned()))
        .set_disable_access_points(old.disable_access_points())
        .set_disable_multi_region_access_points(old.disable_multi_region_access_points())
        .set_use_arn_region(old.use_arn_region())
        .set_use_s3_express_control_endpoint(old.use_s3_express_control_endpoint())
        .set_disable_s3_express_session_auth(old.disable_s3_express_session_auth())
        .build()
}
