use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use std::path::Path as FilePath;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

pub async fn record(
    State(state): State<Arc<AppState>>,
    Path((device_id, date, name)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response<Body> {
    let path = match state.media.record_path(&device_id, &date, &name) {
        Ok(path) => path,
        Err(error) => return text_response(StatusCode::BAD_REQUEST, &error.to_string()),
    };
    serve_file(
        &path,
        headers.get(RANGE).and_then(|value| value.to_str().ok()),
    )
    .await
}

async fn serve_file(path: &FilePath, range: Option<&str>) -> Response<Body> {
    let mut file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return text_response(StatusCode::NOT_FOUND, "not found");
        }
        Err(error) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("open record: {error}"),
            );
        }
    };
    let size = match file.metadata().await {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("record metadata: {error}"),
            );
        }
    };
    let parsed = range.map(|value| parse_range(value, size));
    if parsed.is_some_and(|value| value.is_none()) {
        let mut response = text_response(StatusCode::RANGE_NOT_SATISFIABLE, "invalid range");
        response.headers_mut().insert(
            CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes */{size}")).unwrap(),
        );
        return response;
    }
    let (first, last, status) = parsed
        .flatten()
        .map(|(first, last)| (first, last, StatusCode::PARTIAL_CONTENT))
        .unwrap_or((0, size.saturating_sub(1), StatusCode::OK));
    let length = if size == 0 { 0 } else { last - first + 1 };
    if let Err(error) = file.seek(std::io::SeekFrom::Start(first)).await {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("seek record: {error}"),
        );
    }
    let stream = ReaderStream::new(file.take(length));
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(if path.extension().is_some_and(|value| value == "idx") {
            "application/json"
        } else {
            "video/mp4"
        }),
    );
    if status == StatusCode::PARTIAL_CONTENT {
        response.headers_mut().insert(
            CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {first}-{last}/{size}")).unwrap(),
        );
    }
    response
}

fn parse_range(value: &str, size: u64) -> Option<(u64, u64)> {
    let value = value.trim().strip_prefix("bytes=")?;
    if value.contains(',') || size == 0 {
        return None;
    }
    let (first, last) = value.split_once('-')?;
    let first = first.parse::<u64>().ok()?;
    let last = if last.is_empty() {
        size - 1
    } else {
        last.parse::<u64>().ok()?.min(size - 1)
    };
    (first <= last && first < size).then_some((first, last))
}

fn text_response(status: StatusCode, message: &str) -> Response<Body> {
    (status, message.to_owned()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_byte_range() {
        assert_eq!(parse_range("bytes=10-19", 100), Some((10, 19)));
        assert_eq!(parse_range("bytes=90-", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
    }
}
