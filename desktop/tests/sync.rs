use copyblip::clipboard::{Clipboard, MemoryClipboard};
use copyblip::hub::Hub;
use copyblip::protocol::{Message, Role, handshake};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

const KEY: [u8; 32] = [9; 32];

struct Node {
    hub: Arc<Hub>,
    clipboard: Arc<MemoryClipboard>,
    address: SocketAddr,
}

fn node(name: &str, key: [u8; 32]) -> Node {
    let clipboard = Arc::new(MemoryClipboard::default());
    let shared: Arc<dyn Clipboard> = clipboard.clone();
    let hub = Hub::new(key, name.to_owned(), name.to_owned(), shared);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    hub.serve(listener);
    hub.watch_clipboard();
    Node {
        hub,
        clipboard,
        address,
    }
}

fn connect(from: &Node, to: &Node) {
    from.hub.dial(Arc::new(Mutex::new(vec![to.address])));
}

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        sleep(Duration::from_millis(20));
    }
}

fn wait_for_clip(node: &Node, text: &str) {
    wait_for(text, || node.clipboard.current().as_deref() == Some(text));
}

/// Lets the 250 ms watcher notice an earlier change before the next one.
fn settle() {
    sleep(Duration::from_millis(400));
}

#[test]
fn copies_sync_both_ways_without_echoing() {
    let (a, b) = (node("a", KEY), node("b", KEY));
    connect(&a, &b);
    wait_for("connection", || a.hub.peer_count() == 1 && b.hub.peer_count() == 1);

    a.clipboard.copy("from a");
    wait_for_clip(&b, "from a");
    settle();
    b.clipboard.copy("from b");
    wait_for_clip(&a, "from b");
    settle();

    // One user copy and one applied clip each. An echo would add more.
    assert_eq!(a.clipboard.writes(), 2);
    assert_eq!(b.clipboard.writes(), 2);
}

#[test]
fn clips_are_forwarded_across_a_chain() {
    let (a, b, c) = (node("a", KEY), node("b", KEY), node("c", KEY));
    connect(&a, &b);
    connect(&c, &b);
    wait_for("connections", || b.hub.peer_count() == 2);

    a.clipboard.copy("across the chain");
    wait_for_clip(&c, "across the chain");
    settle();
    assert_eq!(b.clipboard.writes(), 1);
    assert_eq!(c.clipboard.writes(), 1);
}

#[test]
fn a_cycle_of_peers_applies_each_clip_once() {
    let (a, b, c) = (node("a", KEY), node("b", KEY), node("c", KEY));
    connect(&a, &b);
    connect(&b, &c);
    connect(&c, &a);
    wait_for("connections", || {
        a.hub.peer_count() == 2 && b.hub.peer_count() == 2 && c.hub.peer_count() == 2
    });

    a.clipboard.copy("once");
    wait_for_clip(&b, "once");
    wait_for_clip(&c, "once");
    settle();
    assert_eq!(a.clipboard.writes(), 1);
    assert_eq!(b.clipboard.writes(), 1);
    assert_eq!(c.clipboard.writes(), 1);
}

#[test]
fn other_groups_receive_nothing() {
    let (a, outsider) = (node("a", KEY), node("outsider", [8; 32]));
    connect(&outsider, &a);
    sleep(Duration::from_millis(300));
    assert_eq!(a.hub.peer_count(), 0);
    assert_eq!(outsider.hub.peer_count(), 0);

    a.clipboard.copy("private");
    settle();
    assert_eq!(outsider.clipboard.current(), None);
}

#[test]
fn connecting_does_not_overwrite_existing_clipboards() {
    let (a, b) = (node("a", KEY), node("b", KEY));
    a.clipboard.copy("old a");
    b.clipboard.copy("old b");
    settle();
    connect(&a, &b);
    wait_for("connection", || a.hub.peer_count() == 1);
    settle();
    assert_eq!(a.clipboard.current().as_deref(), Some("old a"));
    assert_eq!(b.clipboard.current().as_deref(), Some("old b"));
}

#[test]
fn keeps_dialing_until_the_peer_appears() {
    let a = node("a", KEY);
    let free_port = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = free_port.local_addr().unwrap();
    drop(free_port);
    a.hub.dial(Arc::new(Mutex::new(vec![address])));
    sleep(Duration::from_millis(600));
    assert_eq!(a.hub.peer_count(), 0);

    let clipboard = Arc::new(MemoryClipboard::default());
    let shared: Arc<dyn Clipboard> = clipboard.clone();
    let b = Hub::new(KEY, "b".to_owned(), "b".to_owned(), shared);
    b.serve(TcpListener::bind(address).unwrap());
    wait_for("connection", || a.hub.peer_count() == 1);

    a.clipboard.copy("still works");
    wait_for("clip", || clipboard.current().as_deref() == Some("still works"));
}

/// Talks to a hub the way the iPhone app does.
fn phone(to: &Node) -> (TcpStream, copyblip::protocol::Sealer, copyblip::protocol::Opener) {
    let mut stream = TcpStream::connect(to.address).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (mut sealer, mut opener) = handshake(&mut stream, &KEY, Role::Initiator).unwrap();
    let hello = Message::Hello {
        device: "phone".into(),
        name: "phone".into(),
    };
    sealer.send(&mut stream, &hello).unwrap();
    assert!(matches!(opener.recv(&mut stream).unwrap(), Message::Hello { .. }));
    (stream, sealer, opener)
}

#[test]
fn latest_returns_the_most_recent_clip_to_the_asker_only() {
    let (a, b) = (node("a", KEY), node("b", KEY));
    connect(&a, &b);
    wait_for("connection", || b.hub.peer_count() == 1);
    a.clipboard.copy("newest");
    wait_for_clip(&b, "newest");

    let (mut stream, mut sealer, mut opener) = phone(&b);
    wait_for("phone", || b.hub.peer_count() == 2);
    sealer.send(&mut stream, &Message::Latest).unwrap();
    match opener.recv(&mut stream).unwrap() {
        Message::Clip { text, .. } => assert_eq!(text, "newest"),
        other => panic!("expected a clip, got {other:?}"),
    }
    settle();
    assert_eq!(a.clipboard.writes(), 1);
}

#[test]
fn latest_with_nothing_copied_sends_nothing() {
    let a = node("a", KEY);
    let (mut stream, mut sealer, mut opener) = phone(&a);
    sealer.send(&mut stream, &Message::Latest).unwrap();
    stream.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    assert!(opener.recv(&mut stream).is_err());
}

#[test]
fn a_clip_from_the_phone_reaches_every_device() {
    let (a, b) = (node("a", KEY), node("b", KEY));
    connect(&a, &b);
    wait_for("connection", || b.hub.peer_count() == 1);
    let (mut stream, mut sealer, _) = phone(&b);
    let clip = Message::Clip {
        id: "from-phone".into(),
        text: "copied on iPhone".into(),
    };
    sealer.send(&mut stream, &clip).unwrap();
    wait_for_clip(&b, "copied on iPhone");
    wait_for_clip(&a, "copied on iPhone");
}

#[test]
fn text_already_on_the_clipboard_is_not_rewritten_or_forwarded() {
    let (a, b) = (node("a", KEY), node("b", KEY));
    connect(&a, &b);
    wait_for("connection", || b.hub.peer_count() == 1);
    a.clipboard.copy("same text");
    wait_for_clip(&b, "same text");
    settle();

    // The phone sends back what the computer already has.
    let (mut stream, mut sealer, mut opener) = phone(&b);
    wait_for("phone", || b.hub.peer_count() == 2);
    let echo = Message::Clip {
        id: "echo".into(),
        text: "same text".into(),
    };
    sealer.send(&mut stream, &echo).unwrap();
    settle();
    assert_eq!(b.clipboard.writes(), 1);
    assert_eq!(a.clipboard.writes(), 1);

    // It still counts as the latest clip, so the phone sees itself as up to date.
    sealer.send(&mut stream, &Message::Latest).unwrap();
    assert_eq!(opener.recv(&mut stream).unwrap(), echo);
}
