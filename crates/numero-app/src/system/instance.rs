//! One Numero at a time. The running app listens on a Unix socket; a second
//! launch hands it what it was asked to open, or nothing, which shows the
//! window, and exits. The same on Linux and macOS: on Linux the socket is
//! in `$XDG_RUNTIME_DIR`, which only the user can enter; on macOS, where
//! GPUI already brings a bundle forward but not a bare binary, it is in the
//! app's own data directory, also the user's alone.
//!
//! A message is one line of JSON: `{"open":[…]}`, each entry an
//! [`Opening`]. A socket left behind by an app that crashed refuses the
//! connection; the next launch removes it and listens in its place.

use std::fs;
use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use async_channel::Sender;
use serde::{Deserialize, Serialize};

use super::Event;
use super::opening::Opening;
use crate::app::APP_ID;

/// The longest message read, in bytes.
const MAX_MESSAGE: u64 = 4 << 20;

/// The most openings one message may carry.
const MAX_OPENINGS: usize = 64;

/// Set to anything, a launch neither looks for a running Numero nor
/// listens for later ones: a developer knob, for running a build beside
/// the Numero in use, or several builds at once.
const OFF_SWITCH: &str = "NUMERO_NO_SINGLE_INSTANCE";

/// How long either side waits on the other.
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Message {
    open: Vec<Opening>,
}

/// Encodes `openings` as a message line.
fn encode(openings: &[Opening]) -> io::Result<Vec<u8>> {
    let mut line = serde_json::to_vec(&Message {
        open: openings.to_vec(),
    })
    .map_err(io::Error::other)?;
    line.push(b'\n');
    Ok(line)
}

/// Reads a message line, checking every opening in it.
fn decode(line: &[u8]) -> Result<Vec<Opening>, String> {
    let message: Message =
        serde_json::from_slice(line).map_err(|error| format!("not a message: {error}"))?;
    if message.open.len() > MAX_OPENINGS {
        return Err(format!("more than {MAX_OPENINGS} things to open at once"));
    }
    if let Some(bad) = message.open.iter().find(|opening| !opening.is_valid()) {
        return Err(format!("can't open {bad:?}"));
    }
    Ok(message.open)
}

/// The socket's path.
fn socket_path() -> Option<PathBuf> {
    let dir = dirs::runtime_dir().or_else(|| dirs::data_dir().map(|dir| dir.join(APP_ID)))?;
    Some(dir.join(format!("{APP_ID}.sock")))
}

/// What a launch found.
#[derive(Debug)]
pub(crate) enum Claim {
    /// Another Numero runs and took the openings; this one should exit.
    Forwarded,
    /// This is the one Numero, listening when it could.
    First(Option<Instance>),
}

/// Hands `openings` to the Numero already running, or becomes the one
/// that runs.
pub(crate) fn claim(openings: &[Opening]) -> Claim {
    if std::env::var_os(OFF_SWITCH).is_some() {
        tracing::info!("{OFF_SWITCH} is set; not looking for a running Numero");
        return Claim::First(None);
    }
    let Some(path) = socket_path() else {
        tracing::warn!("no runtime directory; a second launch will start a second Numero");
        return Claim::First(None);
    };
    claim_at(&path, openings)
}

fn claim_at(path: &Path, openings: &[Opening]) -> Claim {
    match UnixStream::connect(path) {
        Ok(stream) => match forward(stream, openings) {
            Ok(()) => return Claim::Forwarded,
            Err(error) => {
                // Something listens but doesn't answer: leave its socket be.
                tracing::warn!(%error, path = %path.display(), "the running Numero didn't answer; starting another");
                return Claim::First(None);
            }
        },
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            tracing::info!(path = %path.display(), "removing the socket a closed Numero left");
            if let Err(error) = fs::remove_file(path) {
                tracing::warn!(%error, "couldn't remove the old socket");
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "can't reach the socket; a second launch will start a second Numero");
            return Claim::First(None);
        }
    }
    match listen(path) {
        Ok(listener) => Claim::First(Some(Instance {
            listener,
            path: path.to_owned(),
        })),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "couldn't listen for a second launch");
            Claim::First(None)
        }
    }
}

fn forward(mut stream: UnixStream, openings: &[Opening]) -> io::Result<()> {
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.write_all(&encode(openings)?)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    if answer.trim() == "ok" {
        Ok(())
    } else {
        Err(io::Error::other(format!("it answered {answer:?}")))
    }
}

fn listen(path: &Path) -> io::Result<UnixListener> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    UnixListener::bind(path)
}

/// The listening socket of the one Numero.
#[derive(Debug)]
pub(crate) struct Instance {
    listener: UnixListener,
    path: PathBuf,
}

impl Instance {
    /// Answers second launches on a thread of its own, sending what each
    /// asks for to `events` as [`Event::Open`]. Returns the socket's path,
    /// to remove on quitting.
    pub(crate) fn serve(self, events: Sender<Event>) -> PathBuf {
        let Self { listener, path } = self;
        let spawned = thread::Builder::new()
            .name("numero-instance".to_owned())
            .spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            if let Some(openings) = answer(stream)
                                && events.send_blocking(Event::Open(openings)).is_err()
                            {
                                return;
                            }
                        }
                        Err(error) => tracing::warn!(%error, "a second launch couldn't connect"),
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "couldn't listen for a second launch");
        }
        path
    }
}

/// Reads one message from a second launch and answers it.
fn answer(mut stream: UnixStream) -> Option<Vec<Opening>> {
    let read = (|| {
        stream.set_read_timeout(Some(TIMEOUT))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        let mut line = Vec::new();
        BufReader::new((&stream).take(MAX_MESSAGE)).read_until(b'\n', &mut line)?;
        Ok::<_, io::Error>(line)
    })();
    let decoded = read
        .map_err(|error| error.to_string())
        .and_then(|line| decode(&line));
    let reply: &[u8] = if decoded.is_ok() {
        b"ok\n"
    } else {
        b"refused\n"
    };
    if let Err(error) = stream.write_all(reply) {
        tracing::debug!(%error, "couldn't answer a second launch");
    }
    decoded
        .inspect(|openings| {
            tracing::info!(
                count = openings.len(),
                "a second launch asked to show Numero"
            )
        })
        .inspect_err(|error| tracing::warn!(%error, "refused a second launch's message"))
        .ok()
}

/// Removes the socket at `path` on quitting, so the next launch needn't.
pub(crate) fn release(path: &Path) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::debug!(%error, "couldn't remove the socket");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::opening::Link;

    fn openings() -> Vec<Opening> {
        vec![
            Opening::File(PathBuf::from("/tmp/a.numi")),
            Opening::Link(Link::New {
                text: "1 + 1".to_owned(),
            }),
            Opening::Link(Link::Show),
        ]
    }

    #[test]
    fn a_message_round_trips() {
        let line = encode(&openings()).unwrap();
        assert_eq!(line.last(), Some(&b'\n'));
        assert_eq!(decode(&line).unwrap(), openings());
        assert_eq!(decode(&encode(&[]).unwrap()).unwrap(), []);
    }

    #[test]
    fn a_bad_message_is_refused() {
        assert!(decode(b"hello\n").is_err());
        assert!(decode(br#"{"open":[{"file":"relative.numi"}]}"#).is_err());
        assert!(decode(br#"{"open":[{"link":{"open":{"name":""}}}]}"#).is_err());
        let many = vec![Opening::Link(Link::Show); MAX_OPENINGS + 1];
        assert!(decode(&encode(&many).unwrap()).is_err());
    }

    #[test]
    fn a_second_launch_hands_its_openings_to_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run").join("numero.sock");
        let Claim::First(Some(first)) = claim_at(&path, &[]) else {
            panic!("the first launch should listen");
        };
        let (events, received) = async_channel::unbounded();
        first.serve(events);

        assert!(matches!(claim_at(&path, &openings()), Claim::Forwarded));

        let Ok(Event::Open(got)) = received.recv_blocking() else {
            panic!("the first launch should hear of the second");
        };
        assert_eq!(got, openings());
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    fn a_socket_left_behind_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("numero.sock");
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());

        assert!(matches!(claim_at(&path, &[]), Claim::First(Some(_))));
    }
}
