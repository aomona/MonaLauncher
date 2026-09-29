use reqwest::{blocking::Response, Url};
use std::io::{self, Read};

/// Preserve the distribution allowlist at every hop; Enderpin handles HTTPS,
/// DNS pinning, public-address checks, connection reuse, timeouts and redirects.
pub(super) fn distribution_response(url: &Url, allowed: fn(&Url) -> bool) -> io::Result<Response> {
    enderpin::registry::response_with_policy(url.as_str(), allowed).map_err(io::Error::other)
}

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
    fn enderpin_transport_keeps_caller_allowlists_and_rejects_private_addresses() {
        let url = Url::parse("https://not-resolved.invalid/file").unwrap();
        let denied = distribution_response(&url, |_| false).unwrap_err();
        assert!(denied.to_string().contains("caller policy"));
        // Even an overly broad caller policy cannot opt out of Enderpin's network boundary.
        for raw in ["http://127.0.0.1/file", "https://127.0.0.1/file"] {
            assert!(distribution_response(&Url::parse(raw).unwrap(), |_| true).is_err());
        }
    }

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
