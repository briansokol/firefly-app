//! Tier selection. Phase 1 always routes to the configured Firefly home-base
//! endpoint; the full tier table (on-device / cloud, reachability checks) is
//! implemented in Phase 2.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskHint {
    Quick,
    CodeComplete,
    Write,
    ExplainFile,
    Agentic,
    Private,
    Best,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    OnDevice,
    HomeBase,
    Cloud,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Kid,
    Adult,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::OnDevice => "on-device",
            Tier::HomeBase => "home-base",
            Tier::Cloud => "cloud",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub tier: Tier,
    pub endpoint: String,
    pub model: String,
    pub use_token: bool,
    pub degraded: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RouteError {
    Refused(String),
    NotConfigured(String),
}

pub struct RouteInputs<'a> {
    pub firefly_endpoint: &'a str,
    pub on_device_endpoint: &'a str,
    pub on_device_model: &'a str,
    pub model_code: &'a str,
    pub model_chat_heavy: &'a str,
    pub model_frontier: &'a str,
    pub firefly_reachable: bool,
    pub profile: Profile,
}

/// Map a task hint + settings + live reachability to a concrete route per
/// PLAN-app-build.md §6. Pure: no IO, fully unit-tested.
pub fn resolve_route(task: TaskHint, i: &RouteInputs) -> Result<Route, RouteError> {
    let on_device = |degraded: bool| Route {
        tier: Tier::OnDevice,
        endpoint: resolve_endpoint(i.on_device_endpoint),
        model: i.on_device_model.to_string(),
        use_token: false,
        degraded,
    };
    let home_base = |model: &str| Route {
        tier: Tier::HomeBase,
        endpoint: resolve_endpoint(i.firefly_endpoint),
        model: model.to_string(),
        use_token: true,
        degraded: false,
    };

    let kid_refused = |what: &str| {
        RouteError::Refused(format!(
            "{what} is not available for a kid profile; ask an adult"
        ))
    };
    let is_kid = i.profile == Profile::Kid;

    match task {
        // Privacy-critical: on-device only, always. Enforced here, not just in UI.
        TaskHint::Private => Ok(on_device(false)),
        // One-liners stay local regardless of reachability.
        TaskHint::Quick => Ok(on_device(false)),
        TaskHint::CodeComplete => {
            if !i.on_device_endpoint.trim().is_empty() {
                Ok(on_device(false))
            } else if is_kid {
                Err(kid_refused("code"))
            } else if i.firefly_reachable {
                Ok(home_base(i.model_code))
            } else {
                Err(RouteError::NotConfigured(
                    "code-complete needs an on-device endpoint, and Firefly is unreachable".into(),
                ))
            }
        }
        TaskHint::Write | TaskHint::ExplainFile => {
            if i.firefly_reachable {
                if is_kid {
                    Err(kid_refused("code"))
                } else {
                    Ok(home_base(i.model_code))
                }
            } else {
                Ok(on_device(true))
            }
        }
        TaskHint::Agentic => {
            if i.firefly_reachable {
                Ok(home_base(i.model_chat_heavy))
            } else {
                Ok(on_device(true))
            }
        }
        TaskHint::Best => {
            if is_kid {
                Err(kid_refused("frontier"))
            } else if i.firefly_reachable {
                Ok(Route {
                    tier: Tier::Cloud,
                    endpoint: resolve_endpoint(i.firefly_endpoint),
                    model: i.model_frontier.to_string(),
                    use_token: true,
                    degraded: false,
                })
            } else {
                Err(RouteError::Refused(
                    "best/frontier requires Firefly (cloud via home-base); it is unreachable".into(),
                ))
            }
        }
    }
}

/// Pure: is a cached reachability result still within its TTL?
pub fn is_fresh(checked_at: Option<Instant>, now: Instant, ttl: Duration) -> bool {
    match checked_at {
        Some(t) => now.duration_since(t) < ttl,
        None => false,
    }
}

/// Probe Firefly with a short timeout. Any HTTP response (even 401) means
/// reachable; only a connection/timeout failure means unreachable.
pub async fn check_reachable(firefly_endpoint: &str) -> bool {
    let url = format!("{}/health", resolve_endpoint(firefly_endpoint).trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.get(&url).send().await.is_ok()
}

/// Normalize the configured Firefly endpoint into a base URL with a scheme.
pub fn resolve_endpoint(firefly_endpoint: &str) -> String {
    let e = firefly_endpoint.trim();
    if e.starts_with("http://") || e.starts_with("https://") {
        e.to_string()
    } else {
        format!("http://{e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn cache_is_stale_when_never_checked() {
        let now = Instant::now();
        assert!(!is_fresh(None, now, Duration::from_secs(5)));
    }

    #[test]
    fn cache_is_fresh_within_ttl() {
        let t0 = Instant::now();
        assert!(is_fresh(Some(t0), t0 + Duration::from_secs(2), Duration::from_secs(5)));
    }

    #[test]
    fn cache_is_stale_past_ttl() {
        let t0 = Instant::now();
        assert!(!is_fresh(Some(t0), t0 + Duration::from_secs(6), Duration::from_secs(5)));
    }

    fn inputs(reachable: bool) -> RouteInputs<'static> {
        RouteInputs {
            firefly_endpoint: "firefly.taild9c345.ts.net:4000",
            on_device_endpoint: "http://localhost:11434",
            on_device_model: "qwen3.6:27b",
            model_code: "code",
            model_chat_heavy: "chat-heavy",
            model_frontier: "frontier",
            firefly_reachable: reachable,
            profile: Profile::Adult,
        }
    }

    #[test]
    fn private_stays_on_device_even_when_firefly_up() {
        let r = resolve_route(TaskHint::Private, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::OnDevice);
        assert_eq!(r.endpoint, "http://localhost:11434");
        assert_eq!(r.model, "qwen3.6:27b");
        assert!(!r.use_token);
        assert!(!r.degraded);
    }

    #[test]
    fn quick_is_always_on_device() {
        assert_eq!(resolve_route(TaskHint::Quick, &inputs(true)).unwrap().tier, Tier::OnDevice);
        assert_eq!(resolve_route(TaskHint::Quick, &inputs(false)).unwrap().tier, Tier::OnDevice);
    }

    #[test]
    fn agentic_hits_home_base_when_up() {
        let r = resolve_route(TaskHint::Agentic, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::HomeBase);
        assert_eq!(r.endpoint, "http://firefly.taild9c345.ts.net:4000");
        assert_eq!(r.model, "chat-heavy");
        assert!(r.use_token);
        assert!(!r.degraded);
    }

    #[test]
    fn agentic_degrades_to_on_device_when_down() {
        let r = resolve_route(TaskHint::Agentic, &inputs(false)).unwrap();
        assert_eq!(r.tier, Tier::OnDevice);
        assert_eq!(r.model, "qwen3.6:27b");
        assert!(!r.use_token);
        assert!(r.degraded);
    }

    #[test]
    fn write_uses_code_model_on_home_base() {
        let r = resolve_route(TaskHint::Write, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::HomeBase);
        assert_eq!(r.model, "code");
    }

    #[test]
    fn explain_file_degrades_when_down() {
        let r = resolve_route(TaskHint::ExplainFile, &inputs(false)).unwrap();
        assert_eq!(r.tier, Tier::OnDevice);
        assert!(r.degraded);
    }

    #[test]
    fn code_complete_prefers_on_device_when_configured() {
        let r = resolve_route(TaskHint::CodeComplete, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::OnDevice);
        assert!(!r.degraded);
    }

    #[test]
    fn code_complete_uses_home_base_when_no_on_device_and_reachable() {
        let mut i = inputs(true);
        i.on_device_endpoint = "";
        let r = resolve_route(TaskHint::CodeComplete, &i).unwrap();
        assert_eq!(r.tier, Tier::HomeBase);
        assert_eq!(r.model, "code");
    }

    #[test]
    fn code_complete_errors_when_no_on_device_and_unreachable() {
        let mut i = inputs(false);
        i.on_device_endpoint = "";
        assert!(matches!(
            resolve_route(TaskHint::CodeComplete, &i),
            Err(RouteError::NotConfigured(_))
        ));
    }

    #[test]
    fn best_uses_cloud_when_up() {
        let r = resolve_route(TaskHint::Best, &inputs(true)).unwrap();
        assert_eq!(r.tier, Tier::Cloud);
        assert_eq!(r.endpoint, "http://firefly.taild9c345.ts.net:4000");
        assert_eq!(r.model, "frontier");
        assert!(r.use_token);
    }

    #[test]
    fn best_refuses_when_down() {
        assert!(matches!(
            resolve_route(TaskHint::Best, &inputs(false)),
            Err(RouteError::Refused(_))
        ));
    }

    #[test]
    fn kid_is_refused_code_on_home_base() {
        let mut i = inputs(true);
        i.profile = Profile::Kid;
        assert!(matches!(resolve_route(TaskHint::Write, &i), Err(RouteError::Refused(_))));
        assert!(matches!(resolve_route(TaskHint::ExplainFile, &i), Err(RouteError::Refused(_))));
    }

    #[test]
    fn kid_is_refused_frontier() {
        let mut i = inputs(true);
        i.profile = Profile::Kid;
        assert!(matches!(resolve_route(TaskHint::Best, &i), Err(RouteError::Refused(_))));
    }

    #[test]
    fn kid_is_refused_code_complete_home_base_fallback() {
        let mut i = inputs(true);
        i.profile = Profile::Kid;
        i.on_device_endpoint = ""; // forces home-base `code`
        assert!(matches!(resolve_route(TaskHint::CodeComplete, &i), Err(RouteError::Refused(_))));
    }

    #[test]
    fn kid_keeps_agentic_quick_private_and_on_device_code_complete() {
        let mut i = inputs(true);
        i.profile = Profile::Kid;
        assert_eq!(resolve_route(TaskHint::Agentic, &i).unwrap().model, "chat-heavy");
        assert_eq!(resolve_route(TaskHint::Quick, &i).unwrap().tier, Tier::OnDevice);
        assert_eq!(resolve_route(TaskHint::Private, &i).unwrap().tier, Tier::OnDevice);
        // code-complete with an on-device endpoint stays local — allowed for kids
        assert_eq!(resolve_route(TaskHint::CodeComplete, &i).unwrap().tier, Tier::OnDevice);
    }
}
