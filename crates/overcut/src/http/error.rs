//! The API error response.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use overcut_application::AppError;
use serde::Serialize;

/// An error the API returns as `{"error": "<message>"}`.
#[derive(Debug)]
pub enum ApiError {
    /// A use case failed.
    App(AppError),
    /// The request was malformed.
    BadRequest(String),
    /// The server failed.
    Internal(String),
}

impl ApiError {
    /// The HTTP status for the error.
    ///
    /// - A client error of the application gives 400.
    /// - A gateway error gives 502.
    /// - Anything else gives 500.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::App(e) if e.is_client_error() => StatusCode::BAD_REQUEST,
            Self::App(AppError::Gateway(_)) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) | Self::App(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The message.
    pub fn message(&self) -> String {
        match self {
            Self::App(e) => e.to_string(),
            Self::BadRequest(m) | Self::Internal(m) => m.clone(),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::Internal(format!("worker failed: {e}"))
    }
}

impl From<axum::extract::rejection::JsonRejection> for ApiError {
    fn from(e: axum::extract::rejection::JsonRejection) -> Self {
        Self::BadRequest(format!("bad request body: {}", e.body_text()))
    }
}

/// The JSON body of an error.
#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() {
            tracing::error!(status = status.as_u16(), error = %self.message(), "request failed");
        }
        (
            status,
            Json(ErrorBody {
                error: self.message(),
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use overcut_application::ports::GatewayError;
    use overcut_domain::DomainError;

    use super::*;

    #[test]
    fn given_a_domain_error_when_mapped_then_the_status_is_400() {
        let e = ApiError::from(AppError::Domain(DomainError::UnknownRound(9)));
        assert_eq!(e.status(), StatusCode::BAD_REQUEST);
        assert!(e.message().contains('9'));
    }

    #[test]
    fn given_a_gateway_error_when_mapped_then_the_status_is_502() {
        let e = ApiError::from(AppError::Gateway(GatewayError::new("feed", "down")));
        assert_eq!(e.status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn given_a_missing_asset_error_when_mapped_then_the_status_is_500() {
        assert_eq!(
            ApiError::from(AppError::NoAssets).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
