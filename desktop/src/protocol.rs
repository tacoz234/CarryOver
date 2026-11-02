//! Wire protocol. See docs/protocol.md for the byte-level description.
use anyhow::{Context, Result, bail, ensure};
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit,
    aead::{Aead, Payload},
};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::io::{Read, Write};

pub const GROUP_KEY_LEN: usize = 32;
pub const MAX_TEXT: usize = 1024 * 1024;
/// JSON escaping can grow text up to sixfold, so this leaves room for MAX_TEXT.
const MAX_FRAME: usize = 8 * 1024 * 1024;
const MAGIC: &[u8; 5] = b"CBLP\x01";
const NONCE_LEN: usize = 32;
const TAG_LEN: usize = 16;

pub type GroupKey = [u8; GROUP_KEY_LEN];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Initiator,
    Responder,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Message {
    Hello { device: String, name: String },
    Clip { id: String, text: String },
    /// Asks for the most recent clip. Phones send this when opened.
    Latest,
    Ping,
}

pub struct Sealer {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

pub struct Opener {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

fn frame_nonce(counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

impl Sealer {
    pub fn send<W: Write>(&mut self, writer: &mut W, message: &Message) -> Result<()> {
        let plaintext = serde_json::to_vec(message)?;
        let nonce = frame_nonce(self.counter);
        let sealed = self
            .cipher
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &plaintext,
                    aad: b"",
                },
            )
            .map_err(|_| anyhow::anyhow!("encryption failed"))?;
        ensure!(sealed.len() <= MAX_FRAME, "frame too large");
        self.counter += 1;
        let mut frame = Vec::with_capacity(4 + sealed.len());
        frame.extend_from_slice(&(sealed.len() as u32).to_be_bytes());
        frame.extend_from_slice(&sealed);
        writer.write_all(&frame)?;
        Ok(())
    }
}

impl Opener {
    pub fn recv<R: Read>(&mut self, reader: &mut R) -> Result<Message> {
        let mut len = [0u8; 4];
        reader.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len) as usize;
        ensure!((TAG_LEN..=MAX_FRAME).contains(&len), "bad frame length {len}");
        let mut sealed = vec![0u8; len];
        reader.read_exact(&mut sealed)?;
        let nonce = frame_nonce(self.counter);
        let plaintext = self
            .cipher
            .decrypt(
                (&nonce).into(),
                Payload {
                    msg: &sealed,
                    aad: b"",
                },
            )
            .map_err(|_| anyhow::anyhow!("frame failed authentication"))?;
        self.counter += 1;
        serde_json::from_slice(&plaintext).context("malformed message")
    }
}

fn cipher(key: &[u8]) -> Result<ChaCha20Poly1305> {
    ChaCha20Poly1305::new_from_slice(key).map_err(|_| anyhow::anyhow!("bad key length"))
}

/// Exchanges fresh nonces and derives one key per direction, so frames can't be
/// replayed into a later connection or reflected back at their sender.
pub fn handshake<S: Read + Write>(
    stream: &mut S,
    key: &GroupKey,
    role: Role,
) -> Result<(Sealer, Opener)> {
    let ours: [u8; NONCE_LEN] = rand::random();
    let mut hello = [0u8; MAGIC.len() + NONCE_LEN];
    hello[..MAGIC.len()].copy_from_slice(MAGIC);
    hello[MAGIC.len()..].copy_from_slice(&ours);
    stream.write_all(&hello)?;

    let mut reply = [0u8; MAGIC.len() + NONCE_LEN];
    stream.read_exact(&mut reply)?;
    if &reply[..MAGIC.len()] != MAGIC {
        bail!("peer is not speaking CopyBlip v1");
    }
    let theirs = &reply[MAGIC.len()..];

    let (initiator, responder) = match role {
        Role::Initiator => (&ours[..], theirs),
        Role::Responder => (theirs, &ours[..]),
    };
    let salt = [initiator, responder].concat();
    let mut okm = [0u8; 64];
    Hkdf::<Sha256>::new(Some(&salt), key)
        .expand(b"copyblip v1 session", &mut okm)
        .map_err(|_| anyhow::anyhow!("key derivation failed"))?;
    let (send, recv) = match role {
        Role::Initiator => (&okm[..32], &okm[32..]),
        Role::Responder => (&okm[32..], &okm[..32]),
    };
    Ok((
        Sealer {
            cipher: cipher(send)?,
            counter: 0,
        },
        Opener {
            cipher: cipher(recv)?,
            counter: 0,
        },
    ))
}

/// A non-secret tag advertised over mDNS so devices can skip other groups.
pub fn group_tag(key: &GroupKey) -> String {
    let mut tag = [0u8; 8];
    Hkdf::<Sha256>::new(None, key)
        .expand(b"copyblip v1 group tag", &mut tag)
        .expect("8 bytes is a valid HKDF length");
    hex::encode(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn pair(a: GroupKey, b: GroupKey) -> ((Sealer, Opener), (Sealer, Opener), TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            let session = handshake(&mut stream, &a, Role::Initiator).unwrap();
            (session, stream)
        });
        let (mut server, _) = listener.accept().unwrap();
        let server_session = handshake(&mut server, &b, Role::Responder).unwrap();
        let (client_session, client) = client.join().unwrap();
        (client_session, server_session, client, server)
    }

    fn clip(text: &str) -> Message {
        Message::Clip {
            id: "1".into(),
            text: text.into(),
        }
    }

    #[test]
    fn round_trips_both_directions() {
        let ((mut cs, mut co), (mut ss, mut so), mut c, mut s) = pair([7; 32], [7; 32]);
        cs.send(&mut c, &clip("to server")).unwrap();
        assert_eq!(so.recv(&mut s).unwrap(), clip("to server"));
        ss.send(&mut s, &clip("to client")).unwrap();
        assert_eq!(co.recv(&mut c).unwrap(), clip("to client"));
    }

    #[test]
    fn wrong_group_key_is_rejected() {
        let ((mut cs, _), (_, mut so), mut c, mut s) = pair([1; 32], [2; 32]);
        cs.send(&mut c, &clip("secret")).unwrap();
        assert!(so.recv(&mut s).is_err());
    }

    #[test]
    fn tampered_frame_is_rejected() {
        let ((mut cs, _), (_, mut so), _c, _s) = pair([3; 32], [3; 32]);
        let mut wire = Vec::new();
        cs.send(&mut wire, &clip("secret")).unwrap();
        let last = wire.len() - 1;
        wire[last] ^= 1;
        assert!(so.recv(&mut wire.as_slice()).is_err());
    }

    #[test]
    fn replayed_frame_is_rejected() {
        let ((mut cs, _), (_, mut so), _c, _s) = pair([4; 32], [4; 32]);
        let mut wire = Vec::new();
        cs.send(&mut wire, &clip("once")).unwrap();
        assert_eq!(so.recv(&mut wire.as_slice()).unwrap(), clip("once"));
        assert!(so.recv(&mut wire.as_slice()).is_err());
    }

    #[test]
    fn frame_from_old_session_is_rejected() {
        let ((mut old, _), _, _c1, _s1) = pair([5; 32], [5; 32]);
        let (_, (_, mut new), _c2, _s2) = pair([5; 32], [5; 32]);
        let mut wire = Vec::new();
        old.send(&mut wire, &clip("captured")).unwrap();
        assert!(new.recv(&mut wire.as_slice()).is_err());
    }

    #[test]
    fn own_frame_cannot_be_reflected_back() {
        let ((mut cs, mut co), _, _c, _s) = pair([6; 32], [6; 32]);
        let mut wire = Vec::new();
        cs.send(&mut wire, &clip("mine")).unwrap();
        assert!(co.recv(&mut wire.as_slice()).is_err());
    }

    #[test]
    fn group_tag_differs_by_key_and_hides_it() {
        assert_ne!(group_tag(&[1; 32]), group_tag(&[2; 32]));
        assert_eq!(group_tag(&[1; 32]).len(), 16);
    }

    /// Also checked by the iOS tests, so both sides must agree on it.
    #[test]
    fn group_tag_matches_reference_value() {
        assert_eq!(group_tag(&[1; 32]), "22953167989813ac");
    }
}
