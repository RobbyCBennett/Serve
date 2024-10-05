use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::Path;


enum StatusCode
{
	BadRequest = 400,
	NotFound   = 404,
}


const HOSTNAME: &str = "localhost";
const PORT: u16 = 8080;

const PREFERRED_PUBLIC_DIR: &str = "public";

const READ_BUFFER_SIZE: usize = 4096;


fn main() -> std::io::Result<()>
{
	// Handle the interrupt signal
	unsafe { libc::signal(libc::SIGINT, handle_interrupt as libc::sighandler_t); }

	// Create a TCP listener or crash
	let listener = TcpListener::bind(format!("{HOSTNAME}:{PORT}"))?;

	// Use "public" if it exists, otherwise "."
	let public_dir = if Path::new(PREFERRED_PUBLIC_DIR).is_dir() {
		PREFERRED_PUBLIC_DIR
	} else {
		"."
	};

	// Print TCP port and public directory
	println!("http://{HOSTNAME}:{PORT}");
	println!("Serving files: {public_dir}");

	// Create a buffer to reuse
	let mut read_buffer = [0; READ_BUFFER_SIZE];
	let mut trash_buffer = [0; READ_BUFFER_SIZE];

	// Handle each stream
	for stream in listener.incoming() {
		match stream {
			Ok(stream) => handle_stream(public_dir, &mut read_buffer, &mut trash_buffer, stream),
			_ => (),
		}
	}

	return Ok(());
}


// When the interrupt signal is received, exit immediately
extern "C" fn handle_interrupt(_signal: libc::c_int)
{
	std::process::exit(0);
}


// Try to read a request and write a response
fn handle_stream(public_dir: &str, read_buffer: &mut [u8], trash_buffer: &mut [u8], mut stream: TcpStream)
{
	let stream = &mut stream;

	// Read the important part of the request into the read buffer
	match stream.read(read_buffer) {
		// Read the remainder into the trash buffer
		Ok(READ_BUFFER_SIZE) => loop {
			match stream.read(trash_buffer) {
				Ok(READ_BUFFER_SIZE) => continue,
				Ok(_) => break,
				Err(_) => return,
			}
		},
		Ok(_) => (),
		Err(_) => return,
	}

	// See a GET request or send an error response
	if !read_buffer.starts_with(b"GET /") {
		return send_response_simple(stream, StatusCode::BadRequest);
	}

	// Parse a path without .. or send an error response
	const START_OF_PATH: usize = 4;
	let mut end_of_path = READ_BUFFER_SIZE - 1;
	let mut last_byte_was_dot = false;
	for i in START_OF_PATH+1..READ_BUFFER_SIZE {
		match read_buffer[i] {
			b'.' => {
				if last_byte_was_dot {
					return send_response_simple(stream, StatusCode::BadRequest);
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

	// Parse the path as UTF-8 or send an error response
	let partial_path = &read_buffer[START_OF_PATH..end_of_path];
	let partial_path = match std::str::from_utf8(partial_path) {
		Ok(partial_path) => partial_path,
		Err(_) => return send_response_simple(stream, StatusCode::BadRequest),
	};

	// Concatenate the public directory, the path, and possibly index.html
	let mut path = String::from(public_dir);
	path.push_str(partial_path);
	let mut path = Path::new(&path).to_path_buf();
	if path.is_dir() {
		// To fix relative paths, redirect by adding a trailing slash
		if !partial_path.ends_with("/") {
			return send_response_redirect(stream, &format!("{partial_path}/"));
		}
		path = path.join("index.html");
	}

	// Get the content type or send an error response
	let content_type = match path.extension() {
		Some(os_str) => match os_str.to_str() {
			Some("html")  => "text/html",
			Some("css")   => "text/css",
			Some("js")    => "application/javascript",
			Some("svg")   => "image/svg+xml",
			Some("woff2") => "font/woff2",
			_ => return send_response_simple(stream, StatusCode::NotFound),
		},
		_ => return send_response_simple(stream, StatusCode::NotFound),
	};

	// Read file content or send an error response
	let content = match std::fs::read(&path) {
		Ok(content) => content,
		Err(_) => return send_response_simple(stream, StatusCode::NotFound),
	};

	// Finally send the file content
	send_response_content(stream, content_type, &content);
}


// Send a new simple response without any content
fn send_response_simple(stream: &mut TcpStream, status_code: StatusCode)
{
	let response: &str = match status_code {
		StatusCode::BadRequest => "HTTP/1.1 400 Bad Request\r\n\r\n",
		StatusCode::NotFound => "HTTP/1.1 404 Not Found\r\n\r\n",
	};

	let _ = stream.write_all(response.as_bytes());
}


// Send a new simple response without any content
fn send_response_redirect(stream: &mut TcpStream, location: &str)
{
	let response = format!("HTTP/1.1 308 Permanent Redirect\r\nLocation: {location}\r\n\r\n");

	let _ = stream.write_all(response.as_bytes());
}


// Send a new response with the given content
fn send_response_content(stream: &mut TcpStream, content_type: &str, content: &[u8])
{
	let content_length = content.len();

	let status_and_headers = format!(
		"HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\nContent-Type: {content_type}\r\n\r\n");

	if stream.write_all(status_and_headers.as_bytes()).is_err() {
		return;
	}

	let _ = stream.write_all(content);
}
