use axum::{
    body::Body,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

use super::active_trace_id;

/// Respuesta HTTP binaria/descargable directa (CSV, XLSX, PDF, ZPL, JSON crudo) fuera del sobre
/// JSON, con sanitización de nombre de archivo e inyección automática de `x-trace-id`.
#[derive(Debug, Clone)]
pub struct FileResponse {
    bytes: Vec<u8>,
    content_type: &'static str,
    disposition: &'static str,
    filename: String,
}

impl FileResponse {
    pub const CSV: &'static str = "text/csv; charset=utf-8";
    pub const XLSX: &'static str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
    pub const PDF: &'static str = "application/pdf";
    pub const JSON: &'static str = "application/json";
    pub const TEXT: &'static str = "text/plain; charset=utf-8";
    pub const ZPL: &'static str = "x-application/zpl";
    pub const OCTET_STREAM: &'static str = "application/octet-stream";

    pub fn attachment(
        filename: impl AsRef<str>,
        content_type: &'static str,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            content_type,
            disposition: "attachment",
            filename: sanitize_filename(filename.as_ref()),
        }
    }

    pub fn inline(
        filename: impl AsRef<str>,
        content_type: &'static str,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            content_type,
            disposition: "inline",
            filename: sanitize_filename(filename.as_ref()),
        }
    }
}

fn sanitize_filename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|ch| !ch.is_control() && !matches!(ch, '"' | '\\' | '/' | ';'))
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() { "download".to_string() } else { trimmed.to_string() }
}

impl IntoResponse for FileResponse {
    fn into_response(self) -> Response {
        let mut response = Response::new(Body::from(self.bytes));
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(self.content_type));

        let disposition = format!("{}; filename=\"{}\"", self.disposition, self.filename);
        if let Ok(value) = HeaderValue::from_str(&disposition) {
            response.headers_mut().insert(header::CONTENT_DISPOSITION, value);
        }

        if let Some(trace_id) = active_trace_id()
            && let Ok(trace_val) = HeaderValue::from_str(&trace_id)
        {
            response.headers_mut().insert("x-trace-id", trace_val);
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn file_response_sanitizes_dangerous_filenames() {
        let res = FileResponse::attachment(
            "rep\"orte\r\n/../mal.csv",
            FileResponse::CSV,
            b"a,b".to_vec(),
        )
        .into_response();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get(header::CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()),
            Some("attachment; filename=\"reporte..mal.csv\"")
        );
    }
}
