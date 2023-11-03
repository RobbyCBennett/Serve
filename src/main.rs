use std::{
	io::prelude::*,
	net::{
		TcpListener,
		TcpStream,
	},
	path::Path,
};


const PORT: u16 = 8080;
const PREFERRED_PUBLIC_DIR: &str = "public";

const MAX_CONNECTIONS: usize = 16;
const READ_BUFFER_SIZE: usize = 256;


#[allow(non_upper_case_globals)]
static mut running: bool = true;


fn main() -> std::io::Result<()>
{
	// Set signal handler
	unsafe {
		libc::signal(libc::SIGINT,  handle_signal as usize);
	}

	// Create a non-blocking TCP listener or stop
	let listener = TcpListener::bind(format!("127.0.0.1:{PORT}"));
	if listener.is_err() {
		return Err(listener.err().unwrap());
	}
	let listener = listener.unwrap();
	let non_blocking = listener.set_nonblocking(true);
	if non_blocking.is_err() {
		return Err(non_blocking.err().unwrap());
	}

	// Use "public" if it exists, otherwise "."
	let public_dir = if Path::new(PREFERRED_PUBLIC_DIR).is_dir() {
		PREFERRED_PUBLIC_DIR
	} else {
		"."
	};

	// Print TCP port and public directory
	println!("http://localhost:{PORT}");
	println!("Serving files: {public_dir}");

	// Many TCP streams as they arrive, with one read bufffer and trash buffer for them all
	let mut streams = Vec::<TcpStream>::with_capacity(MAX_CONNECTIONS);
	let mut read_buffer  = [b'0'; READ_BUFFER_SIZE];
	let mut trash_buffer = [b'0'; READ_BUFFER_SIZE];

	// Keep each incoming stream
	for stream in listener.incoming() {
		unsafe {
			if !running {
				return Ok(());
			}
		}

		// If there's a new stream, enough space, and it can be non-blocking, keep it
		if !stream.is_err() && streams.len() < MAX_CONNECTIONS {
			let stream = stream.unwrap();
			if stream.set_nonblocking(true).is_ok() {
				streams.push(stream);
			}
		}

		// Read/write each stream, removing the ones that don't exist
		streams.retain_mut(|stream|
			read_and_write(public_dir, &mut read_buffer, &mut trash_buffer, stream));
	}

	return Ok(());
}


// When a specific signal is received, remember to stop running
fn handle_signal()
{
	unsafe { running = false; }
}


// Handle each stream by trying to read a request and write a response, returning whether the stream exists
fn read_and_write(public_dir: &str, read_buffer: &mut [u8], trash_buffer: &mut [u8], stream: &mut TcpStream) -> bool
{
	// Read the first part of the request or stop
	let byte_count = stream.read(read_buffer);
	if byte_count.is_err() {
		return true;
	}
	if byte_count.unwrap() == 0 {
		return false;
	}

	// Read the rest of the request into the trash buffer
	loop {
		unsafe {
			if !running {
				return false;
			}
		}

		// Read
		let byte_count = stream.read(trash_buffer);
		if byte_count.is_err() {
			break;
		}
		if byte_count.unwrap() == 0 {
			return false;
		}
	}

	// See a GET request or send error response
	if !read_buffer.starts_with(b"GET /") {
		match std::str::from_utf8(read_buffer) {
			Ok(s) => println!("{s}\n"),
			_ => println!("UTF-8 ERROR\n"),
		}
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
					println!("2\n");
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
	let partial_path = std::str::from_utf8(partial_path);
	if partial_path.is_err()	{
		send_response_simple(stream, 400);
		return true;
	}

	// Concatenate the public directory, the path, and possibly index.html
	let partial_path = partial_path.unwrap();
	let mut path = String::from(public_dir);
	path.push_str(partial_path);
	if path.ends_with("/") {
		path.push_str("index.html");
	}

	// Get content type or send error response
	let content_type = match Path::new(&path).extension() {
		Some(os_str) => {
			match os_str.to_str() {
				Some("html") => "text/html",
				Some("css")  => "text/css",
				Some("js")   => "application/javascript",
				Some("svg")  => "image/svg+xml",
				Some("ttf")  => "font/ttf",
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
	let content_result = std::fs::read(&path);
	if content_result.is_err() {
		send_response_simple(stream, 404);
		return true;
	}
	let content = content_result.unwrap();

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

	let response = format!(
		"HTTP/1.1 {code} {response_status_text}\r\n\
		Content-Length: 0\r\n\
		\r\n");

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
