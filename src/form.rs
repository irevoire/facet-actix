use core::fmt;
use std::{
    marker::PhantomData,
    ops,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_web::{
    FromRequest, HttpRequest, ResponseError,
    http::{StatusCode, header::CONTENT_TYPE},
    mime::{APPLICATION_WWW_FORM_URLENCODED, MULTIPART_FORM_DATA},
    web::Bytes,
};
use facet::Facet;

#[derive(Debug, facet::Facet)]
#[facet(transparent)]
pub struct Form<T>(pub T);

impl<T> Form<T> {
    /// Unwrap into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> ops::Deref for Form<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for Form<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Form<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Debug)]
pub enum FormRejection {
    /// Failed to read the request body.
    Body(actix_web::Error),
    /// Failed to deserialize the form data.
    Deserialize(facet_urlencoded::UrlEncodedError),
    /// Missing `Content-Type: x-www-form-urlencoded` header.
    MissingContentType,
    /// Invalid `Content-Type` header (not x-www-form-urlencoded).
    InvalidContentType,
}

impl fmt::Display for FormRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormRejection::Body(err) => {
                write!(f, "Failed to read request body: {err}")
            }
            FormRejection::Deserialize(err) => {
                write!(f, "Failed to deserialize form: {err}")
            }
            FormRejection::MissingContentType => {
                write!(f, "Missing `Content-Type: x-www-form-urlencoded` header")
            }
            FormRejection::InvalidContentType => {
                write!(
                    f,
                    "Invalid `Content-Type` header: expected `x-www-form-urlencoded`"
                )
            }
        }
    }
}

impl ResponseError for FormRejection {
    fn status_code(&self) -> StatusCode {
        match self {
            FormRejection::Body(_error) => StatusCode::BAD_REQUEST,
            FormRejection::Deserialize(_deserialize_error) => StatusCode::UNPROCESSABLE_ENTITY,
            FormRejection::MissingContentType | FormRejection::InvalidContentType => {
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            }
        }
    }
}

impl<T: Facet<'static>> actix_web::FromRequest for Form<T> {
    type Error = FormRejection;
    type Future = FormExtractFut<T>;

    fn from_request(
        req: &actix_web::HttpRequest,
        payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        FormExtractFut {
            req: Some(req.clone()),
            bytes: Bytes::from_request(req, payload),
            marker: PhantomData,
        }
    }
}

pub struct FormExtractFut<T: Facet<'static>> {
    req: Option<HttpRequest>,
    bytes: <Bytes as FromRequest>::Future,
    marker: PhantomData<T>,
}

impl<T: Facet<'static>> Unpin for FormExtractFut<T> {}

impl<T: Facet<'static>> Future for FormExtractFut<T> {
    type Output = Result<Form<T>, FormRejection>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let FormExtractFut { req, bytes, .. } = self.get_mut();

        if let Some(req) = req.take() {
            match req.headers().get(CONTENT_TYPE) {
                Some(ct)
                    if !ct
                        .to_str()
                        // TODO: remove unwrap
                        .unwrap()
                        .starts_with(APPLICATION_WWW_FORM_URLENCODED.as_ref())
                        && !ct
                            .to_str()
                            .unwrap()
                            .starts_with(MULTIPART_FORM_DATA.as_ref()) =>
                {
                    Err(FormRejection::InvalidContentType)?
                }
                Some(_) => (),
                None => Err(FormRejection::MissingContentType)?,
            }
        }

        let fut = Pin::new(bytes);

        let res = ready!(fut.poll(cx));

        let res = match res {
            Err(err) => Err(FormRejection::Body(err)),
            Ok(data) => {
                match facet_urlencoded::from_str_owned::<T>(str::from_utf8(data.as_ref()).unwrap())
                {
                    Ok(data) => Ok(Form(data)),
                    Err(e) => Err(FormRejection::Deserialize(e))?,
                }
            }
        };

        Poll::Ready(res)
    }
}
