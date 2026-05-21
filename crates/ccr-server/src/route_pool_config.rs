use ccr_types::{Config, Provider, RoutePoolCandidate};

pub fn route_pool_candidates(config: &Config, tried_routes: &[String]) -> Vec<String> {
    let Some(pool) = &config.route_pool else {
        return Vec::new();
    };
    if !pool.enabled {
        return Vec::new();
    }

    let mut candidates = pool.candidates.clone();
    normalize_route_pool_order(&mut candidates);
    let mut routes = Vec::new();
    for candidate in candidates {
        let route = candidate.route.trim();
        if !candidate.enabled || route.is_empty() {
            continue;
        }
        if tried_routes.iter().any(|tried| tried == route) || routes.iter().any(|r| r == route) {
            continue;
        }
        routes.push(route.to_string());
    }
    routes
}

pub fn normalize_route_pool_order(candidates: &mut [RoutePoolCandidate]) {
    candidates.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.route.cmp(&b.route))
    });
}

pub fn route_pool_enabled(config: &Config) -> bool {
    config.route_pool.as_ref().is_some_and(|pool| pool.enabled)
}

pub fn route_pool_failure_threshold(config: &Config) -> u32 {
    config
        .route_pool
        .as_ref()
        .map(|pool| pool.failure_threshold.max(1))
        .unwrap_or(3)
}

pub fn route_pool_ban_seconds(config: &Config) -> u64 {
    config
        .route_pool
        .as_ref()
        .map(|pool| pool.ban_seconds.max(1))
        .unwrap_or(3600)
}

pub fn provider_for_route<'a>(route: &str, config: &'a Config) -> Option<(&'a Provider, String)> {
    let provider = ccr_router::find_provider(route, config)?;
    Some((provider, ccr_router::model_name(route).to_string()))
}

pub fn route_for_display(route: &str) -> String {
    if route.trim().is_empty() {
        "<empty>".to_string()
    } else {
        route.to_string()
    }
}

pub fn provider_names(config: &Config) -> String {
    if config.providers.is_empty() {
        return "<none>".to_string();
    }
    config
        .providers
        .iter()
        .map(|provider| provider.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn provider_not_found_message(route: &str, config: &Config) -> String {
    format!(
        "No provider found for route '{}'. Available providers: {}",
        route_for_display(route),
        provider_names(config)
    )
}
