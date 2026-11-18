//! One request being built.

use std::fmt::Display;
use std::time::Duration;

use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use serde::Serialize;
use url::Url;

use super::{Client, Error, Response};

/// What a request is made of.
#[derive(Debug, Clone)]
pub(crate) struct Parts {
    pub(crate) method: Method,
    pub(crate) url: Url,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Bytes,
    pub(crate) timeout: Option<Duration>,
}

impl Parts {
    pub(crate) fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url,
            headers: HeaderMap::new(),
            body: Bytes::new(),
            timeout: None,
        }
    }
}

/// A request being built. The first invalid part is kept and returned by
/// [`send`](Self::send).
#[derive(Clone)]
pub struct RequestBuilder {
    client: Client,
    parts: Result<Parts, String>,
}

impl RequestBuilder {
    pub(crate) fn new(client: Client, parts: Result<Parts, Error>) -> Self {
        Self {
            client,
            parts: parts.map_err(|error| error.to_string()),
        }
    }

    fn map(mut self, change: impl FnOnce(&mut Parts) -> Result<(), String>) -> Self {
        if let Ok(parts) = &mut self.parts {
            if let Err(error) = change(parts) {
                self.parts = Err(error);
            }
        }
        self
    }

    pub fn header<K, V>(self, name: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Display,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Display,
    {
        self.map(|parts| {
            let name = HeaderName::try_from(name).map_err(|error| error.to_string())?;
            let value = HeaderValue::try_from(value).map_err(|error| error.to_string())?;
            parts.headers.append(name, value);
            Ok(())
        })
    }

    /// Adds every header of `headers`, replacing a header of the same name.
    pub fn headers(self, headers: HeaderMap) -> Self {
        self.map(|parts| {
            parts.headers.extend(headers);
            Ok(())
        })
    }

    pub fn bearer_auth<T>(self, token: T) -> Self
    where
        T: Display,
    {
        self.map(|parts| {
            let mut value = HeaderValue::try_from(format!("Bearer {token}"))
                .map_err(|error| error.to_string())?;
            value.set_sensitive(true);
            parts.headers.insert(AUTHORIZATION, value);
            Ok(())
        })
    }

    /// `value` as a JSON body, with its content type.
    pub fn json<T>(self, value: &T) -> Self
    where
        T: Serialize + ?Sized,
    {
        self.map(|parts| {
            parts.body = serde_json::to_vec(value)
                .map_err(|error| error.to_string())?
                .into();
            if !parts.headers.contains_key(CONTENT_TYPE) {
                parts
                    .headers
                    .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            }
            Ok(())
        })
    }

    pub fn body(self, body: impl Into<Bytes>) -> Self {
        self.map(|parts| {
            parts.body = body.into();
            Ok(())
        })
    }

    /// Appends `query` to the URL's query string.
    pub fn query<T>(self, query: &T) -> Self
    where
        T: Serialize + ?Sized,
    {
        self.map(|parts| {
            let encoded = serde_urlencoded::to_string(query).map_err(|error| error.to_string())?;
            if !encoded.is_empty() {
                let joined = match parts.url.query() {
                    Some(existing) if !existing.is_empty() => format!("{existing}&{encoded}"),
                    _ => encoded,
                };
                parts.url.set_query(Some(&joined));
            }
            Ok(())
        })
    }

    /// This request's time limit, from connecting to the end of the
    /// response body, in place of the client's.
    pub fn timeout(self, limit: Duration) -> Self {
        self.map(|parts| {
            parts.timeout = Some(limit);
            Ok(())
        })
    }

    /// A copy of this request, to send again.
    pub fn try_clone(&self) -> Option<Self> {
        Some(self.clone())
    }

    pub async fn send(self) -> Result<Response, Error> {
        let parts = self.parts.map_err(Error::Builder)?;
        self.client.execute(parts).await
    }
}
