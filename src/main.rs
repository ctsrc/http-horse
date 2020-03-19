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

use std::net::SocketAddr;

use clap::load_yaml;
use clap::crate_version;
use clap::App;
use hyper::http;
use hyper::header;
use hyper::header::HeaderValue;
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};

static NOT_FOUND_BODY_TEXT: &[u8] = b"HTTP 404. File not found.";
static METHOD_NOT_ALLOWED_BODY_TEXT: &[u8] = b"HTTP 405. Method not allowed.";

static INTERNAL_INDEX_PAGE: &[u8] = include_bytes!("../ui-src/html/index.htm");
static INTERNAL_STYLESHEET: &[u8] = include_bytes!("../ui-src/style/main.css");
static INTERNAL_JAVASCRIPT: &[u8] = include_bytes!("../ui-src/js/main.js");

// XXX: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#Directives
static CACHE_CONTROL_VALUE_NO_STORE: &str = "no-store";

#[tokio::main]
async fn main () -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
  let yaml = load_yaml!("cli.yaml");
  let args = App::from_yaml(yaml).version(crate_version!()).get_matches();
  let listen_ip = args.value_of("listen_ip").unwrap().parse()?;
  let port_status = args.value_of("port_status").unwrap().parse()?;
  let port_project = args.value_of("port_project").unwrap().parse()?;

  /*
   * XXX: Listen on same IP address for both status and project servers
   *      because clients speaking to the project server also need to
   *      speak with the status server.
   */
  let addr_status = SocketAddr::new(listen_ip, port_status);
  let addr_project = SocketAddr::new(listen_ip, port_project);

  // Serving of hot-reload-server status pages, showing status and history.
  let handle_status = tokio::spawn(async move
  {
    let srv_status = Server::bind(&addr_status)
      .tcp_nodelay(true)
      // XXX: ^ https://github.com/hyperium/hyper/issues/1997
      //        https://en.wikipedia.org/wiki/Nagle%27s_algorithm
      //        https://www.extrahop.com/company/blog/2016/tcp-nodelay-nagle-quickack-best-practices/
      .serve(make_service_fn(|_| {
        async { Ok::<_, hyper::Error>(service_fn(request_handler_status)) }
      }));

    println!("Access your project through the hot-reload-server status user interface.");
    println!("hot-reload-server status user interface is accessible at http://{}", addr_status);

    srv_status.await
  });

  // Serving of files for the project that the user is working on.
  let handle_project = tokio::spawn(async move
  {
    let srv_project = Server::bind(&addr_project)
      .tcp_nodelay(true)
      .serve(make_service_fn(|_| {
        async { Ok::<_, hyper::Error>(service_fn(request_handler_project)) }
      }));

    println!("Project server started.");

    srv_project.await
  });

  handle_status.await??;
  handle_project.await??;

  Ok(())
}

async fn request_handler_status (req: Request<Body>) -> hyper::http::Result<Response<Body>>
{
  let (method, uri_path) = (req.method(), req.uri().path());

  println!("request_handler_status got request");
  println!("  Method:   {}", method);
  println!("  URI path: {}", uri_path);

  let response_builder = Response::builder()
    .header(header::CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE));

  match (method, uri_path)
  {
    (&Method::GET, "/")               => response_builder.body(INTERNAL_INDEX_PAGE.into()),
    (&Method::GET, "/style/main.css") => response_builder.body(INTERNAL_STYLESHEET.into()),
    (&Method::GET, "/js/main.js")     => response_builder.body(INTERNAL_JAVASCRIPT.into()),
    (&Method::GET, _)                 => not_found(response_builder),
    _                                 => method_not_allowed(response_builder),
  }
}

async fn request_handler_project (req: Request<Body>) -> hyper::http::Result<Response<Body>>
{
  let (method, uri_path) = (req.method(), req.uri().path());

  println!("request_handler_project got request");
  println!("  Method:   {}", method);
  println!("  URI path: {}", uri_path);

  let response_builder = Response::builder()
    .header(header::CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE));

  match (method, uri_path)
  {
    (&Method::GET, _) => not_found(response_builder),
    _                 => method_not_allowed(response_builder),
  }
}

fn method_not_allowed (response_builder: http::response::Builder) -> hyper::http::Result<Response<Body>>
{
  response_builder
    .status(StatusCode::METHOD_NOT_ALLOWED)
    .header(header::ALLOW, HeaderValue::from_static("GET"))
    .body(METHOD_NOT_ALLOWED_BODY_TEXT.into())
}

fn not_found (response_builder: http::response::Builder) -> hyper::http::Result<Response<Body>>
{
  response_builder
    .status(StatusCode::NOT_FOUND)
    .body(NOT_FOUND_BODY_TEXT.into())
}
