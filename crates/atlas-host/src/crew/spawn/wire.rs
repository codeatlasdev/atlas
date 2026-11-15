//! Framing of helper messages on a unix stream socket: a 4-byte big-endian length, then that many
//! bytes of JSON. File descriptors travel as `SCM_RIGHTS` with the *first byte* of the frame (the
//! length prefix), through the same [`send_with_fds`]/[`recv_with_fds`] the live upgrade uses.
//!
//! The reader checks the length against [`MAX_MESSAGE`] before allocating, because the other end of
//! the helper's socket is not trusted to be well behaved.

use std::io::{self, Read};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::protocol::MAX_MESSAGE;
use crate::handoff::{recv_with_fds, send_with_fds};

/// Send `msg` as one frame with `fds` attached.
pub fn send<T: Serialize>(sock: &UnixStream, msg: &T, fds: &[RawFd]) -> io::Result<()> {
    let body = serde_json::to_vec(msg).map_err(io::Error::other)?;
    if body.len() > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "message too large",
        ));
    }
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&body);
    // The fds ride with the first `sendmsg`; the whole frame goes in one call, so the receiver's
    // 4-byte read gets them.
    send_with_fds(sock.as_raw_fd(), &frame, fds)
}

/// One received frame: the body length (for the size check in `validate`), the decoded message
/// and any fds that came with it.
pub struct Frame<T> {
    pub len: usize,
    pub msg: T,
    pub fds: Vec<OwnedFd>,
}

/// Read one frame. `Ok(None)` on a clean end of stream (the peer closed between frames).
pub fn recv<T: DeserializeOwned>(sock: &UnixStream) -> io::Result<Option<Frame<T>>> {
    let mut head = [0u8; 4];
    let (n, fds) = recv_with_fds(sock.as_raw_fd(), &mut head)?;
    if n == 0 {
        return Ok(None);
    }
    let mut got = n;
    let mut s = sock;
    if got < 4 {
        s.read_exact(&mut head[got..])?;
        got = 4;
    }
    debug_assert_eq!(got, 4);
    let len = u32::from_be_bytes(head) as usize;
    if len > MAX_MESSAGE {
        // Nothing is read or allocated: the caller drops the connection.
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut body = vec![0u8; len];
    s.read_exact(&mut body)?;
    let msg =
        serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(Frame { len, msg, fds }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::spawn::protocol::{Mode, Request, Response, SpawnReq};
    use std::io::{Read, Write};
    use std::os::fd::FromRawFd;

    fn pair() -> (UnixStream, UnixStream) {
        UnixStream::pair().unwrap()
    }

    fn spawn_req() -> Request {
        Request::Spawn(SpawnReq {
            user: "atlas-cm-3f2a9c1e5b7d".into(),
            argv: vec!["omp".into(), "--π".into()],
            env: [("A".to_string(), "b c".to_string())].into(),
            cwd: "/var/lib/atlas-cm/3f2a9c1e5b7d/work".into(),
            mode: Mode::Pipe,
            cols: 80,
            rows: 24,
        })
    }

    #[test]
    fn every_request_and_reply_round_trips_through_a_frame() {
        let (a, b) = pair();
        let reqs = [
            spawn_req(),
            Request::Signal {
                session: 7,
                sig: 15,
            },
            Request::Wait { session: 7 },
            Request::Cwd { session: 7 },
            Request::Reap {
                user: "atlas-cm-3f2a9c1e5b7d".into(),
            },
            Request::SetMemoryCap { bytes: 3 << 30 },
            Request::MemoryCap,
        ];
        for r in &reqs {
            send(&a, r, &[]).unwrap();
            let f = recv::<Request>(&b).unwrap().unwrap();
            assert_eq!(&f.msg, r);
            assert!(f.fds.is_empty());
        }
        let resps = [
            Response::Spawned {
                session: 1,
                pid: 4242,
            },
            Response::Done,
            Response::Exit {
                exited: true,
                code: Some(3),
                oom_cap: None,
            },
            Response::Exit {
                exited: true,
                code: Some(137),
                oom_cap: Some(64 << 20),
            },
            Response::Exit {
                exited: true,
                code: None,
                oom_cap: None,
            },
            Response::MemoryCap(crate::crew::spawn::cgroup::Status::on(1 << 30)),
            Response::MemoryCap(crate::crew::spawn::cgroup::Status::off("no cgroup v2")),
            Response::Cwd {
                path: "/var/lib/atlas-cm/x/work".into(),
            },
            Response::Error {
                reason: "nope".into(),
            },
        ];
        for r in &resps {
            send(&b, r, &[]).unwrap();
            assert_eq!(&recv::<Response>(&a).unwrap().unwrap().msg, r);
        }
    }

    // The real reason this module exists: the pty master must reach hostd as a live fd.
    #[test]
    fn a_reply_carries_a_working_fd_with_the_message() {
        let (a, b) = pair();
        let (mut keep, give) = UnixStream::pair().unwrap();
        send(
            &a,
            &Response::Spawned { session: 9, pid: 1 },
            &[give.as_raw_fd()],
        )
        .unwrap();
        drop(give);

        let mut f = recv::<Response>(&b).unwrap().unwrap();
        assert_eq!(f.msg, Response::Spawned { session: 9, pid: 1 });
        assert_eq!(f.fds.len(), 1);
        let mut got = unsafe {
            UnixStream::from_raw_fd(std::os::fd::IntoRawFd::into_raw_fd(f.fds.remove(0)))
        };
        keep.write_all(b"ping").unwrap();
        let mut buf = [0u8; 4];
        got.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
        got.write_all(b"pong").unwrap();
        keep.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"pong");
    }

    #[test]
    fn a_closed_peer_is_a_clean_end_not_an_error() {
        let (a, b) = pair();
        drop(a);
        assert!(recv::<Request>(&b).unwrap().is_none());
    }

    // A hostile peer announces a huge length: refused before allocating, nothing read.
    #[test]
    fn an_oversized_length_prefix_is_refused_without_reading_the_body() {
        let (mut a, b) = pair();
        a.write_all(&((MAX_MESSAGE as u32) + 1).to_be_bytes())
            .unwrap();
        let e = recv::<Request>(&b).err().unwrap();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        a.write_all(&u32::MAX.to_be_bytes()).unwrap();
        assert_eq!(
            recv::<Request>(&b).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_frame_at_the_limit_passes_and_sending_over_it_is_refused() {
        let (a, b) = pair();
        let big = Response::Error {
            reason: "x".repeat(MAX_MESSAGE),
        };
        assert_eq!(
            send(&a, &big, &[]).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        // Exactly MAX_MESSAGE bytes of body: `{"status":"error","reason":"..."}` minus its wrapper.
        let wrapper = serde_json::to_vec(&Response::Error {
            reason: String::new(),
        })
        .unwrap()
        .len();
        let fits = Response::Error {
            reason: "y".repeat(MAX_MESSAGE - wrapper),
        };
        let t = std::thread::spawn(move || send(&a, &fits, &[]).unwrap());
        let f = recv::<Response>(&b).unwrap().unwrap();
        t.join().unwrap();
        assert_eq!(f.len, MAX_MESSAGE);
    }

    #[test]
    fn garbage_and_truncated_bodies_are_errors() {
        let (mut a, b) = pair();
        a.write_all(&5u32.to_be_bytes()).unwrap();
        a.write_all(b"nojso").unwrap();
        assert_eq!(
            recv::<Request>(&b).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );

        let (mut a, b) = pair();
        a.write_all(&100u32.to_be_bytes()).unwrap();
        a.write_all(b"{\"op\"").unwrap();
        drop(a);
        assert_eq!(
            recv::<Request>(&b).err().unwrap().kind(),
            io::ErrorKind::UnexpectedEof
        );

        // An op the helper does not know (a shell exec, say) does not decode at all.
        let (mut a, b) = pair();
        let body = br#"{"op":"exec","cmd":"sh"}"#;
        a.write_all(&(body.len() as u32).to_be_bytes()).unwrap();
        a.write_all(body).unwrap();
        assert_eq!(
            recv::<Request>(&b).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
