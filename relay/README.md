# CopyBlip relay

A Cloudflare Worker that connects CopyBlip devices on different networks. It passes encrypted bytes between them and can't read them. Each relayed connection runs the same end-to-end encrypted protocol as a direct one.

The relay sees only:

- **Room names:** derived from the group key, so they reveal nothing about it.
- **Device IDs:** random values, assigned when a device sets up.
- **Traffic:** when data flows, and how much.

## Deploy

You need a free [Cloudflare account](https://dash.cloudflare.com/sign-up).

```sh
cd relay
npm install
npx wrangler login     # opens a browser to sign in to Cloudflare
npx wrangler deploy
```

`deploy` prints the relay's address, such as `https://copyblip-relay.<you>.workers.dev`. Give that to your devices as described in [docs/away-from-home.md](../docs/away-from-home.md).

It runs on Cloudflare's free plan using Durable Objects. Idle connections hibernate, so a few devices stay well inside the free limits.

## How it works

Each group gets one Durable Object, a "room":

- **Computers listen.** They hold a WebSocket open, get told which other computers are listening, and get asked to accept pipes.
- **Opening a pipe.** A device opens a pipe by dialing a listening computer. When the computer accepts, the relay says `ready` to the dialer, then passes binary messages between the two ends until either closes.
- **Pairs of computers.** Of each pair, the one with the lower device ID dials the other, and only when they aren't already connected directly.
- **The iPhone** asks for the list of listening computers, then dials each.

The endpoints are documented at the top of [src/index.ts](src/index.ts).

## Local testing

```sh
npx wrangler dev --port 8787
```

Then point an agent at it with `copyblip run --relay http://127.0.0.1:8787`, or run the iOS tests through it:

```sh
COPYBLIP_TEST_RELAY=http://127.0.0.1:8787 ios/scripts/interop.sh
```
