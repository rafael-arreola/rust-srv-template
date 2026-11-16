use axum::http::{HeaderMap, header};

pub const CONTENT_TYPE_MSGPACK: &str = "application/vnd.msgpack";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyFormat {
    Json,
    MsgPack,
}

tokio::task_local! {
    pub static RESPONSE_FORMAT: BodyFormat;
}

pub fn is_msgpack_content_type(content_type: &str) -> bool {
    let media_type = content_type.split(';').next().unwrap_or_default().trim();

    media_type.eq_ignore_ascii_case(CONTENT_TYPE_MSGPACK)
        || media_type.eq_ignore_ascii_case("application/msgpack")
        || media_type.eq_ignore_ascii_case("application/x-msgpack")
        || media_type
            .split_once('+')
            .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("msgpack"))
}

pub fn wants_msgpack_response(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.split(',').any(is_msgpack_content_type))
}

pub(crate) fn negotiate_format(headers: &HeaderMap) -> BodyFormat {
    if wants_msgpack_response(headers) {
        return BodyFormat::MsgPack;
    }

    let accepts_json_explicitly =
        headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|accept| {
            accept.split(',').any(|entry| {
                let media = entry.split(';').next().unwrap_or_default().trim();
                media.eq_ignore_ascii_case("application/json")
                    || media
                        .split_once('+')
                        .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("json"))
            })
        });

    if accepts_json_explicitly {
        return BodyFormat::Json;
    }

    let request_is_msgpack = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_msgpack_content_type);

    if request_is_msgpack { BodyFormat::MsgPack } else { BodyFormat::Json }
}
