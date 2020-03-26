use crate::sys::{DB_functions_t, DB_metaInfo_t, DB_playItem_t};
use ffix::{string::StringReader, Error as FfixError};
use serde::Serialize;
use std::{
    collections::HashMap,
    error::Error,
    ffi::{CStr, CString, NulError},
    fmt,
    os::raw::{c_char, c_int},
    ptr::null,
    str::Utf8Error,
};

const KEY_ARTIST: &str = "artist";
const KEYS_ALBUM_ARTIST: &[&str] = &["band", "album artist", "albumartist"];
const KEY_ALBUM: &str = "album";
const KEY_TITLE: &str = "title";
const KEY_YEAR: &str = "year";
const KEY_DISC_NUMBER: &str = "disc";
const KEY_TOTAL_DISCS: &str = "numdiscs";
const KEY_TRACK_NUMBER: &str = "track";
const KEY_TOTAL_TRACKS: &str = "numtracks";

#[derive(Clone, Copy, Debug)]
pub(crate) struct Api {
    _conf_get_str:
        unsafe extern "C" fn(key: *const c_char, def: *const c_char, buffer: *mut c_char, buffer_size: c_int),
    _pl_get_item_duration: unsafe extern "C" fn(it: *mut DB_playItem_t) -> f32,
    _pl_get_metadata_head: unsafe extern "C" fn(it: *mut DB_playItem_t) -> *mut DB_metaInfo_t,
    _pl_lock: unsafe extern "C" fn(),
    _pl_unlock: unsafe extern "C" fn(),
}

impl Api {
    pub(crate) unsafe fn new(ptr: *mut DB_functions_t) -> Result<Self, ApiError> {
        assert!(!ptr.is_null());
        let api = *ptr;
        macro_rules! get_method {
            ($name:ident) => {{
                match api.$name {
                    Some(v) => v,
                    None => return Err(ApiError::MethodNotFound(String::from(stringify!($name)))),
                }
            }};
        }
        Ok(Self {
            _conf_get_str: get_method!(conf_get_str),
            _pl_get_item_duration: get_method!(pl_get_item_duration),
            _pl_get_metadata_head: get_method!(pl_get_metadata_head),
            _pl_lock: get_method!(pl_lock),
            _pl_unlock: get_method!(pl_unlock),
        })
    }

    pub(crate) unsafe fn conf_get_str<K>(&self, key: K) -> Result<String, ConfigError>
    where
        K: Into<Vec<u8>>,
    {
        const CAPACITY: i32 = 2000;
        let key = CString::new(key).map_err(ConfigError::ConvertKey)?;
        let mut reader = StringReader::new(CAPACITY as usize);
        let default_value = null();
        (self._conf_get_str)(key.as_ptr(), default_value, reader.get_target(), CAPACITY);
        let value = reader.into_string_opt().map_err(ConfigError::ReadString)?;
        match value {
            Some(value) => {
                if value.is_empty() {
                    Err(ConfigError::KeyMissing)
                } else {
                    Ok(value)
                }
            }
            None => Err(ConfigError::KeyMissing),
        }
    }

    unsafe fn get_metadata(&self, ptr: *mut DB_playItem_t) -> Result<HashMap<String, String>, MetadataError> {
        let mut metadata = HashMap::new();
        let mut raw_metadata = (self._pl_get_metadata_head)(ptr).as_ref();
        while let Some(raw_item) = raw_metadata {
            let (key, val) = (CStr::from_ptr(raw_item.key), CStr::from_ptr(raw_item.value));
            let utf8_key = key.to_str().map_err(MetadataError::InvalidKey)?;
            let utf8_val = val.to_str().map_err(MetadataError::InvalidValue)?;
            metadata.insert(utf8_key.to_string().to_lowercase(), utf8_val.to_string());
            raw_metadata = raw_item.next.as_ref();
        }
        Ok(metadata)
    }

    pub(crate) unsafe fn get_track_info(&self, ptr: *mut DB_playItem_t) -> Result<TrackInfo, TrackInfoError> {
        if ptr.is_null() {
            return Err(TrackInfoError::NoTrack);
        }
        let _lock = PlaylistLock::new(*self);
        let metadata = self.get_metadata(ptr).map_err(TrackInfoError::ReadMetadata)?;
        macro_rules! required_string {
            ($key:expr) => {
                match metadata.get($key) {
                    Some(value) => String::from(value),
                    None => {
                        return Err(TrackInfoError::MetadataKeyNotFound(String::from($key)));
                    }
                }
            };
        }
        let mut album_artist = None;
        for key in KEYS_ALBUM_ARTIST {
            if let Some(value) = metadata.get(*key) {
                album_artist = Some(String::from(value));
                break;
            }
        }
        macro_rules! optional_u32 {
            ($key:expr) => {
                match metadata.get($key) {
                    Some(value) => match value.parse::<u32>() {
                        Ok(value) => Some(value),
                        Err(err) => {
                            eprintln!("[playlog] can not parse '{}' as u32: {}", $key, err);
                            None
                        }
                    },
                    None => None,
                }
            };
        }
        let duration = (self._pl_get_item_duration)(ptr);
        Ok(TrackInfo {
            artist: required_string!(KEY_ARTIST),
            album_artist,
            album: required_string!(KEY_ALBUM),
            title: required_string!(KEY_TITLE),
            year: optional_u32!(KEY_YEAR),
            disc_number: optional_u32!(KEY_DISC_NUMBER),
            total_discs: optional_u32!(KEY_TOTAL_DISCS),
            track_number: optional_u32!(KEY_TRACK_NUMBER),
            total_tracks: optional_u32!(KEY_TOTAL_TRACKS),
            duration,
        })
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct TrackInfo {
    artist: String,
    album_artist: Option<String>,
    album: String,
    title: String,
    year: Option<u32>,
    disc_number: Option<u32>,
    total_discs: Option<u32>,
    track_number: Option<u32>,
    total_tracks: Option<u32>,
    duration: f32,
}

struct PlaylistLock {
    api: Api,
}

impl PlaylistLock {
    fn new(api: Api) -> Self {
        unsafe { (api._pl_lock)() }
        Self { api }
    }
}

impl Drop for PlaylistLock {
    fn drop(&mut self) {
        unsafe { (self.api._pl_unlock)() }
    }
}

#[derive(Debug)]
pub enum ApiError {
    MethodNotFound(String),
}

impl Error for ApiError {}

impl fmt::Display for ApiError {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        use self::ApiError::*;
        match self {
            MethodNotFound(name) => write!(out, "method '{}' is not found in DeaDBeeF API", name),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    ConvertKey(NulError),
    KeyMissing,
    ReadString(FfixError),
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use self::ConfigError::*;
        Some(match self {
            ConvertKey(err) => err,
            KeyMissing => return None,
            ReadString(err) => err,
        })
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        use self::ConfigError::*;
        match self {
            ConvertKey(err) => write!(out, "could not create CString for a key: {}", err),
            KeyMissing => write!(out, "configuration option is missing"),
            ReadString(err) => write!(out, "could not read a string from config: {}", err),
        }
    }
}

#[derive(Debug)]
pub enum TrackInfoError {
    ReadMetadata(MetadataError),
    MetadataKeyNotFound(String),
    NoTrack,
}

impl Error for TrackInfoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use self::TrackInfoError::*;
        match self {
            ReadMetadata(err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for TrackInfoError {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        use self::TrackInfoError::*;
        match self {
            ReadMetadata(err) => write!(out, "can not read metadata: {}", err),
            MetadataKeyNotFound(key) => write!(out, "'{}' is not found in track metadata", key),
            NoTrack => write!(out, "can not get track info: DB_playItem_t is NULL"),
        }
    }
}

#[derive(Debug)]
pub enum MetadataError {
    InvalidKey(Utf8Error),
    InvalidValue(Utf8Error),
}

impl Error for MetadataError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use self::MetadataError::*;
        Some(match self {
            InvalidKey(err) => err,
            InvalidValue(err) => err,
        })
    }
}

impl fmt::Display for MetadataError {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        use self::MetadataError::*;
        match self {
            InvalidKey(err) => write!(out, "key is not valid UTF-8 string: {}", err),
            InvalidValue(err) => write!(out, "value is not valid UTF-8 string: {}", err),
        }
    }
}
