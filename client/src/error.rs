use harmony_rust_sdk::{
    api::{
        exports::{
            hrpc::url::{ParseError, Url},
            prost::Message,
        },
        harmonytypes::Error,
    },
    client::error::{ClientError as InnerClientError, HmcParseError, InternalClientError},
};
use std::fmt::{self, Display};

pub type ClientResult<T> = Result<T, ClientError>;

#[derive(Debug)]
pub enum ClientError {
    /// Error occurred during an IO operation.
    IoError(std::io::Error),
    /// Error occurred while parsing a string as URL.
    UrlParse(String, ParseError),
    /// Error occurred while parsing an URL as HMC.
    HmcParse(Url, HmcParseError),
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
        Self::IoError(other)
    }
}

impl From<InnerClientError> for ClientError {
    fn from(other: InnerClientError) -> Self {
        Self::Internal(other)
    }
}

impl From<InternalClientError> for ClientError {
    fn from(other: InternalClientError) -> Self {
        Self::Internal(InnerClientError::Internal(other))
    }
}

impl Display for ClientError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientError::HmcParse(url, err) => {
                write!(fmt, "Could not parse URL '{}' as HMC: {}", url, err)
            }
            ClientError::UrlParse(string, err) => {
                write!(fmt, "Could not parse string '{}' as URL: {}", string, err)
            }
            ClientError::Internal(err) => {
                if let InnerClientError::Internal(
                    harmony_rust_sdk::api::exports::hrpc::client::ClientError::EndpointError { raw_error, .. },
                ) = err
                {
                    match Error::decode(raw_error.clone()) {
                        Ok(err) => {
                            write!(
                                fmt,
                                "API error: {} | {}",
                                err.identifier.replace('\n', " "),
                                err.human_message.replace('\n', " ")
                            )
                        }
                        Err(_) => {
                            write!(
                                fmt,
                                "API error: {}",
                                std::str::from_utf8(raw_error)
                                    .unwrap_or("couldn't parse error")
                                    .replace('\n', " "),
                            )
                        }
                    }
                } else {
                    write!(fmt, "{}", err)
                }
            }
            ClientError::IoError(err) => write!(fmt, "An IO error occurred: {}", err),
            ClientError::AlreadyLoggedIn => write!(fmt, "Already logged in with another user."),
            ClientError::MissingLoginInfo => {
                write!(fmt, "Missing required login information, can't login.")
            }
            ClientError::Custom(msg) => write!(fmt, "{}", msg),
        }
    }
}
