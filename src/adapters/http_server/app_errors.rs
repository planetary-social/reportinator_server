use anyhow::Error;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

#[derive(Debug)]
enum AppErrorKind {
    General(Error),
    // TODO: Let's be more specific later
    SlackParsingError(String),
}

#[derive(Debug)]
pub struct AppError {
    kind: AppErrorKind,
}

impl AppError {
    fn new(kind: AppErrorKind) -> Self {
        Self { kind }
    }

    pub fn slack_parsing_error(context: &str) -> Self {
        Self::new(AppErrorKind::SlackParsingError(context.to_string()))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!("{:?}", self);
        match self.kind {
            AppErrorKind::General(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Something went wrong: {}", err),
            )
                .into_response(),
            AppErrorKind::SlackParsingError(context) => (
                StatusCode::BAD_REQUEST,
                format!("Slack parsing error: {}.", context),
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
