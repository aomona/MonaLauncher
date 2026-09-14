use reqwest::blocking::Response;
use std::io::{self, Read};

/// Keep each caller's size limit and error while bounding responses with or without a length.
pub(super) fn read_bounded<E: From<io::Error>>(
    response: Response,
    maximum: u64,
    too_large: E,
) -> Result<Vec<u8>, E> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err(too_large);
    }
    let mut bytes = Vec::new();
    response.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(too_large);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    #[test]
    fn preserves_size_limits_and_read_errors() {
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        for (header, body, expected) in [
            ("Content-Length: 4\r\n", "test", "ok"),
            ("Content-Length: 5\r\n", "", "limit"),
            ("", "test", "ok"),
            ("", "large", "limit"),
            ("Content-Length: 4\r\n", "x", "io"),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                for line in io::BufReader::new(&mut stream).lines() {
                    if line.unwrap().is_empty() {
                        break;
                    }
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\n{header}Connection: close\r\n\r\n{body}"
                )
                .unwrap();
            });
            let response = client.get(url).send().unwrap();
            let result = read_bounded(response, 4, io::Error::other("limit"));
            server.join().unwrap();
            match expected {
                "ok" => assert_eq!(result.unwrap(), b"test"),
                "limit" => assert_eq!(result.unwrap_err().to_string(), "limit"),
                _ => assert_ne!(result.unwrap_err().to_string(), "limit"),
            }
        }
    }
}
