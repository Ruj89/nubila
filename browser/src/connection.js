let postponedPayload = null;
let peer;

const $ = window.$;
const SimplePeer = window.SimplePeer;
const WebTorrent = window.WebTorrent;
import bencode from "https://cdn.jsdelivr.net/npm/bencode@4.0.0/+esm"

class WebRtcExtension {
	constructor(wire) {
		this._wire = wire
	}

	onMessage(payload) {
		try {
			let decodedPayload = bencode.decode(payload.toString(), 'utf8')
			const msg = JSON.parse(decodedPayload);
			if (msg.type === 'signal') {
				console.log(`📥 New SDP received via ${this.name}:`, msg.sdp);
				peer.signal(msg.sdp);
			}
		} catch (err) {
			console.warn("⚠️ Error decoding payload:", err);
		}
	}
}

function createWebRtcExtension() {
	let extension = WebRtcExtension;
	extension.prototype.name = 'webrtc_extension'
	return extension
}

async function sha1(str) {
	const buf = new TextEncoder().encode(str);
	const hashBuf = await crypto.subtle.digest("SHA-1", buf);
	return Array.from(new Uint8Array(hashBuf)).map(b => b.toString(16).padStart(2, "0")).join("");
}

async function openChannel(initiator, secret) {
	const infoHash = await sha1(secret);

	const client = new WebTorrent();
	const fakeTorrent = {
		infoHash,
		announce: [
			'ws://localhost:8000'
		]
	};

	peer = new SimplePeer({ initiator, trickle: false });

	peer.on('signal', sdp => {
		const payload = JSON.stringify({ type: 'signal', sdp });
		if (torrent.wires.length == 0) {
			console.log("⌛ Postponed payload to wire");
			postponedPayload = payload;
		} else
			torrent.wires.forEach(wire => {
				if (!!wire.peerExtendedHandshake?.m?.webrtc_extension) {
					console.log(`📨 Sending ${sdp.type} payload to wire`);
					wire.extended('webrtc_extension', payload);
				}
			});
	});

	peer.on('connect', () => {
		console.log("✅ WebRTC P2P connection established!");
		peer.send(`👋 Hello peer from ${initiator ? "client" : "server"}!`);
	});

	peer.on('error', err => console.error('🚩 Error:', err));

	peer.on('iceStateChange', state => console.log('ICE state:', state));

	peer.on('close', () => console.log('🚫 Connection closed'));

	peer.on('data', data => {
		console.log("📩 Received message:", data.toString());
	});

	const torrent = client.add(fakeTorrent);

	torrent.on('wire', wire => {
		console.log("🤝 Connected to a new peer via WebTorrent");
		wire.use(createWebRtcExtension());

		wire.on('extended', () => {
			if (postponedPayload != null && !!wire.peerExtendedHandshake?.m?.webrtc_extension) {
				console.log(`📨 Sending ${postponedPayload.type} payload to wire`);
				wire.extended('webrtc_extension', postponedPayload)
				postponedPayload = null;
			}
		});
	});
}

function configureConnection() {
	$("form#connect-form").submit((e) => {
		(async () => {
			postponedPayload = null;
			const secret = $("#secret").val();
			if ((secret ?? "") == "") {
				console.error("🚩 No secret inserted");
				return false;
			}

			let initiator = false;
			if (e?.originalEvent?.submitter?.name == "listen")
				initiator = false;
			else if (e?.originalEvent?.submitter?.name == "connect")
				initiator = true;
			else {
				console.error("🚩 No valid action specified");
				return false;
			}

			await openChannel(initiator, secret);
		})();
		return false
	});

	if ('serviceWorker' in navigator) {
		navigator.serviceWorker.register('/sw.js')
			.then(registration => {
				console.log('✅ Service Worker registered successfully:', registration);
			})
			.catch(error => {
				console.error('🚩 Error in Service Worker registration:', error);
			});
	}
}

configureConnection();
