import WebTorrent from "webtorrent"

import dotenv from "dotenv"

dotenv.config();

async function sha1(str: string) {
	const buf = new TextEncoder().encode(str);
	const hashBuf = await crypto.subtle.digest("SHA-1", buf);
	return Array.from(new Uint8Array(hashBuf)).map(b => b.toString(16).padStart(2, "0")).join("");
}

if (!process?.env?.SECRET)
    throw "No secret specified";

let client = new WebTorrent();
const infoHash = await sha1(process.env.SECRET);
const fakeTorrent = {
    infoHash,
    announce: [
        'wss://tracker.openwebtorrent.com',
        'wss://tracker.btorrent.xyz',
        'wss://tracker.fastcast.nz'
    ]
};
const torrent = client.add(fakeTorrent);