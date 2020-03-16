/*
 * Copyright (c) 2020 Erik Nordstrøm <erik@nordstroem.no>
 *
 * Permission to use, copy, modify, and/or distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 * ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 * ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 * OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */

use hyper::header;
use hyper::header::HeaderValue;
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use std::net::SocketAddr;

static NOT_FOUND_BODY_TEXT: &[u8] = b"HTTP 404. File not found.";
static METHOD_NOT_ALLOWED_BODY_TEXT: &[u8] = b"HTTP 405. Method not allowed.";

// XXX: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#Directives
static CACHE_CONTROL_VALUE_NO_STORE: &str = "no-store";

#[tokio::main]
async fn main () -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{

  let handle_internal = tokio::spawn(async {
    // address on which we serve hot-reload-server internal pages, showing status and history
    let addr_internal: SocketAddr = ([127, 0, 0, 1], 8000).into();

    let srv_internal = Server::bind(&addr_internal)
      .tcp_nodelay(true)
      // XXX: ^ https://github.com/hyperium/hyper/issues/1997
      //        https://en.wikipedia.org/wiki/Nagle%27s_algorithm
      //        https://www.extrahop.com/company/blog/2016/tcp-nodelay-nagle-quickack-best-practices/
      .serve(make_service_fn(|_| {
        async { Ok::<_, hyper::Error>(service_fn(request_handler_internal)) }
      }));

    println!("Access your project through the hot-reload-server user interface.");
    println!("hot-reload-server user interface is accessible at http://{}", addr_internal);

    srv_internal.await
  });

  let handle_project = tokio::spawn(async {
    // address on which we serve the files for the project that the user is working on
    let addr_project  = ([127, 0, 0, 1], 8080).into();

    let srv_project = Server::bind(&addr_project)
      .tcp_nodelay(true)
      .serve(make_service_fn(|_| {
        async { Ok::<_, hyper::Error>(service_fn(request_handler_project)) }
      }));

    println!("Project server started.");

    srv_project.await
  });

  let a = handle_internal.await?;
  let b = handle_project.await?;

  Ok(())
}

async fn request_handler_internal (req: Request<Body>) -> hyper::Result<Response<Body>>
{
  let (method, uri_path) = (req.method(), req.uri().path());

  println!("request_handler_internal got request");
  println!("  Method:   {}", method);
  println!("  URI path: {}", uri_path);

  let mut response = match (method, uri_path)
  {
    (&Method::GET, _) => not_found(),
    _                 => method_not_allowed(),
  };

  response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE));

  Ok(response)
}

async fn request_handler_project (req: Request<Body>) -> hyper::Result<Response<Body>>
{
  let (method, uri_path) = (req.method(), req.uri().path());

  println!("request_handler_project got request");
  println!("  Method:   {}", method);
  println!("  URI path: {}", uri_path);

  let mut response = match (method, uri_path)
  {
    (&Method::GET, _) => not_found(),
    _                 => method_not_allowed(),
  };

  response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE));

  Ok(response)
}

fn method_not_allowed () -> Response<Body>
{
  Response::builder()
    .status(StatusCode::METHOD_NOT_ALLOWED)
    .header(header::ALLOW, HeaderValue::from_static("GET"))
    .body(METHOD_NOT_ALLOWED_BODY_TEXT.into())
    .unwrap()
}

fn not_found () -> Response<Body>
{
  Response::builder()
    .status(StatusCode::NOT_FOUND)
    .body(NOT_FOUND_BODY_TEXT.into())
    .unwrap()
}
