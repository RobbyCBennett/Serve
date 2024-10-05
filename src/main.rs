use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::Path;


const HOSTNAME: &str = "localhost";
const PORT: u16 = 8080;

const PREFERRED_PUBLIC_DIR: &str = "public";

const MAX_CONNECTIONS: usize = 16;
const READ_BUFFER_SIZE: usize = 256;


static mut RUNNING: bool = true;


fn main() -> std::io::Result<()>
{
	// Handle the interrupt signal
	unsafe { libc::signal(libc::SIGINT, handle_signal as libc::sighandler_t); }

	// Create a non-blocking TCP listener or stop
	let listener = TcpListener::bind(format!("{HOSTNAME}:{PORT}"))?;
	listener.set_nonblocking(true)?;

	// Use "public" if it exists, otherwise "."
	let public_dir = if Path::new(PREFERRED_PUBLIC_DIR).is_dir() {
		PREFERRED_PUBLIC_DIR
	} else {
		"."
	};

	// Print TCP port and public directory
	println!("http://{HOSTNAME}:{PORT}");
	println!("Serving files: {public_dir}");

	// Many TCP streams as they arrive, with one read bufffer and trash buffer for them all
	let mut streams = Vec::<TcpStream>::with_capacity(MAX_CONNECTIONS);
	let mut read_buffer  = [b'0'; READ_BUFFER_SIZE];
	let mut trash_buffer = [b'0'; READ_BUFFER_SIZE];

	// Keep each incoming stream
	for stream in listener.incoming() {
		if unsafe { !RUNNING } {
			return Ok(());
		}

		// If there's a new stream, enough space, and it can be non-blocking, keep it
		if streams.len() < MAX_CONNECTIONS {
			match stream {
				Ok(stream) => match stream.set_nonblocking(true) {
					Ok(()) => streams.push(stream),
					_ => (),
				}
				_ => (),
			}
		}

		// Read/write each stream, removing the ones that don't exist
		streams.retain_mut(|stream|
			read_and_write(public_dir, &mut read_buffer, &mut trash_buffer, stream));
	}

	return Ok(());
}


// When a specific signal is received, remember to stop running
extern "C" fn handle_signal(_signal: libc::c_int)
{
	unsafe { RUNNING = false; }
}


// Handle each stream by trying to read a request and write a response, returning whether the stream exists
fn read_and_write(public_dir: &str, read_buffer: &mut [u8], trash_buffer: &mut [u8], stream: &mut TcpStream) -> bool
{
	// Read the first part of the request or stop
	match stream.read(read_buffer) {
		Err(_) => return true,
		Ok(0) => return false,
		_ => (),
	}

	// Read the rest of the request into the trash buffer
	loop {
		if unsafe { !RUNNING } {
			return false;
		}

		// Read
		match stream.read(trash_buffer) {
			Ok(0) => return false,
			Ok(_) => (),
			Err(_) => break,
		}
	}

	// See a GET request or send error response
	if !read_buffer.starts_with(b"GET /") {
		send_response_simple(stream, 400);
		return true;
	}

	// Parse a path without .. or send error response
	const START_OF_PATH: usize = 4;
	let mut end_of_path = READ_BUFFER_SIZE - 1;
	let mut last_byte_was_dot = false;
	for i in START_OF_PATH+1..READ_BUFFER_SIZE {
		match read_buffer[i] {
			b'.' => {
				if last_byte_was_dot {
					send_response_simple(stream, 400);
					return true;
				}
				last_byte_was_dot = true;
			},
			b' ' | b'?' | b'#' => {
				end_of_path = i;
				break;
			},
			_ => {
				last_byte_was_dot = false;
			},
		}
	}

	// Parse the path as UTF-8 or send error response
	let partial_path = &read_buffer[START_OF_PATH..end_of_path];
	let partial_path = match std::str::from_utf8(partial_path) {
		Ok(partial_path) => partial_path,
		Err(_) => {
			send_response_simple(stream, 400);
			return true;
		},
	};

	// Concatenate the public directory, the path, and possibly index.html
	let mut path = String::from(public_dir);
	path.push_str(partial_path);
	let mut path = Path::new(&path).to_path_buf();
	if path.is_dir() {
		// To fix relative paths, redirect by adding a trailing slash
		if !partial_path.ends_with("/") {
			send_response_redirect(stream, &format!("{partial_path}/"));
			return true;
		}
		path = path.join("index.html");
	}

	// Get content type or send error response
	let content_type = match path.extension() {
		Some(os_str) => {
			match os_str.to_str() {
				Some("html")  => "text/html",
				Some("css")   => "text/css",
				Some("js")    => "application/javascript",
				Some("svg")   => "image/svg+xml",
				Some("woff2") => "font/woff2",
				_ => "",
			}
		},
		_ => "",
	};
	if content_type.len() == 0 {
		send_response_simple(stream, 404);
		return true;
	}

	// Read file content or send error response
	let content = match std::fs::read(&path) {
		Ok(content) => content,
		Err(_) => {
			send_response_simple(stream, 404);
			return true;
		},
	};

	// Finally send the file content
	send_response_content(stream, content_type, &content);
	return true;
}


// Send a new simple response without any content
fn send_response_simple(stream: &mut TcpStream, code: u16)
{
	let response_status_text: &str = match code {
		400 => "Bad Request",
		404 => "Not Found",
		_ => "",
	};

	let response = format!("HTTP/1.1 {code} {response_status_text}\r\n\r\n");

	if stream.write_all(response.as_bytes()).is_ok() {
		let _ = stream.flush();
	}
}


// Send a new simple response without any content
fn send_response_redirect(stream: &mut TcpStream, location: &str)
{
	let response = format!("HTTP/1.1 308 Permanent Redirect\r\nLocation: {location}\r\n\r\n");

	if stream.write_all(response.as_bytes()).is_ok() {
		let _ = stream.flush();
	}
}


// Send a new response with the given content
fn send_response_content(stream: &mut TcpStream, content_type: &str, content: &[u8])
{
	let content_length = content.len();

	let status_and_headers = format!(
		"HTTP/1.1 200 OK\r\n\
		Content-Length: {content_length}\r\n\
		Content-Type: {content_type}\r\n\
		\r\n"
	);

	if stream.write_all(status_and_headers.as_bytes()).is_ok() {
		if stream.write_all(content).is_ok() {
			let _ = stream.flush();
		}
	}
}
