use std::{str::from_utf8, sync::Arc};

use bytes::Bytes;
use fastwebsockets::{
	FragmentCollectorRead, Frame, OpCode, Payload, Role, WebSocket, WebSocketWrite,
};
use futures_util::lock::Mutex;
use http::{
	header::{
		CONNECTION, HOST, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION,
		UPGRADE, USER_AGENT,
	},
	Method, Request, Response, StatusCode, Uri,
};
use http_body_util::Empty;
use hyper::{
	body::Incoming,
	client::conn::http1,
	upgrade::{self, Upgraded},
};
use hyper_util_wasm::client::legacy::connect::ConnectSvc;
use js_sys::{ArrayBuffer, Function, Uint8Array};
use tokio::io::WriteHalf;
use wasm_bindgen::{prelude::*, JsError, JsValue};
use wasm_bindgen_futures::spawn_local;

use crate::{
	console_error,
	stream_provider::StreamProviderService,
	tokioio::TokioIo,
	utils::{entries_of_object, from_entries, ws_key},
	NubilaClient, NubilaError, NubilaHandlers, NubilaUrlInput, NubilaWebSocketHeadersInput,
	NubilaWebSocketInput,
};

#[wasm_bindgen]
pub struct NubilaWebSocket {
	tx: Arc<Mutex<WebSocketWrite<WriteHalf<TokioIo<Upgraded>>>>>,
	onerror: Function,
}

#[wasm_bindgen]
impl NubilaWebSocket {
	pub(crate) async fn connect(
		client: &NubilaClient,
		handlers: NubilaHandlers,
		url: NubilaUrlInput,
		protocols: Vec<String>,
		headers: NubilaWebSocketHeadersInput,
		user_agent: &str,
	) -> Result<Self, NubilaError> {
		let headers = JsValue::from(headers);
		let NubilaHandlers {
			onopen,
			onclose,
			onerror,
			onmessage,
		} = handlers;
		let onerror_cloned = onerror.clone();
		let ret: Result<NubilaWebSocket, NubilaError> = async move {
			let url: Uri = url.try_into()?;
			let host = url.host().ok_or(NubilaError::NoUrlHost)?;

			let mut req = Request::builder()
				.method(Method::GET)
				.uri(url.clone())
				.header(HOST, host)
				.header(CONNECTION, "Upgrade")
				.header(UPGRADE, "websocket")
				.header(SEC_WEBSOCKET_KEY, ws_key())
				.header(SEC_WEBSOCKET_VERSION, "13")
				.header(USER_AGENT, user_agent);

			if !protocols.is_empty() {
				req = req.header(SEC_WEBSOCKET_PROTOCOL, protocols.join(","));
			}

			if web_sys::Headers::instanceof(&headers)
				&& let Ok(entries) = from_entries(&headers)
			{
				for header in entries_of_object(&entries) {
					req = req.header(&header[0], &header[1]);
				}
			} else if headers.is_truthy() {
				for header in entries_of_object(&headers.into()) {
					req = req.header(&header[0], &header[1]);
				}
			}

			let req = req.body(Empty::new())?;

			let mut response = request(req, client).await?;
			verify(&response)?;

			let websocket = WebSocket::after_handshake(
				TokioIo::new(upgrade::on(&mut response).await?),
				Role::Client,
			);

			let (rx, tx) = websocket.split(tokio::io::split);

			let mut rx = FragmentCollectorRead::new(rx);
			let tx = Arc::new(Mutex::new(tx));

			let read_tx = tx.clone();
			let onerror_cloned = onerror.clone();

			spawn_local(async move {
				loop {
					match rx
						.read_frame(&mut |arg| async {
							read_tx.lock().await.write_frame(arg).await
						})
						.await
					{
						Ok(frame) => match frame.opcode {
							OpCode::Text => {
								if let Ok(str) = from_utf8(&frame.payload) {
									let _ = onmessage.call1(&JsValue::null(), &str.into());
								}
							}
							OpCode::Binary => {
								let _ = onmessage.call1(
									&JsValue::null(),
									&Uint8Array::from(frame.payload.to_vec().as_slice()).into(),
								);
							}
							OpCode::Close => {
								break;
							}
							// ping/pong/continue
							_ => {}
						},
						Err(err) => {
							let _ = onerror.call1(&JsValue::null(), &JsError::from(err).into());
							break;
						}
					}
				}
				let _ = onclose.call0(&JsValue::null());
			});

			let _ = onopen.call0(&JsValue::null());

			Ok(Self {
				tx,
				onerror: onerror_cloned,
			})
		}
		.await;

		match ret {
			Ok(ok) => Ok(ok),
			Err(err) => {
				let _ = onerror_cloned.call1(&JsValue::null(), &err.to_string().into());
				Err(err)
			}
		}
	}

	pub async fn send(&self, payload: NubilaWebSocketInput) -> Result<(), NubilaError> {
		let ret = if let Some(str) = payload.as_string() {
			self.tx
				.lock()
				.await
				.write_frame(Frame::text(Payload::Owned(str.as_bytes().to_vec())))
				.await
				.map_err(NubilaError::from)
		} else if let Some(binary) = payload.dyn_ref::<ArrayBuffer>() {
			self.tx
				.lock()
				.await
				.write_frame(Frame::binary(Payload::Owned(
					Uint8Array::new(binary).to_vec(),
				)))
				.await
				.map_err(NubilaError::from)
		} else {
			Err(NubilaError::WsInvalidPayload(format!(
				"{:?}",
				JsValue::from(payload)
			)))
		};

		match ret {
			Ok(ok) => Ok(ok),
			Err(err) => {
				let _ = self
					.onerror
					.call1(&JsValue::null(), &err.to_string().into());
				Err(err)
			}
		}
	}

	pub async fn close(&self, code: u16, reason: String) -> Result<(), NubilaError> {
		let ret = self
			.tx
			.lock()
			.await
			.write_frame(Frame::close(code, reason.as_bytes()))
			.await;
		match ret {
			Ok(ok) => Ok(ok),
			Err(err) => {
				let _ = self
					.onerror
					.call1(&JsValue::null(), &err.to_string().into());
				Err(err.into())
			}
		}
	}
}

async fn request(
	req: Request<Empty<Bytes>>,
	client: &NubilaClient,
) -> Result<Response<Incoming>, NubilaError> {
	let stream = StreamProviderService(client.stream_provider.clone())
		.connect(req.uri().clone())
		.await?;

	let (mut sender, conn) = http1::Builder::new()
		.title_case_headers(client.ws_title_case_headers)
		.max_headers(client.header_limit)
		.handshake(stream)
		.await?;

	spawn_local(async move {
		if let Err(err) = conn.with_upgrades().await {
			console_error!("nubila: websocket connection task failed: {:?}", err);
		}
	});

	Ok(sender.send_request(req).await?)
}

// https://github.com/snapview/tungstenite-rs/blob/314feea3055a93e585882fb769854a912a7e6dae/src/handshake/client.rs#L189
fn verify(response: &Response<Incoming>) -> Result<(), NubilaError> {
	if response.status() != StatusCode::SWITCHING_PROTOCOLS {
		return Err(NubilaError::WsInvalidStatusCode(
			response.status().as_u16(),
			StatusCode::SWITCHING_PROTOCOLS.as_u16(),
		));
	}

	let headers = response.headers();

	let upgrade_header = headers
		.get(UPGRADE)
		.and_then(|h| h.to_str().ok())
		.unwrap_or_default();

	if !upgrade_header.eq_ignore_ascii_case("websocket") {
		return Err(NubilaError::WsInvalidUpgradeHeader(
			upgrade_header.to_string(),
		));
	}

	let connection_header = headers
		.get(CONNECTION)
		.and_then(|h| h.to_str().ok())
		.unwrap_or_default();

	if !connection_header.eq_ignore_ascii_case("Upgrade") {
		return Err(NubilaError::WsInvalidConnectionHeader(
			connection_header.to_string(),
		));
	}

	Ok(())
}
