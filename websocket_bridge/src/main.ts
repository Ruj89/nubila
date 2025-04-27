import dotenv from "dotenv"
import { joinRoom } from "trystero/torrent"
import { RTCPeerConnection } from 'node-datachannel/polyfill';
import { WebSocketServer, WebSocket, RawData } from 'ws';
import { DataPayload, JsonValue } from "trystero";

function rawToBuffer(data: RawData): Buffer {
    if (Buffer.isBuffer(data)) {
        return data;
    }
    if (data instanceof ArrayBuffer) {
        return Buffer.from(data);
    }
    if (Array.isArray(data)) {
        return Buffer.concat(data);
    }
    return Buffer.from(data as string, 'utf8');
}

await (async function main() {
    dotenv.config();

    let pendingSockets: WebSocket[] = [];
    let isServer = true;
    let ws: WebSocket;

    let quit: () => void;
    let running = new Promise<void>((res) => {
        quit = res;
    });

    const config = {
        appId: "nubila",
        rtcPolyfill: RTCPeerConnection,
        password: "o2g7s~ybqk2Mq2&k"
    };
    const room = joinRoom(config, `yoyodyne1`);
    room.onPeerJoin(peerId => {
        console.log(`WebRTC: 🤝 ${peerId} joined, channel enabled`)
    });
    room.onPeerLeave(peerId => {
        if (isServer)
            console.log(`WebRTC: 🚪 ${peerId} left`);
        else {
            console.log(`WebRTC: 😅 Server ${peerId} left, why I'm still here? Hey server, something wrong?`);
            quit();
        }
    });
    let [sendRTCMessage, receiveRTCMessage] = room.makeAction("message")
    let [activateConnection, onConnection] = room.makeAction("connect")

    async function graceful(code: number) {
        try {
            console.log("🛑 Quitting room");
            await room.leave();
        }
        catch (e) { console.error('❌ Error closing: ', e); code = 1; }
        finally { process.exit(code); }
    }

    process
        .once('SIGINT', () => graceful(0))
        .once('SIGTERM', () => graceful(0))
        .once('exit', (c) => console.log(`👋 Closing application (${c})`))
        .once('uncaughtException', (err) => { console.error(err); graceful(1); });

    receiveRTCMessage((data: DataPayload, peerId, metadata) => {
        console.log(`WebRTC: 📩 Received from peer ${peerId}`);
        const messageId = <number>(<{ [key: string]: JsonValue }>metadata)?.id
        if (isServer) {
            if (messageId && pendingSockets.at(messageId - 1))
                pendingSockets[messageId - 1].send(<string>data);
        } else {
            ws.send(Buffer.from(<Uint8Array>data))
        }
    })
    onConnection((data: DataPayload, peerId, metadata) => {
        if (!isServer) {
            console.log(`WebRTC: 📩 Received WebSocket connection request from peer ${peerId}`);
            const id = <number>(<{ [key: string]: JsonValue }>metadata)?.id
            ws = new WebSocket("ws://localhost:1337");
            ws.on('open', () => {
                console.log(`WS: ✅ WebSocket client connected to ws://localhost:1337`);
            })
            ws.on('message', (data) => {
                console.log(`WS: 📩 Received through WebSocket from server, sending through WebRTC`);
                sendRTCMessage(rawToBuffer(data), undefined, { id })
            })
        }
    })

    const wss = new WebSocketServer({ port: 8080 });
    wss.on('listening', () => {
        console.log(`WS: ✅ Server is listening on ws://localhost:8080`);
        isServer = true;
    });
    wss.on('connection', (socket: WebSocket, request) => {
        const ip = request.socket.remoteAddress;
        console.log(`WS: 🤝 New client connected, activating remote WS connection though WebRTC message: ${ip}`);
        const id = pendingSockets.push(socket);
        activateConnection(Buffer.from("activate"), undefined, { id });

        socket.on('message', (data) => {
            console.log(`WS: 📩 Received from WebSocket ${ip}`);
            const id = pendingSockets.push(socket);
            sendRTCMessage(rawToBuffer(data), undefined, { id });
        });

        socket.on('close', (code, _) => {
            console.log(`WS: 🚪 Client ${ip} disconnected (${code})`);
        });

        socket.on('error', (err) => {
            console.error(`WS: ❌ Connection error with ${ip}:`, err);
            quit();
        });
    });
    wss.on('error', (e: any) => {
        if (e?.code != "EADDRINUSE")
            throw e;

        console.log("WS: 😴 Assuming server is already started");
        isServer = false;
    });

    console.log("WebRTC: ⏳ Waiting for peers on torrent");
    await running;

    graceful(0)
})();