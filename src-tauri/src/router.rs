//! Tier selection. Phase 1 always routes to the configured Firefly home-base
//! endpoint; the full tier table (on-device / cloud, reachability checks) is
//! implemented in Phase 2.

/// Normalize the configured Firefly endpoint into a base URL with a scheme.
pub fn resolve_endpoint(firefly_endpoint: &str) -> String {
    let e = firefly_endpoint.trim();
    if e.starts_with("http://") || e.starts_with("https://") {
        e.to_string()
    } else {
        format!("http://{e}")
    }
}
