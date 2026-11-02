// CopyBlip relay: connects devices that can't reach each other directly.
//
// Devices never send anything readable through here. Each pipe carries the
// same end-to-end encrypted protocol as a direct connection, so the relay
// sees only device IDs, timing and sizes. See docs/relay.md.
//
// A room is named by a value derived from the group key, so only members can
// find it. Inside a room:
//   GET /v1/rooms/:room/listen?device=ID  (WebSocket) Computers wait here for
//       pipes. They receive {"devices": [...]} whenever the list changes and
//       {"pipe": "...", "from": "..."} when someone dials them.
//   GET /v1/rooms/:room/devices           JSON list of listening device IDs.
//   GET /v1/rooms/:room/dial?device=ID&to=ID  (WebSocket) Opens a pipe to a
//       listening device. The relay sends "ready" once the other end joins,
//       and after that every binary message is passed through.
//   GET /v1/rooms/:room/accept?pipe=P     (WebSocket) The listener's end of a pipe.
import { DurableObject } from "cloudflare:workers";

interface Env {
  ROOMS: DurableObjectNamespace<Room>;
}

type Attachment =
  | { role: "listen"; device: string }
  | { role: "dial" | "accept"; pipe: string };

const ROUTE = /^\/v1\/rooms\/([0-9a-f]{32})\/(listen|dial|accept|devices)$/;
const DEVICE = /^[0-9A-Za-z-]{1,64}$/;

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const route = new URL(request.url).pathname.match(ROUTE);
    if (!route) {
      return new Response("CopyBlip relay\n", { status: 404 });
    }
    return env.ROOMS.get(env.ROOMS.idFromName(route[1])).fetch(request);
  },
} satisfies ExportedHandler<Env>;

export class Room extends DurableObject<Env> {
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    // Answers keepalives without waking the room.
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const action = url.pathname.split("/").pop();
    if (action === "devices") {
      return Response.json(this.devices());
    }
    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("expected a WebSocket\n", { status: 426 });
    }
    const device = url.searchParams.get("device") ?? "";
    const [client, server] = Object.values(new WebSocketPair());

    if (action === "listen") {
      if (!DEVICE.test(device)) return new Response("bad device\n", { status: 400 });
      // A computer that reconnects replaces its old connection.
      for (const old of this.ctx.getWebSockets(`device:${device}`)) {
        old.close(1000, "replaced");
      }
      this.accept(server, { role: "listen", device }, ["listen", `device:${device}`]);
      this.announce();
    } else if (action === "dial") {
      const to = url.searchParams.get("to") ?? "";
      const target = this.open(this.ctx.getWebSockets(`device:${to}`))[0];
      if (!DEVICE.test(device) || !target) {
        return new Response("that device isn't connected\n", { status: 404 });
      }
      const pipe = crypto.randomUUID();
      this.accept(server, { role: "dial", pipe }, [`pipe:${pipe}`]);
      target.send(JSON.stringify({ pipe, from: device }));
    } else if (action === "accept") {
      const pipe = url.searchParams.get("pipe") ?? "";
      const dialer = this.partner(pipe);
      if (!dialer) return new Response("that pipe is gone\n", { status: 404 });
      this.accept(server, { role: "accept", pipe }, [`pipe:${pipe}`]);
      dialer.send("ready");
    } else {
      return new Response("unknown action\n", { status: 404 });
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  webSocketMessage(socket: WebSocket, message: string | ArrayBuffer) {
    const info = socket.deserializeAttachment() as Attachment;
    if (info.role !== "listen") {
      this.partner(info.pipe, socket)?.send(message);
    }
  }

  webSocketClose(socket: WebSocket, code: number, reason: string) {
    this.closed(socket, code, reason);
  }

  webSocketError(socket: WebSocket) {
    this.closed(socket, 1011, "error");
  }

  private closed(socket: WebSocket, code: number, reason: string) {
    try {
      socket.close(code === 1005 || code === 1006 ? 1000 : code, reason);
    } catch {
      // Already closed.
    }
    const info = socket.deserializeAttachment() as Attachment;
    if (info.role === "listen") {
      this.announce();
    } else {
      // One end leaving ends the pipe, like a TCP connection.
      this.partner(info.pipe, socket)?.close(1000, "other end left");
    }
  }

  private accept(socket: WebSocket, info: Attachment, tags: string[]) {
    this.ctx.acceptWebSocket(socket, tags);
    socket.serializeAttachment(info);
  }

  private open(sockets: WebSocket[]): WebSocket[] {
    return sockets.filter((s) => s.readyState === WebSocket.OPEN);
  }

  private partner(pipe: string, self?: WebSocket): WebSocket | undefined {
    return this.open(this.ctx.getWebSockets(`pipe:${pipe}`)).find((s) => s !== self);
  }

  private devices(): string[] {
    return this.open(this.ctx.getWebSockets("listen")).map(
      (s) => (s.deserializeAttachment() as Attachment & { role: "listen" }).device,
    );
  }

  private announce() {
    const message = JSON.stringify({ devices: this.devices() });
    for (const socket of this.open(this.ctx.getWebSockets("listen"))) {
      socket.send(message);
    }
  }
}
