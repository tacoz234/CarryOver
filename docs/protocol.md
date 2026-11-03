# Protocol v1

Devices in a group share a 32-byte group key. The pairing code is that key in hex.

## Discovery

mDNS service `_copyblip._tcp.local.` with TXT records:

- `device`: random device ID
- `group`: first 8 bytes of `HKDF-SHA256(ikm = key, no salt, info = "copyblip v1 group tag")`, in hex. Not secret, it only lets devices skip other groups.

The default port is TCP 47820.

## Handshake

Each side sends `"CBLP" 0x01` followed by 32 random bytes, then reads the same from the peer.

```
okm = HKDF-SHA256(ikm = key, salt = initiator_random || responder_random,
                  info = "copyblip v1 session", length = 64)
initiator -> responder key = okm[0..32]
responder -> initiator key = okm[32..64]
```

## Frames

```
u32 big-endian ciphertext length (16 to 24 MiB)
ChaCha20-Poly1305 ciphertext and tag
```

The nonce is 4 zero bytes followed by a big-endian `u64` frame counter, which starts at 0 for each direction. There's no associated data. A frame that fails authentication ends the connection.

## Messages

The plaintext of each frame is one JSON object:

```json
{"t":"hello","device":"<id>","name":"<display name>","devices":[<presence>, ...]}
{"t":"clip","id":"<random hex>","text":"<up to 1 MiB>"}
{"t":"image","id":"<random hex>","png":"<base64 PNG, up to 16 MiB>"}
{"t":"latest"}
{"t":"devices","devices":[<presence>, ...]}
{"t":"ping"}
```

A presence is `{"device":"<id>","name":"<display name>","connected":true,"last_seen":<unix seconds>}`: a device the sender knows about, whether the sender is connected to it now, and when it last was.

Devices skip message types and fields they don't recognize, so newer versions can add them without breaking older ones.

Each side's first frame is `hello`. Nothing else is sent until the peer's `hello` authenticates, which proves it holds the key.

## Sync rules

- A `clip` or `image` with an ID that's already been seen is dropped. Otherwise it's applied and forwarded to other connections. If the content is already on the clipboard, it's neither rewritten nor forwarded.
- When a copy holds both text and an image, only the text is sent.
- The most recent clip to arrive wins. Device clocks aren't used, so a wrong clock can't block a device.
- Nothing is sent on startup or connect, only after a new copy.
- `latest` asks for the most recent clip, which is sent back to the asker only, with its original ID. The iPhone app sends this when opened. If nothing has been copied yet, there's no reply.
- Computers send their known devices in `hello`, and send `devices` to every connection whenever one of their own connections starts or ends. A device counts as online if it's connected directly, or if a directly connected computer reports it as connected. Only direct reports count, so peers can't keep a departed device online by repeating each other. The iPhone sends no list.
- Pings go out after 15 seconds of silence, and a connection is dropped after 45 seconds without a frame.

## Relay

Devices on different networks can connect through a relay; see [relay/README.md](../relay/README.md). A relayed pipe carries exactly the same bytes as a TCP connection, split into WebSocket binary messages of at most 256 KiB.

The relay room is the first 16 bytes of `HKDF-SHA256(ikm = key, no salt, info = "copyblip v1 relay room")`, in hex.

## Invite links

`copyblip://join?code=<pairing code>&relay=<https URL>`. The relay is optional. `copyblip invite` shows this as a QR code, and the iPhone app opens it after asking.

## Known limits

- The group key is long-lived, so there's no forward secrecy: anyone who later gets the key can decrypt recorded traffic.
- A single device can't be removed from a group. Start a new group instead.
