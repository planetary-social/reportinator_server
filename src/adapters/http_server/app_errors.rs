use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

enum AppErrorKind {
    General(anyhow::Error),
    MissingResponseUrl,
    ActionError,
}

pub struct AppError {
    kind: AppErrorKind,
}

impl AppError {
    fn new(kind: AppErrorKind) -> Self {
        Self { kind }
    }

    pub fn missing_response_url() -> Self {
        Self::new(AppErrorKind::MissingResponseUrl)
    }

    pub fn action_error() -> Self {
        Self::new(AppErrorKind::ActionError)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self.kind {
            AppErrorKind::General(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Something went wrong: {}", err),
            )
                .into_response(),
            AppErrorKind::MissingResponseUrl => {
                (StatusCode::BAD_REQUEST, "Missing response URL.".to_string()).into_response()
            }
            AppErrorKind::ActionError => (
                StatusCode::BAD_REQUEST,
                "Action error: missing actions or values.".to_string(),
            )
                .into_response(),
        }
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self {
            kind: AppErrorKind::General(err.into()),
        }
    }
}
