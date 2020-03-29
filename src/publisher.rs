use crate::event::Event;
use hmac::{crypto_mac::InvalidKeyLength, Hmac, Mac};
use reqwest::{
    blocking::Client,
    header::{HeaderName, HeaderValue, InvalidHeaderValue, CONTENT_TYPE},
    Error as ReqwestError, StatusCode,
};
use serde_json::Error as JsonError;
use sha2::Sha256;
use std::{error::Error, fmt, sync::mpsc::Receiver, thread::sleep, time::Duration};

const MAX_TRIES: u64 = 5;

pub enum Payload {
    Event(Event),
    Stop,
}

pub struct Publisher {
    client: Client,
    receiver: Receiver<Payload>,
    url: String,
    secret: Hmac<Sha256>,
    error_queue: Vec<Event>,
}

impl Publisher {
    pub fn new<U>(client: Client, url: U, secret: &[u8], receiver: Receiver<Payload>) -> Result<Self, PublisherError>
    where
        U: Into<String>,
    {
        Ok(Self {
            client,
            url: url.into(),
            secret: Hmac::new_varkey(secret)?,
            receiver,
            error_queue: Vec::new(),
        })
    }

    pub fn run(mut self) {
        loop {
            match self.receiver.recv() {
                Ok(Payload::Event(Event::ConfigChanged(event))) => {
                    self.url = event.url;
                    match Hmac::new_varkey(event.secret.as_bytes()) {
                        Ok(secret) => {
                            self.secret = secret;
                        }
                        Err(err) => {
                            eprintln!("[playlog] Failed to reload secret: {}", err);
                        }
                    }
                }
                Ok(Payload::Event(event)) => {
                    if let Err(err) = self.try_publish_event(&event) {
                        eprintln!("[playlog] Failed to publish an event: {}", err);
                        self.error_queue.push(event);
                    }
                }
                Ok(Payload::Stop) => {
                    for event in &self.error_queue {
                        if let Err(err) = self.publish_event(&event) {
                            eprintln!("[playlog] Failed to publish an event: {}", err);
                        }
                    }
                    break;
                }
                Err(err) => {
                    eprintln!("[playlog] Failed to receive an event: {}", err);
                }
            }
        }
    }

    fn try_publish_event(&self, event: &Event) -> Result<(), PublisherError> {
        let mut current_try = 0;
        loop {
            match self.publish_event(event) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    if current_try == MAX_TRIES {
                        return Err(err);
                    }
                    eprintln!("[playlog] Failed to publish an event: {}, trying again...", err);
                    sleep(Duration::from_millis(100 * current_try));
                    current_try += 1;
                }
            }
        }
    }

    fn publish_event(&self, event: &Event) -> Result<(), PublisherError> {
        let data = serde_json::to_vec(&event)?;
        let mut secret = self.secret.clone();
        secret.input(&data);
        let secret = secret.result();
        let rep = self
            .client
            .post(&self.url)
            .header(
                HeaderName::from_static("x-hmac-signature"),
                HeaderValue::from_str(&hex::encode(secret.code()))?,
            )
            .header(CONTENT_TYPE, "application/json")
            .body(data)
            .send()?;
        let status = rep.status();
        if !status.is_success() {
            Err(PublisherError::RequestFailed(status))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
pub enum PublisherError {
    InvalidHeaderValue(InvalidHeaderValue),
    InvalidKeyLength(InvalidKeyLength),
    Json(JsonError),
    Reqwest(ReqwestError),
    RequestFailed(StatusCode),
}

impl From<InvalidHeaderValue> for PublisherError {
    fn from(err: InvalidHeaderValue) -> Self {
        PublisherError::InvalidHeaderValue(err)
    }
}

impl From<InvalidKeyLength> for PublisherError {
    fn from(err: InvalidKeyLength) -> Self {
        PublisherError::InvalidKeyLength(err)
    }
}

impl From<JsonError> for PublisherError {
    fn from(err: JsonError) -> Self {
        PublisherError::Json(err)
    }
}

impl From<ReqwestError> for PublisherError {
    fn from(err: ReqwestError) -> Self {
        PublisherError::Reqwest(err)
    }
}

impl Error for PublisherError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PublisherError::InvalidHeaderValue(err) => Some(err),
            PublisherError::InvalidKeyLength(_) => None,
            PublisherError::Json(err) => Some(err),
            PublisherError::Reqwest(err) => Some(err),
            PublisherError::RequestFailed(_) => None,
        }
    }
}

impl fmt::Display for PublisherError {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PublisherError::InvalidHeaderValue(err) => write!(out, "could not set request header: {}", err),
            PublisherError::InvalidKeyLength(err) => write!(out, "secret key error: {}", err),
            PublisherError::Json(err) => write!(out, "can not serialize JSON: {}", err),
            PublisherError::Reqwest(err) => write!(out, "failed to send HTTP request: {}", err),
            PublisherError::RequestFailed(status) => write!(out, "server respond with {} status code", status),
        }
    }
}
