use harmony_rust_sdk::{
    api::exports::http::{uri::InvalidUri, Uri},
    client::error::{ClientError as InnerClientError, HmcParseError},
};
use std::fmt::{self, Display};

pub type ClientResult<T> = Result<T, ClientError>;

#[derive(Debug)]
pub enum ClientError {
    /// Error occurred during an IO operation.
    IOError(std::io::Error),
    /// Error occurred while parsing a string as URL.
    URLParse(String, InvalidUri),
    /// Error occurred while parsing an URL as HMC.
    HmcParse(Uri, HmcParseError),
    /// Error occurred in the Harmony client library.
    Internal(InnerClientError),
    /// The user is already logged in.
    AlreadyLoggedIn,
    /// Not all required login information was provided.
    MissingLoginInfo,
    /// Custom error
    Custom(String),
}

impl Clone for ClientError {
    fn clone(&self) -> Self {
        use ClientError::*;

        match self {
            AlreadyLoggedIn => AlreadyLoggedIn,
            MissingLoginInfo => MissingLoginInfo,
            Custom(err) => Custom(err.clone()),
            _ => Custom(self.to_string()),
        }
    }
}

impl From<std::io::Error> for ClientError {
    fn from(other: std::io::Error) -> Self {
        Self::IOError(other)
    }
}

impl From<InnerClientError> for ClientError {
    fn from(other: InnerClientError) -> Self {
        Self::Internal(other)
    }
}

impl Display for ClientError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientError::HmcParse(url, err) => {
                write!(fmt, "Could not parse URL '{}' as HMC: {}", url, err)
            }
            ClientError::URLParse(string, err) => {
                write!(fmt, "Could not parse string '{}' as URL: {}", string, err)
            }
            ClientError::Internal(err) => {
                write!(fmt, "An internal error occurred: {}", err)
            }
            ClientError::IOError(err) => write!(fmt, "An IO error occurred: {}", err),
            ClientError::AlreadyLoggedIn => write!(fmt, "Already logged in with another user."),
            ClientError::MissingLoginInfo => {
                write!(fmt, "Missing required login information, can't login.")
            }
            ClientError::Custom(msg) => write!(fmt, "{}", msg),
        }
    }
}
