use crate::{
    api::{Api, ConfigError, TrackInfo, TrackInfoError},
    sys::{ddb_event_track_t, ddb_event_trackchange_t, DB_EV_CONFIGCHANGED, DB_EV_SONGCHANGED, DB_EV_SONGSTARTED},
};
use serde::Serialize;
use std::{error::Error, fmt};

#[derive(Debug, Serialize)]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum Event {
    ConfigChanged(EventConfigChanged),
    Start(EventStart),
    Stop(EventStop),
}

impl Event {
    pub(crate) unsafe fn from_raw(
        api: Api,
        id: u32,
        ctx: usize,
        _p1: u32,
        _p2: u32,
    ) -> Result<Option<Event>, EventError> {
        match id {
            DB_EV_CONFIGCHANGED => EventConfigChanged::read(api).map(|x| Some(Event::ConfigChanged(x))),
            DB_EV_SONGCHANGED => EventStop::from_context(api, ctx).map(|x| x.map(Event::Stop)),
            DB_EV_SONGSTARTED => EventStart::from_context(api, ctx).map(|x| Some(Event::Start(x))),
            _ => Ok(None),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EventConfigChanged {
    pub(crate) url: String,
    pub(crate) secret: String,
}

impl EventConfigChanged {
    unsafe fn read(api: Api) -> Result<Self, EventError> {
        let url = api.conf_get_str("playlog.url").map_err(EventError::ReadConfig)?;
        let secret = api.conf_get_str("playlog.secret").map_err(EventError::ReadConfig)?;
        Ok(EventConfigChanged { url, secret })
    }
}

#[derive(Debug, Serialize)]
pub struct EventStart {
    #[serde(flatten)]
    track_info: TrackInfo,
}

impl EventStart {
    unsafe fn from_context(api: Api, context: usize) -> Result<Self, EventError> {
        let ptr = context as *mut ddb_event_track_t;
        if ptr.is_null() {
            return Err(EventError::NoContext);
        }
        let raw = *ptr;
        let track_info = api.get_track_info(raw.track).map_err(EventError::ReadTrackInfo)?;
        Ok(Self { track_info })
    }
}

#[derive(Debug, Serialize)]
pub struct EventStop {
    #[serde(flatten)]
    track_info: TrackInfo,
    play_time: f32,
    started_at: i64,
}

impl EventStop {
    unsafe fn from_context(api: Api, context: usize) -> Result<Option<Self>, EventError> {
        let ptr = context as *mut ddb_event_trackchange_t;
        if ptr.is_null() {
            return Err(EventError::NoContext);
        }
        let raw = *ptr;
        if raw.from.is_null() {
            return Ok(None);
        }
        Ok(Some(Self {
            track_info: api.get_track_info(raw.from).map_err(EventError::ReadTrackInfo)?,
            play_time: raw.playtime,
            started_at: raw.started_timestamp,
        }))
    }
}

#[derive(Debug)]
pub enum EventError {
    ReadConfig(ConfigError),
    NoContext,
    ReadTrackInfo(TrackInfoError),
}

impl Error for EventError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use self::EventError::*;
        match self {
            ReadConfig(err) => Some(err),
            NoContext => None,
            ReadTrackInfo(err) => Some(err),
        }
    }
}

impl fmt::Display for EventError {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        use self::EventError::*;
        match self {
            ReadConfig(err) => write!(out, "failed to read config: {}", err),
            NoContext => write!(out, "event context is NULL"),
            ReadTrackInfo(err) => write!(out, "{}", err),
        }
    }
}
