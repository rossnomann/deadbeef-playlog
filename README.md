# DEADBEEF-PLAYLOG

A [DeaDBeeF](https://deadbeef.sourceforge.io/) plugin which sends played songs information to an HTTP server.

## Requirements

- [Rust](https://rust-lang.org) (tested on 1.42.0)
- [Clang and other bindgen requirements](https://rust-lang.github.io/rust-bindgen/requirements.html)

## Installation

```
$ cargo build --release
$ mkdir -p ~/.local/lib/deadbeef
$ cp target/release/libplaylog.so ~/.local/lib/deadbeef/playlog.so
```

## Usage

Open DeaDBeeF and go to `Edit` > `Preferences` > `Plugins` > `Playlog` > `Configure`

Set a server URL (e.g. `http://127.0.0.1/submit`) and a secret key.

Server should receive events in the following format:

```
{
    "event": "start",  // Track started playing
    "data": {
        "artist": "Cattle Decapitation",  // Artist name
        "album_artist": "Cattle Decapitation",  // Album artist name (optional)
        "album": "Humanure",  // Album name
        "title": "Humanure",  // Track title
        "year": 2004,  // Album release year
        "disc_number": 1,  // Number of disc
        "total_discs": 1,  // Total number of discs
        "track_number": 2,  // Number of track on disc
        "total_tracks": 11,  // Total number of tracks on disc
        "duration": 185.6  // Track duration in seconds
    }
}
```

```
{
    "event": "stop",  // Track stopped playing
    "data": {
        "artist": "Cattle Decapitation",
        "album_artist": "Cattle Decapitation",
        "album": "Humanure",
        "title": "Humanure",
        "year": 2004,
        "disc_number": 1,
        "total_discs": 1,
        "track_number": 2,
        "total_tracks": 11,
        "duration": 185.57333,
        "play_time": 0.9752379,  // Total played time in seconds
        "started_at": 1585189977  // UNIX timestamp when the track started playing
    }
}
```

In `X-HMAC-SIGNATURE` header you will receive a signature which allows to verify incoming request:

```python
import hashlib
import hmac


SECRET = 'my-secret'  # A secret key (same as in plugin settings)


def verify_signature(expected_signature, data):
    actual_signature = hmac.new(SECRET, digestmod=hashlib.sha256)
    actual_signature.update(data)
    actual_signature = actual_signature.hexdigest()
    return actual_signature == expected_signature


def handle_request(request):
    sig = request.headers.get('X-HMAC-SIGNATURE')
    data = request.read()  # read request body here
    if verify_signature(sig, data):
        print('OK')
    else:
        print('FORBIDDEN')
    # ...

```

See [server-example.py](./server-example.py) for more details.

## Changelog

### 0.1.0 (xx.yy.2020)

- First release.

## LICENSE

The MIT License (MIT)
