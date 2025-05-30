#![feature(let_chains, impl_trait_in_assoc_type)]

use std::{error::Error, str::FromStr, sync::Arc};

#[cfg(feature = "full")]
use async_compression::futures::bufread as async_comp;
use bytes::Bytes;
use cfg_if::cfg_if;
use futures_util::{future::Either, StreamExt, TryStreamExt};
use http::{
	header::{
		InvalidHeaderName, InvalidHeaderValue, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH,
		CONTENT_TYPE, LOCATION, USER_AGENT,
	},
	method::InvalidMethod,
	uri::{InvalidUri, InvalidUriParts},
	HeaderName, HeaderValue, Method, Request, Response,
};
use http_body::Body;
use http_body_util::{BodyDataStream, Full};
use hyper::{body::Incoming, Uri};
use hyper_util_wasm::client::legacy::Client;
use io_stream::{iostream_from_asyncrw, iostream_from_stream};
use js_sys::{Array, ArrayBuffer, Function, Object, Promise, Uint8Array};
use send_wrapper::SendWrapper;
use stream_provider::{
	ProviderWispTransportGenerator, ProviderWispTransportRead, ProviderWispTransportWrite,
	StreamProvider, StreamProviderService,
};
use thiserror::Error;
use utils::{
	asyncread_to_readablestream, convert_streaming_body, entries_of_object, from_entries,
	is_null_body, is_redirect, object_get, object_set, object_truthy, ws_protocol, StreamingBody,
	UriExt, WasmExecutor, WispTransportWrite,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{ResponseInit, Url, WritableStream};
#[cfg(feature = "full")]
use websocket::NubilaWebSocket;
use wisp_mux::{
	packet::{CloseReason, StreamType},
	WispError,
};
use ws_wrapper::WebSocketWrapper;

mod io_stream;
mod stream_provider;
mod tokioio;
mod utils;
#[cfg(feature = "full")]
mod websocket;
mod ws_wrapper;

#[wasm_bindgen(typescript_custom_section)]
const NUBILACLIENT_TYPES: &'static str = r#"
type NubilaIoStream = {
	read: ReadableStream<Uint8Array>,
	write: WritableStream<Uint8Array>,
};
type NubilaWispTransport = string | (() => { read: ReadableStream<ArrayBuffer>, write: WritableStream<Uint8Array> });
type NubilaWebSocketInput = string | ArrayBuffer;
type NubilaWebSocketHeadersInput = Headers | { [key: string]: string };
type NubilaUrlInput = string | URL;
"#;
#[wasm_bindgen]
extern "C" {
	#[wasm_bindgen(typescript_type = "NubilaWispTransport")]
	pub type NubilaWispTransport;
	#[wasm_bindgen(typescript_type = "NubilaIoStream")]
	pub type NubilaIoStream;
	#[wasm_bindgen(typescript_type = "NubilaWebSocketInput")]
	pub type NubilaWebSocketInput;
	#[wasm_bindgen(typescript_type = "NubilaWebSocketHeadersInput")]
	pub type NubilaWebSocketHeadersInput;
	#[wasm_bindgen(typescript_type = "NubilaUrlInput")]
	pub type NubilaUrlInput;
}

impl TryFrom<NubilaUrlInput> for Uri {
	type Error = NubilaError;
	fn try_from(value: NubilaUrlInput) -> Result<Self, Self::Error> {
		let value = JsValue::from(value);
		if let Some(value) = value.dyn_ref::<Url>() {
			value.href().try_into().map_err(NubilaError::from)
		} else if let Some(value) = value.as_string() {
			value.try_into().map_err(NubilaError::from)
		} else {
			Err(NubilaError::InvalidUrl(format!("{value:?}")))
		}
	}
}

#[derive(Debug, Error)]
pub enum NubilaError {
	#[error("Invalid DNS name: {0:?} ({0})")]
	InvalidDnsName(#[from] futures_rustls::rustls::pki_types::InvalidDnsNameError),
	#[error("Wisp: {0:?} ({0})")]
	Wisp(#[from] wisp_mux::WispError),
	#[error("Wisp server closed: {0} (IO error: {1:?} ({1}))")]
	WispCloseReason(CloseReason, std::io::Error),
	#[error("IO: {0:?} ({0})")]
	Io(#[from] std::io::Error),
	#[error("HTTP: {0:?} ({0})")]
	Http(#[from] http::Error),
	#[error("Hyper client: {0:?} ({0})")]
	HyperClient(#[from] hyper_util_wasm::client::legacy::Error),
	#[error("Hyper: {0:?} ({0})")]
	Hyper(#[from] hyper::Error),
	#[error("HTTP ToStr: {0:?} ({0})")]
	ToStr(#[from] http::header::ToStrError),
	#[error("Rustls: {0:?} ({0})")]
	Rustls(#[from] futures_rustls::rustls::Error),
	#[cfg(feature = "full")]
	#[error("Pemfile: {0:?} ({0})")]
	Pemfile(std::io::Error),
	#[cfg(feature = "full")]
	#[error("Webpki: {0:?} ({0})")]
	Webpki(#[from] webpki::Error),

	#[error("Wisp WebSocket failed to connect: {0}")]
	WebSocketConnectFailed(String),

	#[error("Custom Wisp transport: {0}")]
	WispTransport(String),
	#[error("Invalid Wisp transport: {0}")]
	InvalidWispTransport(String),
	#[error("Invalid Wisp transport packet: {0}")]
	InvalidWispTransportPacket(String),
	#[error("Wisp transport already closed")]
	WispTransportClosed,

	#[cfg(feature = "full")]
	#[error("Fastwebsockets: {0:?} ({0})")]
	FastWebSockets(#[from] fastwebsockets::WebSocketError),
	#[cfg(feature = "full")]
	#[error("Invalid websocket response status code: {0} != {1}")]
	WsInvalidStatusCode(u16, u16),
	#[cfg(feature = "full")]
	#[error("Invalid websocket upgrade header: {0:?} != \"websocket\"")]
	WsInvalidUpgradeHeader(String),
	#[cfg(feature = "full")]
	#[error("Invalid websocket connection header: {0:?} != \"Upgrade\"")]
	WsInvalidConnectionHeader(String),
	#[cfg(feature = "full")]
	#[error("Invalid websocket payload: {0}")]
	WsInvalidPayload(String),

	#[error("Invalid URL input: {0}")]
	InvalidUrl(String),
	#[error("Invalid URL scheme: {0:?}")]
	InvalidUrlScheme(Option<String>),
	#[error("No URL host found")]
	NoUrlHost,
	#[error("No URL port found")]
	NoUrlPort,
	#[error("Invalid request body")]
	InvalidRequestBody,
	#[error("Invalid request")]
	InvalidRequest,
	#[error("Invalid payload")]
	InvalidPayload,
	#[error("Failed to construct response headers object")]
	ResponseHeadersFromEntriesFailed,
	#[error("Failed to construct response object")]
	ResponseNewFailed,
	#[error("Failed to convert streaming body: {0}")]
	StreamingBodyConvertFailed(String),
	#[error("Failed to tee streaming body: {0}")]
	StreamingBodyTeeFailed(String),
	#[error("Failed to collect streaming body: {0:?} ({0})")]
	StreamingBodyCollectFailed(Box<dyn Error + Sync + Send>),
}

impl NubilaError {
	#[expect(clippy::needless_pass_by_value)]
	pub fn wisp_transport(value: JsValue) -> Self {
		if let Some(err) = value.dyn_ref::<js_sys::Error>() {
			Self::WispTransport(err.to_string().into())
		} else {
			Self::WispTransport(format!("{value:?}"))
		}
	}
}

impl From<NubilaError> for JsValue {
	fn from(value: NubilaError) -> Self {
		JsError::from(value).into()
	}
}

impl From<InvalidUri> for NubilaError {
	fn from(value: InvalidUri) -> Self {
		http::Error::from(value).into()
	}
}

impl From<InvalidUriParts> for NubilaError {
	fn from(value: InvalidUriParts) -> Self {
		http::Error::from(value).into()
	}
}

impl From<InvalidHeaderName> for NubilaError {
	fn from(value: InvalidHeaderName) -> Self {
		http::Error::from(value).into()
	}
}

impl From<InvalidHeaderValue> for NubilaError {
	fn from(value: InvalidHeaderValue) -> Self {
		http::Error::from(value).into()
	}
}

impl From<InvalidMethod> for NubilaError {
	fn from(value: InvalidMethod) -> Self {
		http::Error::from(value).into()
	}
}

enum NubilaResponse {
	Success(Response<Incoming>),
	Redirect((Response<Incoming>, http::Request<StreamingBody>)),
}

#[cfg(feature = "full")]
enum NubilaCompression {
	Brotli,
	Gzip,
}

// ugly hack. switch to serde-wasm-bindgen or a knockoff
cfg_if! {
	if #[cfg(feature = "full")] {
		#[wasm_bindgen]
		pub struct NubilaClientOptions {
			pub wisp_v2: bool,
			pub udp_extension_required: bool,
			pub title_case_headers: bool,
			pub ws_title_case_headers: bool,
			#[wasm_bindgen(getter_with_clone)]
			pub websocket_protocols: Vec<String>,
			pub redirect_limit: usize,
			pub header_limit: usize,
			#[wasm_bindgen(getter_with_clone)]
			pub user_agent: String,
			#[wasm_bindgen(getter_with_clone)]
			pub pem_files: Vec<String>,
			pub disable_certificate_validation: bool,
			pub buffer_size: usize,
		}
	} else {
		#[wasm_bindgen]
		pub struct NubilaClientOptions {
			pub wisp_v2: bool,
			pub udp_extension_required: bool,
			pub title_case_headers: bool,
			#[wasm_bindgen(getter_with_clone)]
			pub websocket_protocols: Vec<String>,
			pub redirect_limit: usize,
			pub header_limit: usize,
			#[wasm_bindgen(getter_with_clone)]
			pub user_agent: String,
			pub disable_certificate_validation: bool,
			pub buffer_size: usize,
		}
	}
}

#[wasm_bindgen]
impl NubilaClientOptions {
	#[wasm_bindgen(constructor)]
	pub fn new_default() -> Self {
		Self::default()
	}
}

impl Default for NubilaClientOptions {
	fn default() -> Self {
		Self {
			wisp_v2: false,
			udp_extension_required: false,
			title_case_headers: false,
			#[cfg(feature = "full")]
			ws_title_case_headers: true,
			websocket_protocols: Vec::new(),
			redirect_limit: 10,
			header_limit: 200,
			user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36".to_string(),
			#[cfg(feature = "full")]
			pem_files: Vec::new(),
			disable_certificate_validation: false,
			buffer_size: 16384,
        }
	}
}

#[wasm_bindgen(getter_with_clone)]
pub struct NubilaHandlers {
	pub onopen: Function,
	pub onclose: Function,
	pub onerror: Function,
	pub onmessage: Function,
}

#[cfg(feature = "full")]
#[wasm_bindgen]
impl NubilaHandlers {
	#[wasm_bindgen(constructor)]
	pub fn new(
		onopen: Function,
		onclose: Function,
		onerror: Function,
		onmessage: Function,
	) -> Self {
		Self {
			onopen,
			onclose,
			onerror,
			onmessage,
		}
	}
}

fn create_wisp_transport(function: Function) -> ProviderWispTransportGenerator {
	let wisp_transport = SendWrapper::new(function);
	Box::new(move |wisp_v2| {
		let wisp_transport = wisp_transport.clone();
		Box::pin(SendWrapper::new(async move {
			let transport = wisp_transport
				.call1(&JsValue::NULL, &wisp_v2.into())
				.map_err(NubilaError::wisp_transport)?;

			let transport = match transport.dyn_into::<Promise>() {
				Ok(transport) => {
					let fut = JsFuture::from(transport);
					fut.await.map_err(NubilaError::wisp_transport)?
				}
				Err(transport) => transport,
			}
			.into();

			let read = Box::pin(SendWrapper::new(
				wasm_streams::ReadableStream::from_raw(object_get(&transport, "read").into())
					.try_into_stream()
					.map_err(|x| NubilaError::wisp_transport(x.0.into()))?
					.map(|x| {
						let pkt = x
							.map_err(NubilaError::wisp_transport)
							.map_err(|x| WispError::WsImplError(Box::new(x)))?;
						let arr: ArrayBuffer = pkt.dyn_into().map_err(|x| {
							WispError::WsImplError(Box::new(
								NubilaError::InvalidWispTransportPacket(format!("{x:?}")),
							))
						})?;
						Ok::<Bytes, WispError>(Bytes::from(Uint8Array::new(&arr).to_vec()))
					}),
			)) as ProviderWispTransportRead;

			let write: WritableStream = object_get(&transport, "write").into();
			let write = Box::pin(WispTransportWrite(
				wasm_streams::WritableStream::from_raw(write)
					.try_into_sink()
					.map_err(|x| NubilaError::wisp_transport(x.0.into()))?,
			)) as ProviderWispTransportWrite;

			Ok((read, write))
		}))
	})
}

#[wasm_bindgen(inspectable)]
pub struct NubilaClient {
	stream_provider: Arc<StreamProvider>,
	client: Client<StreamProviderService, StreamingBody>,

	certs_tampered: bool,

	pub redirect_limit: usize,
	#[cfg(feature = "full")]
	header_limit: usize,
	#[cfg(feature = "full")]
	ws_title_case_headers: bool,
	#[wasm_bindgen(getter_with_clone)]
	pub user_agent: String,
	pub buffer_size: usize,
}

#[wasm_bindgen]
impl NubilaClient {
	#[wasm_bindgen(constructor)]
	pub fn new(
		transport: NubilaWispTransport,
		options: NubilaClientOptions,
	) -> Result<NubilaClient, NubilaError> {
		let stream_provider = if let Some(wisp_url) = transport.as_string() {
			let uri: Uri = wisp_url.clone().try_into()?;
			if uri.scheme_str() != Some("wss") && uri.scheme_str() != Some("ws") {
				return Err(NubilaError::InvalidUrlScheme(
					uri.scheme_str().map(ToString::to_string),
				));
			}

			let ws_protocols = options.websocket_protocols.clone();
			Arc::new(StreamProvider::new(
				Box::new(move |wisp_v2| {
					let wisp_url = wisp_url.clone();
					let mut ws_protocols = ws_protocols.clone();
					if wisp_v2 {
						// send some random data to ask the server for v2
						ws_protocols.push(ws_protocol());
					}

					Box::pin(async move {
						let (write, read) = WebSocketWrapper::connect(&wisp_url, &ws_protocols)?;
						while write.inner.ready_state() == 0 {
							if !write.wait_for_open().await {
								return Err(NubilaError::WebSocketConnectFailed(
									"websocket did not open".to_string(),
								));
							}
						}
						Ok((read.into_read(), write.into_write()))
					})
				}),
				&options,
			)?)
		} else if let Some(wisp_transport) = transport.dyn_ref::<Function>() {
			Arc::new(StreamProvider::new(
				create_wisp_transport(wisp_transport.clone()),
				&options,
			)?)
		} else {
			return Err(NubilaError::InvalidWispTransport(format!(
				"{:?}",
				JsValue::from(transport)
			)));
		};

		let service = StreamProviderService(stream_provider.clone());
		let mut builder = Client::builder(WasmExecutor);
		builder
			.http09_responses(true)
			.http1_title_case_headers(options.title_case_headers)
			.http1_max_headers(options.header_limit);

		#[cfg(feature = "full")]
		builder.http2_max_concurrent_reset_streams(10); // set back to default, on wasm it is 0
		let client = builder.build(service);

		Ok(Self {
			stream_provider,
			client,
			redirect_limit: options.redirect_limit,
			user_agent: options.user_agent,
			buffer_size: options.buffer_size,

			#[cfg(feature = "full")]
			header_limit: options.header_limit,
			#[cfg(feature = "full")]
			ws_title_case_headers: options.ws_title_case_headers,
			#[cfg(feature = "full")]
			certs_tampered: options.disable_certificate_validation || !options.pem_files.is_empty(),
			#[cfg(not(feature = "full"))]
			certs_tampered: options.disable_certificate_validation,
		})
	}

	pub async fn replace_stream_provider(&self) -> Result<(), NubilaError> {
		self.stream_provider.replace_client().await
	}

	#[cfg(feature = "full")]
	pub async fn connect_websocket(
		&self,
		handlers: NubilaHandlers,
		url: NubilaUrlInput,
		protocols: Vec<String>,
		headers: NubilaWebSocketHeadersInput,
	) -> Result<NubilaWebSocket, NubilaError> {
		NubilaWebSocket::connect(self, handlers, url, protocols, headers, &self.user_agent).await
	}

	pub async fn connect_tcp(&self, url: NubilaUrlInput) -> Result<NubilaIoStream, NubilaError> {
		let url: Uri = url.try_into()?;
		let host = url.host().ok_or(NubilaError::NoUrlHost)?;
		let port = url.port_u16().ok_or(NubilaError::NoUrlPort)?;
		let stream = self
			.stream_provider
			.get_asyncread(StreamType::Tcp, host.to_string(), port)
			.await?;
		Ok(iostream_from_asyncrw(
			Either::Right(stream),
			self.buffer_size,
		))
	}

	pub async fn connect_tls(&self, url: NubilaUrlInput) -> Result<NubilaIoStream, NubilaError> {
		let url: Uri = url.try_into()?;
		let host = url.host().ok_or(NubilaError::NoUrlHost)?;
		let port = url.port_u16().ok_or(NubilaError::NoUrlPort)?;
		let stream = self
			.stream_provider
			.get_tls_stream(host.to_string(), port, false)
			.await?;
		Ok(iostream_from_asyncrw(
			Either::Left(stream),
			self.buffer_size,
		))
	}

	pub async fn connect_udp(&self, url: NubilaUrlInput) -> Result<NubilaIoStream, NubilaError> {
		let url: Uri = url.try_into()?;
		let host = url.host().ok_or(NubilaError::NoUrlHost)?;
		let port = url.port_u16().ok_or(NubilaError::NoUrlPort)?;
		let stream = self
			.stream_provider
			.get_stream(StreamType::Udp, host.to_string(), port)
			.await?;
		Ok(iostream_from_stream(stream))
	}

	async fn send_req_inner(
		&self,
		req: http::Request<StreamingBody>,
		should_redirect: bool,
	) -> Result<NubilaResponse, NubilaError> {
		let new_req = if should_redirect {
			Some(req.clone())
		} else {
			None
		};

		let resp = self.client.request(req).await;
		match resp {
			Ok(resp) => {
				if is_redirect(resp.status().as_u16())
					&& let Some(mut new_req) = new_req
					&& let Some(location) = resp.headers().get(LOCATION)
					&& let Ok(redirect_url) = new_req.uri().get_redirect(location)
				{
					*new_req.uri_mut() = redirect_url;
					Ok(NubilaResponse::Redirect((resp, new_req)))
				} else {
					Ok(NubilaResponse::Success(resp))
				}
			}
			Err(err) => Err(err.into()),
		}
	}

	async fn send_req(
		&self,
		req: http::Request<StreamingBody>,
		should_redirect: bool,
	) -> Result<(hyper::Response<Incoming>, Uri, bool), NubilaError> {
		let mut redirected = false;
		let mut current_url = req.uri().clone();
		let mut current_resp: NubilaResponse = self.send_req_inner(req, should_redirect).await?;
		for _ in 0..self.redirect_limit {
			match current_resp {
				NubilaResponse::Success(_) => break,
				NubilaResponse::Redirect((_, req)) => {
					redirected = true;
					current_url = req.uri().clone();
					current_resp = self.send_req_inner(req, should_redirect).await?;
				}
			}
		}

		match current_resp {
			NubilaResponse::Redirect((resp, _)) | NubilaResponse::Success(resp) => {
				Ok((resp, current_url, redirected))
			}
		}
	}

	pub async fn fetch(
		&self,
		url: NubilaUrlInput,
		options: Object,
	) -> Result<web_sys::Response, NubilaError> {
		let url: Uri = url.try_into()?;
		// only valid `Scheme`s are HTTP and HTTPS, which are the ones we support
		url.scheme().ok_or(NubilaError::InvalidUrlScheme(
			url.scheme_str().map(ToString::to_string),
		))?;

		let request_method = object_get(&options, "method")
			.as_string()
			.unwrap_or_else(|| "GET".to_string());
		let request_method: Method = Method::from_str(&request_method)?;

		let request_redirect = !matches!(
			object_get(&options, "redirect")
				.as_string()
				.unwrap_or_default()
				.as_str(),
			"error" | "manual"
		);

		let mut body_content_type: Option<String> = None;
		let body = match object_truthy(object_get(&options, "body")) {
			Some(buf) => {
				let (body, content_type) = convert_streaming_body(buf)
					.await
					.map_err(|_| NubilaError::InvalidRequestBody)?;
				body_content_type = content_type;
				body.into_httpbody()?
			}
			None => http_body_util::Either::Right(Full::new(Bytes::new())),
		};

		let headers = object_truthy(object_get(&options, "headers")).and_then(|val| {
			if web_sys::Headers::instanceof(&val) {
				Some(entries_of_object(&from_entries(&val).ok()?))
			} else if val.is_truthy() {
				Some(entries_of_object(&Object::from(val)))
			} else {
				None
			}
		});

		let mut request_builder = Request::builder()
			.uri(url.clone())
			.method(request_method.clone());

		// Generic InvalidRequest because this only returns None if the builder has some error
		// which we don't know
		let headers_map = request_builder
			.headers_mut()
			.ok_or(NubilaError::InvalidRequest)?;

		cfg_if! {
			if #[cfg(feature = "full")] {
				headers_map.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, br"));
			} else {
				headers_map.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
			}
		}
		headers_map.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
		headers_map.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)?);

		if let Some(content_type) = body_content_type {
			headers_map.insert(CONTENT_TYPE, HeaderValue::from_str(&content_type)?);
		}

		if let Some(headers) = headers {
			for hdr in headers {
				if ["host"].contains(&hdr[0].to_lowercase().as_str()) {
					continue;
				}
				headers_map.insert(
					HeaderName::from_str(&hdr[0])?,
					HeaderValue::from_str(&hdr[1])?,
				);
			}
		}

		let body_empty = if let http_body_util::Either::Right(ref x) = body {
			x.size_hint().exact().unwrap_or(1) == 0
		} else {
			false
		};

		if matches!(request_method, Method::POST | Method::PUT | Method::PATCH) && body_empty {
			headers_map.insert(CONTENT_LENGTH, 0.into());
		}

		let (mut response, response_uri, redirected) = self
			.send_req(request_builder.body(body)?, request_redirect)
			.await?;

		if self.certs_tampered {
			response
				.headers_mut()
				.insert("X-Nubila-CertsTampered", HeaderValue::from_static("true"));
		}

		let response_headers: Array = response
			.headers()
			.iter()
			.filter_map(|val| {
				Some(Array::of2(
					&val.0.as_str().into(),
					&val.1.to_str().ok()?.into(),
				))
			})
			.collect();
		let response_headers = from_entries(&response_headers)
			.map_err(|_| NubilaError::ResponseHeadersFromEntriesFailed)?;

		let response_headers_raw = response.headers().clone();

		let response_builder = ResponseInit::new();
		response_builder.set_headers(&response_headers);
		response_builder.set_status(response.status().as_u16());
		response_builder.set_status_text(response.status().canonical_reason().unwrap_or_default());

		cfg_if! {
			if #[cfg(feature = "full")] {
				let response_stream = if !is_null_body(response.status().as_u16()) {
					let compression = match response
						.headers()
						.get("Content-Encoding")
						.and_then(|val| val.to_str().ok())
						.unwrap_or_default()
					{
						"gzip" => Some(NubilaCompression::Gzip),
						"br" => Some(NubilaCompression::Brotli),
						_ => None,
					};

					let response_body = BodyDataStream::new(response.into_body()).map_err(std::io::Error::other).into_async_read();
					let decompressed_body = match compression {
						Some(alg) => match alg {
							NubilaCompression::Gzip => {
								Either::Left(Either::Left(async_comp::GzipDecoder::new(response_body)))
							}
							NubilaCompression::Brotli => {
								Either::Left(Either::Right(async_comp::BrotliDecoder::new(response_body)))
							}
						},
						None => Either::Right(response_body),
					};
					Some(asyncread_to_readablestream(Box::pin(decompressed_body), self.buffer_size))
				} else {
					None
				};
			} else {
				let response_stream = if !is_null_body(response.status().as_u16()) {
					let response_body = BodyDataStream::new(response.into_body()).map_err(std::io::Error::other).into_async_read();
					Some(asyncread_to_readablestream(Box::pin(response_body), self.buffer_size))
				} else {
					None
				};
			}
		}

		let resp = web_sys::Response::new_with_opt_readable_stream_and_init(
			response_stream.as_ref(),
			&response_builder,
		)
		.map_err(|_| NubilaError::ResponseNewFailed)?;

		utils::define_property(&resp, "url", response_uri.to_string().into());
		utils::define_property(&resp, "redirected", redirected.into());

		let raw_headers = Object::new();
		for (k, v) in &response_headers_raw {
			let k = k.as_str();
			let v: JsValue = v.to_str()?.to_string().into();
			let jv = object_get(&raw_headers, k);
			if jv.is_array() {
				let arr = Array::from(&jv);
				arr.push(&v);
				object_set(&raw_headers, k, arr.into());
			} else if jv.is_truthy() {
				object_set(&raw_headers, k, Array::of2(&jv, &v).into());
			} else {
				object_set(&raw_headers, k, v);
			}
		}
		utils::define_property(&resp, "rawHeaders", raw_headers.into());

		Ok(resp)
	}
}
