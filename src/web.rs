use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{Html, IntoResponse, Response};

const INDEX: &str = include_str!("../web/index.html");
const APP: &str = include_str!("../web/app.js");
const FLV_PLAYER: &str = include_str!("../web/generated/flv-player.js");
const MOQ_PLAYER: &str = include_str!("../web/generated/moq-player.js");
const EVALUATION: &str = include_str!("../web/generated/evaluation.js");
const STYLE: &str = include_str!("../web/style.css");
const ICON_SVG: &str = include_str!("../web/favicon.svg");

pub async fn index() -> impl IntoResponse {
    (
        [(CACHE_CONTROL, HeaderValue::from_static("no-cache"))],
        Html(INDEX),
    )
}

pub async fn app() -> Response {
    static_response("application/javascript; charset=utf-8", APP)
}

pub async fn flv_player() -> Response {
    static_response("application/javascript; charset=utf-8", FLV_PLAYER)
}

pub async fn moq_player() -> Response {
    static_response("application/javascript; charset=utf-8", MOQ_PLAYER)
}

pub async fn evaluation() -> Response {
    static_response("application/javascript; charset=utf-8", EVALUATION)
}

pub async fn style() -> Response {
    static_response("text/css; charset=utf-8", STYLE)
}

pub async fn favicon() -> Response {
    static_response("image/svg+xml", ICON_SVG)
}

fn static_response(content_type: &'static str, body: &'static str) -> Response {
    (
        [
            (CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        body,
    )
        .into_response()
}
