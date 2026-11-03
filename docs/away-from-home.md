# Syncing away from home

At home, devices find each other on the Wi-Fi. On different networks, they need one of these:

| | Tailscale | Relay |
| --- | --- | --- |
| Setup | Install Tailscale on every device | Deploy the relay once, then point each device at it |
| Needs running | Tailscale's VPN, including on the iPhone | Nothing extra |
| Data path | Directly between your devices | Through Cloudflare, end-to-end encrypted |
| Cost | Free for personal use | Free tier |

You can use both. The iPhone tries direct connections first, including Tailscale addresses, and falls back to the relay if nothing answers within about a second.

Either way, the relay or Tailscale only ever carries encrypted data. Only devices holding your group's pairing code can read it.

## Tailscale

1. Install [Tailscale](https://tailscale.com) on each device and sign in with the same account.
2. Find each computer's Tailscale name or `100.x` address in the Tailscale app.
3. On each computer, open **CopyBlip › Settings** and add the other computers under **Computer addresses**. They apply right away.

   From a terminal, `copyblip peer add macmini` (or `100.64.0.1`, or `name:port`) does the same, but needs CopyBlip restarted.

4. On the iPhone, open **CopyBlip › Away From Home** and add each computer under **Computer Addresses**. Keep Tailscale's VPN on.

## Relay

Deploy the relay once, as described in [relay/README.md](../relay/README.md). Then:

1. On each computer, enter the relay's address in **CopyBlip › Settings › Relay** and click Save. It applies right away.

   From a terminal, `copyblip relay https://copyblip-relay.<you>.workers.dev` does the same, but needs CopyBlip restarted.

2. On the iPhone, either:
   - open **Add a Device** in CopyBlip on a computer and scan the QR code with the Camera app, which sets the relay too, or
   - enter the relay address under **CopyBlip › Away From Home › Relay**.

`copyblip relay` shows the current relay, and `copyblip relay --off` stops using it.
