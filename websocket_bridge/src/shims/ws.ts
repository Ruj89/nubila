/**
 * @internal
 */
export const WebSocket = window.WebSocket;
export type RawData = string
export class WebSocketServer{
    constructor(public config: {port: number}){}
}