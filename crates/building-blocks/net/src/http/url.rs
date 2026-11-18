//! What a request may be addressed with.

use url::Url;

use super::Error;

/// A value that names a URL: a [`Url`], or text that parses as one.
pub trait IntoUrl {
    fn into_url(self) -> Result<Url, Error>;
}

impl IntoUrl for Url {
    fn into_url(self) -> Result<Url, Error> {
        Ok(self)
    }
}

impl IntoUrl for &Url {
    fn into_url(self) -> Result<Url, Error> {
        Ok(self.clone())
    }
}

impl IntoUrl for &str {
    fn into_url(self) -> Result<Url, Error> {
        Url::parse(self).map_err(|error| Error::builder(format!("{self}: {error}")))
    }
}

impl IntoUrl for String {
    fn into_url(self) -> Result<Url, Error> {
        self.as_str().into_url()
    }
}

impl IntoUrl for &String {
    fn into_url(self) -> Result<Url, Error> {
        self.as_str().into_url()
    }
}
