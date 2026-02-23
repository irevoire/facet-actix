use core::fmt;
use std::{
    marker::PhantomData,
    ops,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_web::{
    FromRequest, HttpRequest, HttpResponse, Responder, ResponseError,
    body::EitherBody,
    http::{StatusCode, header::CONTENT_TYPE},
    mime::{self, APPLICATION_JSON},
    web::Bytes,
};
use facet::Facet;
use facet_format::SerializeError;
use facet_json::{DeserializeError, JsonSerializeError};

#[derive(Debug, facet::Facet)]
#[facet(transparent)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Unwrap into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Json<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Debug)]
pub enum JsonRejection {
    /// Failed to read the request body.
    Body(actix_web::Error),
    /// Failed to deserialize the JSON data.
    Deserialize(DeserializeError),
    /// Missing `Content-Type: application/json` header.
    MissingContentType,
    /// Invalid `Content-Type` header (not application/json).
    InvalidContentType,
}

impl fmt::Display for JsonRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonRejection::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            JsonRejection::Deserialize(err) => {
                write!(f, "Failed to deserialize JSON: {err}")
            }
            JsonRejection::MissingContentType => {
                write!(f, "Missing `Content-Type: application/json` header")
            }
            JsonRejection::InvalidContentType => {
                write!(
                    f,
                    "Invalid `Content-Type` header: expected `application/json`"
                )
            }
        }
    }
}

impl ResponseError for JsonRejection {
    fn status_code(&self) -> StatusCode {
        match self {
            JsonRejection::Body(_error) => StatusCode::BAD_REQUEST,
            JsonRejection::Deserialize(_deserialize_error) => StatusCode::UNPROCESSABLE_ENTITY,
            JsonRejection::MissingContentType | JsonRejection::InvalidContentType => {
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            }
        }
    }
}

impl<T: Facet<'static>> actix_web::FromRequest for Json<T> {
    type Error = JsonRejection;
    type Future = JsonExtractFut<T>;

    fn from_request(
        req: &actix_web::HttpRequest,
        payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        JsonExtractFut {
            req: Some(req.clone()),
            bytes: Bytes::from_request(req, payload),
            marker: PhantomData,
        }
    }
}

pub struct JsonExtractFut<T: Facet<'static>> {
    req: Option<HttpRequest>,
    bytes: <Bytes as FromRequest>::Future,
    marker: PhantomData<T>,
}

impl<T: Facet<'static>> Unpin for JsonExtractFut<T> {}

impl<T: Facet<'static>> Future for JsonExtractFut<T> {
    type Output = Result<Json<T>, JsonRejection>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let JsonExtractFut { req, bytes, .. } = self.get_mut();

        if let Some(req) = req.take() {
            match req.headers().get(CONTENT_TYPE) {
                Some(ct) if ct != APPLICATION_JSON.as_ref() => {
                    Err(JsonRejection::InvalidContentType)?
                }
                Some(_) => (),
                None => Err(JsonRejection::MissingContentType)?,
            }
        }

        let fut = Pin::new(bytes);

        let res = ready!(fut.poll(cx));

        let res = match res {
            Err(err) => Err(JsonRejection::Body(err)),
            Ok(data) => match facet_json::from_slice::<T>(&data) {
                Ok(data) => Ok(Json(data)),
                Err(e) => Err(JsonRejection::Deserialize(e))?,
            },
        };

        Poll::Ready(res)
    }
}

impl<'a, T: Facet<'a>> Responder for Json<T> {
    type Body = EitherBody<String>;

    fn respond_to(self, _: &HttpRequest) -> HttpResponse<Self::Body> {
        match facet_json::to_string(&self.0) {
            Ok(body) => match HttpResponse::Ok()
                .content_type(mime::APPLICATION_JSON)
                .message_body(body)
            {
                Ok(res) => res.map_into_left_body(),
                Err(err) => HttpResponse::from_error(err).map_into_right_body(),
            },

            Err(err) => {
                HttpResponse::from_error(SerializeErrorToActixError(err)).map_into_right_body()
            }
        }
    }
}

#[derive(Debug)]
struct SerializeErrorToActixError(pub SerializeError<JsonSerializeError>);

impl fmt::Display for SerializeErrorToActixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl actix_web::ResponseError for SerializeErrorToActixError {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}
