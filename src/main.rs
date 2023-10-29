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

use clap::Parser;
use fsevent;
use futures_util::future::join;
use hyper::header::HeaderValue;
use hyper::http;
use hyper::service::{make_service_fn, service_fn};
use hyper::{header, Server};
use hyper::{Body, Method, Request, Response, StatusCode};
use std::fs::metadata;
use std::net::{IpAddr, SocketAddr};

static NOT_FOUND_BODY_TEXT: &[u8] = b"HTTP 404. File not found.";
static METHOD_NOT_ALLOWED_BODY_TEXT: &[u8] = b"HTTP 405. Method not allowed.";

static INTERNAL_INDEX_PAGE: &[u8] = include_bytes!("../webui-src/html/index.htm");
static INTERNAL_STYLESHEET: &[u8] = include_bytes!("../webui-src/style/main.css");
static INTERNAL_JAVASCRIPT: &[u8] = include_bytes!("../webui-src/js/main.js");

// XXX: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#Directives
static CACHE_CONTROL_VALUE_NO_STORE: &str = "no-store";

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    /// Project directory
    dir: String,
    /// Address to serve project on
    #[arg(short = 'l', long, default_value = "::1")]
    project_listen_addr: IpAddr,
    /// Port to serve project on
    #[arg(short = 'p', long, default_value_t = 0)]
    project_listen_port: u16,
    /// Address to serve status on
    #[arg(short = 's', long, default_value = "::1")]
    status_listen_addr: IpAddr,
    /// Port to serve status on
    #[arg(short = 'q', long, default_value_t = 0)]
    status_listen_port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Cli::parse();

    let project_dir = args.dir;
    let project_dir_md = metadata(project_dir.clone())?;
    if !project_dir_md.is_dir() {
        return Err(format!("File is not a directory: {project_dir}").into());
    }

    let status_addr = SocketAddr::new(args.project_listen_addr, args.project_listen_port);
    let project_addr = SocketAddr::new(args.status_listen_addr, args.status_listen_port);

    /*
     * Try binding to the IP and port pairs for each of status and project servers.
     * Additionally, enable TCP_NODELAY for accepted connections.
     *
     * XXX: For details about TCP_NODELAY, see
     *      https://github.com/hyperium/hyper/issues/1997
     *      https://en.wikipedia.org/wiki/Nagle%27s_algorithm
     *      https://www.extrahop.com/company/blog/2016/tcp-nodelay-nagle-quickack-best-practices/
     */
    println!("Attempting to bind status server to {}", status_addr);
    let status_server_builder = Server::try_bind(&status_addr)?.tcp_nodelay(true);
    println!("Attempting to bind project server to {}", project_addr);
    let project_server_builder = Server::try_bind(&project_addr)?.tcp_nodelay(true);

    /*
     * We monitor FS events in the project dir using the
     * Apple File System Events API via the fsevent crate.
     *
     * XXX: Hardlink creation does not result in any corresponding event.
     *      Issue for this filed at https://github.com/octplane/fsevent-rust/issues/27
     *
     * XXX: When files are moved, two events are generated. One for the source file path,
     *      and one for the target file path. Because we are choosing to subscribe to events
     *      for the project directory only, we don't see "the other half" of a pair of events
     *      where a file is moved into or out of the project directory. Now, we could of course
     *      monitor the whole file system, and do our best to correlate all moves that affect us.
     *      But really, that's a lot of work for little actual benefit.
     *
     *      So what we are gonna do is, anytime a file or directory is moved into, within, or out
     *      of the project directory, we create a temporary file, recursively rescan the project
     *      directory and "fast-forward" to the point in the stream where we see the creation of
     *      our temporary file. We do that same temporary file thing for the initial scan as well.
     *
     *      And if you think it's weird to do it that way, keep in mind that:
     *
     *        1. The information provided by the FSE API is only advisory anyway, and
     *
     *        2. Our use-case revolves mainly around files being written to most of the
     *           time, and sometimes being created or deleted, and normally not being moved.
     *           So, whereas in contexts where there is a lot of moving going on it might
     *           not make so much sense to do it like this, it does in our case and also
     *           helps keep the picture that we have of our project dir over time robust
     *           (i.e. correctly corresponding to actual reality).
     *
     *      So all in all this is actually a good solution we have here, I think.
     */

    let (fs_event_tx, fs_event_rx) = std::sync::mpsc::channel();

    let fs_event_observer_task = tokio::task::spawn_blocking(move || {
        let fs_observer = fsevent::FsEvent::new(vec![project_dir.clone()]);
        fs_observer.observe(fs_event_tx);
    });

    let fs_event_transformer_task = tokio::task::spawn_blocking(move || {
        // TODO: Sleep for like 15ms or something
        // TODO: Create temp file
        // TODO: Scan project-dir
        loop {
            match fs_event_rx.recv() {
                Ok(fs_ev) => {
                    if false
                    // TODO: If temp file is Some
                    {
                        if false
                        // TODO: If fs_ev path == temp file path
                        {
                            // TODO: Delete temp file and set variable to None
                        } else {
                            println!("(fast-forwarding) skipping event: {:?}", fs_ev);
                        }
                    } else {
                        if false
                        // TODO: If event type is move
                        {
                            // TODO: Create temp file
                            // TODO: Rescan project-dir
                        } else {
                            println!("fs event: {:?}", fs_ev)
                        }
                    }
                }
                Err(_e) => println!("fs event recv error!"),
            };
        }
    });

    /*
     * Serving of hot-reload-server status pages, showing status and history.
     */
    let status_server = status_server_builder.serve(make_service_fn(|_| async {
        Ok::<_, hyper::Error>(service_fn(request_handler_status))
    }));

    /*
     * Serving of files for the project that the user is working on.
     */
    let project_server = project_server_builder.serve(make_service_fn(|_| async {
        Ok::<_, hyper::Error>(service_fn(request_handler_project))
    }));

    println!("Starting status and project servers.");
    println!("Access your project through the hot-reload-server status user interface.");
    println!(
        "hot-reload-server status user interface is accessible at http://{}",
        status_addr
    );

    let fs_event_tasks = join(fs_event_observer_task, fs_event_transformer_task);
    let servers = join(status_server, project_server);
    let ret = join(fs_event_tasks, servers).await;

    (ret.0).0?;
    (ret.0).1?;
    (ret.1).0?;
    (ret.1).1?;

    Ok(())
}

fn connect_event_stream(
    response_builder: http::response::Builder,
) -> hyper::http::Result<Response<Body>> {
    let (_sender, body) = hyper::body::Body::channel();

    // TODO: Connect the thing

    response_builder.body(body)
}

async fn request_handler_status(req: Request<Body>) -> hyper::http::Result<Response<Body>> {
    let (method, uri_path) = (req.method(), req.uri().path());

    println!("request_handler_status got request");
    println!("  Method:   {}", method);
    println!("  URI path: {}", uri_path);

    let response_builder = Response::builder().header(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE),
    );

    match (method, uri_path) {
        (&Method::GET, "/") => response_builder.body(INTERNAL_INDEX_PAGE.into()),
        (&Method::GET, "/style/main.css") => response_builder.body(INTERNAL_STYLESHEET.into()),
        (&Method::GET, "/js/main.js") => response_builder.body(INTERNAL_JAVASCRIPT.into()),
        (&Method::GET, "/event-stream/") => connect_event_stream(response_builder),
        (&Method::GET, _) => not_found(response_builder),
        _ => method_not_allowed(response_builder),
    }
}

async fn request_handler_project(req: Request<Body>) -> hyper::http::Result<Response<Body>> {
    let (method, uri_path) = (req.method(), req.uri().path());

    println!("request_handler_project got request");
    println!("  Method:   {}", method);
    println!("  URI path: {}", uri_path);

    let response_builder = Response::builder().header(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE),
    );

    match (method, uri_path) {
        (&Method::GET, _) => not_found(response_builder),
        _ => method_not_allowed(response_builder),
    }
}

fn method_not_allowed(
    response_builder: http::response::Builder,
) -> hyper::http::Result<Response<Body>> {
    response_builder
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .header(header::ALLOW, HeaderValue::from_static("GET"))
        .body(METHOD_NOT_ALLOWED_BODY_TEXT.into())
}

fn not_found(response_builder: http::response::Builder) -> hyper::http::Result<Response<Body>> {
    response_builder
        .status(StatusCode::NOT_FOUND)
        .body(NOT_FOUND_BODY_TEXT.into())
}
