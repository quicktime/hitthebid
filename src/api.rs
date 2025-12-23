use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::supabase::{SessionRow, SignalQuery, SignalRow};
use crate::types::AppState;

/// Response for signals list
#[derive(Serialize)]
pub struct SignalsResponse {
    pub signals: Vec<SignalRow>,
    pub total: u32,
}

/// Response for sessions list
#[derive(Serialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionRow>,
}

/// Query params for signals endpoint
#[derive(Debug, Deserialize)]
pub struct SignalsQueryParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub signal_type: Option<String>,
    pub direction: Option<String>,
    pub outcome: Option<String>,
}

/// Query params for sessions endpoint
#[derive(Debug, Deserialize)]
pub struct SessionsQueryParams {
    pub limit: Option<u32>,
}

/// GET /api/signals - List signals with filtering
pub async fn get_signals(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SignalsQueryParams>,
) -> impl IntoResponse {
    let Some(ref supabase) = state.supabase else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Supabase not configured"})),
        );
    };

    let query = SignalQuery {
        limit: params.limit,
        offset: params.offset,
        signal_type: params.signal_type.clone(),
        direction: params.direction.clone(),
        outcome: params.outcome.clone(),
    };

    // Get signals and count in parallel
    let (signals_result, count_result) = tokio::join!(
        supabase.query_signals(&query),
        supabase.count_signals(&query)
    );

    match (signals_result, count_result) {
        (Ok(signals), Ok(total)) => (
            StatusCode::OK,
            Json(serde_json::json!(SignalsResponse { signals, total })),
        ),
        (Err(e), _) | (_, Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// GET /api/sessions - List sessions
pub async fn get_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SessionsQueryParams>,
) -> impl IntoResponse {
    let Some(ref supabase) = state.supabase else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Supabase not configured"})),
        );
    };

    let limit = params.limit.unwrap_or(20);

    match supabase.query_sessions(limit).await {
        Ok(sessions) => (
            StatusCode::OK,
            Json(serde_json::json!(SessionsResponse { sessions })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// GET /api/stats - Aggregate stats
pub async fn get_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let Some(ref supabase) = state.supabase else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Supabase not configured"})),
        );
    };

    match supabase.get_aggregate_stats().await {
        Ok(stats) => (StatusCode::OK, Json(serde_json::json!(stats))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}
