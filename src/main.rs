use anyhow::{anyhow, Context};
use askama::Template;
use async_stream::stream;
use bytes::Bytes;
use clap::{crate_version, Parser, ValueEnum};
use futures_util::TryStreamExt;
use http_body_util::{combinators::BoxBody, BodyExt, Either, Full, StreamBody};
use hyper::{
    body::{Frame, Incoming},
    header,
    header::HeaderValue,
    http::{response::Builder as ResponseBuilder, Result as HttpResult},
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use std::{
    io::ErrorKind,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    pin::pin,
    sync::OnceLock,
    time::Duration,
};
use thiserror::Error;
use tokio::{fs::File, net::TcpListener};
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, warn};

#[derive(Template)]
#[template(path = "status-webui/index.htm")]
struct StatusWebUiIndex<'a> {
    project_dir: &'a str,
    color_scheme: ColorScheme,
}

static INTERNAL_INDEX_PAGE: OnceLock<Vec<u8>> = OnceLock::new();

static NOT_FOUND_BODY_TEXT: &[u8] = b"HTTP 404. File not found.";
static METHOD_NOT_ALLOWED_BODY_TEXT: &[u8] = b"HTTP 405. Method not allowed.";
static INTERNAL_SERVER_ERROR_BODY_TEXT: &[u8] = b"HTTP 500. Internal server error.";

static INTERNAL_STYLESHEET: &[u8] = include_bytes!("../webui-src/style/main.css");
static INTERNAL_JAVASCRIPT: &[u8] = include_bytes!("../webui-src/js/main.js");

// XXX: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#Directives
static CACHE_CONTROL_VALUE_NO_STORE: &str = "no-store";

// MIME type for Server-Sent Events
// XXX: https://html.spec.whatwg.org/multipage/server-sent-events.html#server-sent-events
static TEXT_EVENT_STREAM: &str = "text/event-stream";

static IMAGE_X_ICON: &str = "image/x-icon";
static TEXT_CSS: &str = "text/css";
static TEXT_HTML: &str = "text/html";
static TEXT_JAVASCRIPT: &str = "text/javascript";
static TEXT_PLAIN: &str = "text/plain";

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    /*
     * Flags
     */
    /// Open the project and status pages in a web browser.
    #[arg(short = 'o', long)]
    open: bool,
    /*
     * Options
     */
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
    /// Color theme to use for status web-ui
    #[arg(value_enum, short = 'c', long, default_value_t = ColorScheme::GraphiteAndCopper)]
    color_scheme: ColorScheme,
    /*
     * Positional arguments
     */
    /// Project directory
    #[arg(default_value = ".")]
    dir: String,
}

/// Color theme to use for status web-ui
#[derive(ValueEnum, Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ColorScheme {
    /// Midnight Purple (Dark Mode)
    MidnightPurple,
    /// Slate Green (Dark Mode)
    SlateGreen,
    /// Abyss Blue (Dark Mode)
    AbyssBlue,
    /// Graphite & Copper (Dark Mode)
    GraphiteAndCopper,
    /// Crimson & Charcoal (Dark Mode)
    CrimsonAndCharcoal,
}

static PROJECT_DIR: OnceLock<PathBuf> = OnceLock::new();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    info!("Starting http-horse v{}", crate_version!());

    let args = Cli::parse();

    let project_dir = PathBuf::from(args.dir);
    let project_dir = project_dir
        .canonicalize()
        .inspect_err(
            |e| error!(err = ?e, ?project_dir, "Fatal: Failed to canonicalize project dir path."),
        )
        .with_context(|| format!("Failed to canonicalize project dir path: {project_dir:?}"))?;

    if !project_dir.is_dir() {
        error!(
            ?project_dir,
            "Fatal: File is not a directory: Project dir path."
        );
        return Err(anyhow!(
            "File is not a directory: Project dir path: {project_dir:?}"
        ));
    }

    PROJECT_DIR
        .set(project_dir.clone())
        .inspect_err(|e| error!(existing_value = ?e, "Fatal: OnceLock has existing value."))
        .map_err(|_| anyhow!("Failed to set value of OnceLock."))?;

    // FsEvent takes strings as arguments. We always want to use the canonical path,
    // and because of that we have to convert back to String from PathBuf.
    let pdir = project_dir
        .into_os_string()
        .into_string()
        .inspect_err(|e| error!(os_string = ?e, "Fatal: Failed to convert PathBuf to String."))
        .map_err(|_| anyhow!("Failed to convert PathBuf to String."))?;

    let internal_index_page = StatusWebUiIndex {
        project_dir: &pdir,
        color_scheme: args.color_scheme,
    };
    let internal_index_page_rendered = internal_index_page.render()?.as_bytes().to_vec();
    INTERNAL_INDEX_PAGE
        .set(internal_index_page_rendered)
        .inspect_err(|e| error!(existing_value = ?e, "Fatal: OnceLock has existing value."))
        .map_err(|_| anyhow!("Failed to set value of OnceLock."))?;

    let status_addr = SocketAddr::new(args.status_listen_addr, args.status_listen_port);
    let status_tcp = TcpListener::bind(status_addr)
        .await
        .inspect_err(|e| {
            error!(
                err = ?e,
                ?status_addr,
                "Fatal: Failed to bind TCP listener for status server."
            )
        })
        .with_context(|| "Failed to bind TCP listener for status server.")?;
    let status_addr = status_tcp
        .local_addr()
        .inspect_err(|e| {
            error!(
                err = ?e,
                ?status_addr,
                ?status_tcp,
                "Fatal: Failed to get local address that status server is bound to."
            )
        })
        .with_context(|| "Failed to get local address that status server is bound to.")?;
    let status_url_s = format!("http://{status_addr}");
    let status_url = &status_url_s;
    info!(status_url, "Status pages will be served on <{status_url}>.");

    let project_addr = SocketAddr::new(args.project_listen_addr, args.project_listen_port);
    let project_tcp = TcpListener::bind(project_addr)
        .await
        .inspect_err(|e| {
            error!(
                err = ?e,
                ?project_addr,
                "Fatal: Failed to bind TCP listener for project server."
            )
        })
        .with_context(|| "Failed to bind TCP listener for project server.")?;
    let project_addr = project_tcp
        .local_addr()
        .inspect_err(|e| {
            error!(
                err = ?e,
                ?project_addr,
                ?project_tcp,
                "Fatal: Failed to get local address that project server is bound to."
            )
        })
        .with_context(|| "Failed to get local address that project server is bound to.")?;
    let project_url_s = format!("http://{project_addr}");
    let project_url = &project_url_s;
    info!(
        project_url,
        "Project pages will be served on <{project_url}>."
    );

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
     *      So what we are going to do is, anytime a file or directory is moved into, within, or out
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

    let (project_out_fs_event_tx, project_out_fs_event_rx) = std::sync::mpsc::channel();

    let project_out_fs_event_observer_handle = std::thread::spawn(move || {
        let project_out_fs_observer = fsevent::FsEvent::new(vec![pdir]);
        project_out_fs_observer.observe(project_out_fs_event_tx);
    });

    let project_out_fs_event_transformer_handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(15));
        // TODO: Create initial temp file in project dir
        // TODO: Start a timer so we can check how long has passed since we created initial temp file.
        // TODO: Initial scan of project dir
        'skip_up_to_temp_file: loop {
            match project_out_fs_event_rx.recv() {
                Ok(fs_ev) => {
                    debug!(?fs_ev, "fs event");
                    if false
                    // TODO: If this event corresponds to the creation of the initial temp file
                    {
                        break 'skip_up_to_temp_file;
                    } else {
                        // TODO: Check how much time has passed since initial temp file was created
                        // TODO: If more than 30 seconds has passed, create a new temp file
                        //       and rescan project dir. Skip all events up to new temp file.
                    }
                }
                Err(e) => error!(err = ?e, "fs event recv error!"),
            };
        }
        loop {
            match project_out_fs_event_rx.recv() {
                Ok(fs_ev) => {
                    if false
                    // TODO: If event type is move
                    {
                        // TODO: Create temp file in project dir
                        // TODO: Start a timer so we can check how long has passed since we created temp file.
                        // TODO: Rescan of project dir
                        'skip_up_to_temp_file: loop {
                            match project_out_fs_event_rx.recv() {
                                Ok(fs_ev) => {
                                    debug!(?fs_ev, "fs event");
                                    if false
                                    // TODO: If this event corresponds to the creation of the temp file
                                    {
                                        break 'skip_up_to_temp_file;
                                    } else {
                                        // TODO: Check how much time has passed since temp file was created
                                        // TODO: If more than n seconds has passed, create a new temp file
                                        //       and rescan project dir. Skip all events up to new temp file.
                                        //       n is exponentially increasing for each time this happens,
                                        //       up to an upper limit of 10 minutes.
                                    }
                                }
                                Err(e) => error!(err = ?e, "fs event recv error!"),
                            };
                        }
                    } else {
                        info!(?fs_ev, "fs event")
                    }
                }
                Err(e) => error!(err = ?e, "fs event recv error!"),
            };
        }
    });

    let server = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
    let graceful = hyper_util::server::graceful::GracefulShutdown::new();
    let mut ctrl_c = pin!(tokio::signal::ctrl_c());

    info!("Starting status and project servers.");
    // Skip printing hints if we are going to attempt to open the web browser for the user.
    if !args.open {
        info!("Access your project through the http-horse status user interface.");
        info!(
            status_url,
            "http-horse status user interface is accessible at <{status_url}>."
        );
    }

    // Attempt to open web browser for the user if they supplied the flag for doing so.
    // If we fail to open any of the URLs, print corresponding error and instruct the user
    // to manually open each of the URLs that we failed to open for them.
    // These errors are considered non-fatal, and program execution continues.
    if args.open {
        info!("Attempting to open http-horse status page in web browser.");
        if let Err(e) = opener::open(status_url) {
            error!(?e, "Failed to open http-horse status page in web browser.");
            info!(status_url, "To view the http-horse status user interface, please open the following URL manually in a web browser: <{status_url}>.");
        }
        info!("Attempting to open served project in web browser.");
        if let Err(e) = opener::open(project_url) {
            error!(?e, "Failed to open served project in web browser.");
            info!(project_url, "To view your served project, please open the following URL manually in a web browser: <{project_url}>.");
        }
    }

    // XXX: https://github.com/hyperium/hyper-util/blob/df55abac42d0cc1e1577f771d8a1fc91f4bcd0dd/examples/server_graceful.rs
    loop {
        tokio::select! {
            /*
             * TODO: Enable TCP_NODELAY for accepted connections.
             *
             * XXX: For details about TCP_NODELAY, see
             *      https://github.com/hyperium/hyper/issues/1997
             *      https://en.wikipedia.org/wiki/Nagle%27s_algorithm
             *      https://www.extrahop.com/company/blog/2016/tcp-nodelay-nagle-quickack-best-practices/
             */

            /*
             * Serving of files for the project that the user is working on.
             */
            project_conn = project_tcp.accept() => {
                let (stream, peer_addr) = match project_conn {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!(err = ?e, "Accept error");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                debug!(?peer_addr, "Incoming connection accepted");
                let stream = TokioIo::new(Box::pin(stream));
                let conn = server.serve_connection_with_upgrades(stream, service_fn(request_handler_project));
                let conn = graceful.watch(conn.into_owned());
                tokio::spawn(async move {
                    if let Err(e) = conn.await {
                        // We log this error at debug level because it is usually not interesting.
                        // Known, uninteresting things (from error level logs perspective)
                        // that trigger this error:
                        // - In the case where user closes browser tab, we get a connection error
                        //   if a message was still in progress of being sent.
                        // - If the user agent is sends just `GET /` without specifying HTTP version,
                        //   as they used to do for what we now sometimes refer to as HTTP/0.9,
                        //   we get an "invalid URI" error.
                        //   Conversely:
                        //   * A client that sends `GET / HTTP/1.1` gets a HTTP/1.1 response
                        //     as one would expect.
                        //   * A client that sends `GET / HTTP/1.0` gets a HTTP/1.0 response
                        //     as one might expect.
                        // TODO: Any cases that would warrant logging this at level error?
                        debug!(err = e, "Connection error");
                    }
                    debug!(?peer_addr, "Connection dropped");
                });
            },

            /*
             * Serving of status pages, showing status and history.
             */
            status_conn = status_tcp.accept() => {
                let (stream, peer_addr) = match status_conn {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!(err = ?e, "Accept error");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                debug!(?peer_addr, "Incoming connection accepted");
                let stream = TokioIo::new(Box::pin(stream));
                let conn = server.serve_connection_with_upgrades(stream, service_fn(request_handler_status));
                let conn = graceful.watch(conn.into_owned());
                tokio::spawn(async move {
                    if let Err(e) = conn.await {
                        // We log this error at debug level because it is usually not interesting.
                        // Known, uninteresting things (from error level logs perspective)
                        // that trigger this error:
                        // - In the case where user closes browser tab, we get a connection error
                        //   if a message was still in progress of being sent.
                        // - If the user agent is sends just `GET /` without specifying HTTP version,
                        //   as they used to do for what we now sometimes refer to as HTTP/0.9,
                        //   we get an "invalid URI" error.
                        //   Conversely:
                        //   * A client that sends `GET / HTTP/1.1` gets a HTTP/1.1 response
                        //     as one would expect.
                        //   * A client that sends `GET / HTTP/1.0` gets a HTTP/1.0 response
                        //     as one might expect.
                        // TODO: Any cases that would warrant logging this at level error?
                        debug!(err = e, "Connection error");
                    }
                    debug!(?peer_addr, "Connection dropped");
                });
            },

            _ = ctrl_c.as_mut() => {
                drop(project_tcp);
                drop(status_tcp);
                info!("Ctrl-C received, starting shutdown");
                break;
            }
        }
    }

    info!("Shutting down FS event observer thread for project out dir.");
    drop(project_out_fs_event_observer_handle);

    info!("Shutting down FS event transformer thread for project out dir.");
    drop(project_out_fs_event_transformer_handle);

    Ok(())
}

#[derive(Error, Debug)]
#[error("FS Event Observer has disconnected")]
pub struct FSEventObserverDisconnectedError;

fn event_stream() -> BoxBody<Bytes, FSEventObserverDisconnectedError> {
    // TODO: Connect the thing
    let stream = stream! {
        let mut i = 0;
        loop {
            // Sleep 250ms between each iteration so we don't overwhelm the web page with events.
            tokio::time::sleep(Duration::from_millis(250)).await;
            yield Ok(Bytes::from(format!("data: {{\"elem\": {i}}}\n\n")));
            i += 1;
        }
    };
    let stream_body = StreamBody::new(stream.map_ok(Frame::data));
    BodyExt::boxed(stream_body)
}

async fn request_handler_status(
    req: Request<Incoming>,
) -> HttpResult<Response<Either<Full<Bytes>, BoxBody<Bytes, FSEventObserverDisconnectedError>>>> {
    let (method, uri_path) = (req.method(), req.uri().path());
    let uri_path_trimmed = uri_path.trim_start_matches('/');
    debug!(
        ?method,
        uri_path, uri_path_trimmed, "Status server is handling a request"
    );
    // XXX: The path join operation completely replaces the path we are joining onto
    //      if the component we are joining has a leading slash. Likewise, pushing onto
    //      a path buf behaves in a similar fashion in terms of leading slashes.
    //      It is therefore essential that we only use the path that has leading slashes stripped.
    let uri_path = uri_path_trimmed;

    let response_builder = Response::builder().header(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE),
    );

    match (method, uri_path) {
        (&Method::GET, "") => match INTERNAL_INDEX_PAGE.get() {
            None => {
                error!("Failed to get rendered index page for status web-ui!");
                let (status, content_type, body) = server_error();
                response_builder
                    .header(header::CONTENT_TYPE, content_type)
                    .status(status)
                    .body(Either::Left(body))
            }
            Some(internal_index_page) => response_builder
                .header(header::CONTENT_TYPE, HeaderValue::from_static(TEXT_HTML))
                .body(Either::Left(internal_index_page.as_slice().into())),
        },
        (&Method::GET, "favicon.ico") => response_builder
            .header(header::CONTENT_TYPE, HeaderValue::from_static(IMAGE_X_ICON))
            .status(StatusCode::NO_CONTENT)
            .body(Either::Left("".into())),
        (&Method::GET, "style/main.css") => response_builder
            .header(header::CONTENT_TYPE, HeaderValue::from_static(TEXT_CSS))
            .body(Either::Left(INTERNAL_STYLESHEET.into())),
        (&Method::GET, "js/main.js") => response_builder
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_static(TEXT_JAVASCRIPT),
            )
            .body(Either::Left(INTERNAL_JAVASCRIPT.into())),
        (&Method::GET, "event-stream/") => response_builder
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_static(TEXT_EVENT_STREAM),
            )
            .body(Either::Right(event_stream())),
        (&Method::GET, _) => {
            warn!(
                uri_path,
                "Status server got request with unexpected uri path. Returning 404."
            );
            let (status, content_type, body) = not_found();
            response_builder
                .header(header::CONTENT_TYPE, content_type)
                .status(status)
                .body(Either::Left(body))
        }
        _ => {
            warn!(
                uri_path,
                "Status server got request with unexpected request method. Returning 405."
            );
            let (status, content_type, body) = method_not_allowed();
            response_builder
                .header(header::CONTENT_TYPE, content_type)
                .status(status)
                .body(Either::Left(body))
        }
    }
}

async fn request_handler_project(
    req: Request<Incoming>,
) -> HttpResult<Response<Either<Full<Bytes>, BoxBody<Bytes, std::io::Error>>>> {
    let (method, uri_path) = (req.method(), req.uri().path());
    let uri_path_trimmed = uri_path.trim_start_matches('/');
    debug!(
        ?method,
        uri_path, uri_path_trimmed, "Project server is handling a request"
    );
    // XXX: The path join operation completely replaces the path we are joining onto
    //      if the component we are joining has a leading slash. Likewise, pushing onto
    //      a path buf behaves in a similar fashion in terms of leading slashes.
    //      It is therefore essential that we only use the path that has leading slashes stripped.
    let uri_path = uri_path_trimmed;

    let response_builder = Response::builder().header(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE),
    );

    let Some(project_dir) = PROJECT_DIR.get() else {
        let (status, content_type, body) = server_error();
        let resp = response_builder
            .header(header::CONTENT_TYPE, content_type)
            .status(status)
            .body(Either::Left(body));
        return resp;
    };

    match (method, uri_path) {
        (&Method::GET, _) => {
            if uri_path.is_empty() {
                handle_dir_request(project_dir, response_builder).await
            } else {
                let uri_path = uri_path.trim_start_matches('/');
                let req_path = Path::join(project_dir.as_ref(), uri_path);
                debug!(
                    ?project_dir,
                    uri_path,
                    ?req_path,
                    "Constructed req_path from project_dir and uri_path."
                );
                // Early check to ensure that the constructed req path still begins with project dir path.
                if !(req_path.starts_with(project_dir)) {
                    error!(
                        ?req_path,
                        ?project_dir,
                        "Constructed req_path does not begin with project_dir path."
                    );
                }

                let Ok(req_path) = req_path.canonicalize().inspect_err(|e| match e.kind() {
                    ErrorKind::NotFound => {
                        // Note: We explicitly log that we did not find file, because we actually went looking for it.
                        warn!(err = ?e, uri_path, ?req_path, "File not found on file system.");
                    }
                    _ => {
                        error!(err = ?e, uri_path, ?req_path, "Unexpected I/O error");
                    }
                }) else {
                    // Any error resulting from the above attempt at finding canonical path
                    // of the file is returned as a 404 Not Found error to the user agent.
                    warn!(
                        uri_path,
                        ?req_path,
                        "Returning 404 Not Found to client due to request path error."
                    );
                    let (status, content_type, body) = not_found();
                    return response_builder
                        .header(header::CONTENT_TYPE, content_type)
                        .status(status)
                        .body(Either::Left(body));
                };

                // We disallow traversing up above the project dir.
                //
                // Sidenote: Well-behaved user-agents like Firefox or curl
                // will default to resolving paths locally so that they don't
                // attempt to go further up than "/" in the url path before they
                // send the request. But anyone can manually send a http request
                // that attempts to traverse outside the project root dir.
                //
                // For example, using telnet
                //
                // ```zsh
                // telnet example.com 80
                // ```
                //
                // They can send a request like say:
                //
                // ```http
                // GET /../../../ HTTP/1.1
                // Host: example.com
                //
                // ```
                if !req_path.starts_with(project_dir) {
                    warn!(
                        uri_path,
                        ?req_path,
                        "Client attempted to traverse outside of project directory. Returning 404."
                    );
                    let (status, content_type, body) = not_found();
                    return response_builder
                        .header(header::CONTENT_TYPE, content_type)
                        .status(status)
                        .body(Either::Left(body));
                }
                let req_path_checked = req_path;

                if req_path_checked.is_dir() {
                    handle_dir_request(req_path_checked, response_builder).await
                } else {
                    // TODO: Look for the file
                    let (status, content_type, body) = not_found();
                    response_builder
                        .header(header::CONTENT_TYPE, content_type)
                        .status(status)
                        .body(Either::Left(body))
                }
            }
        }
        _ => {
            warn!(
                uri_path,
                "Project server got request with unexpected request method. Returning 405."
            );
            let (status, content_type, body) = method_not_allowed();
            response_builder
                .header(header::CONTENT_TYPE, content_type)
                .status(status)
                .body(Either::Left(body))
        }
    }
}

/// Handle a dir request.
///
/// Security note: It is the responsibility of the *caller* to ensure
/// that the requested directory is not outside the intended path.
/// (I.e. caller has to be careful about requests like `GET /foo/../../../bar/`, etc.)
async fn handle_dir_request<P: AsRef<Path>>(
    req_path_checked: P,
    response_builder: ResponseBuilder,
) -> HttpResult<Response<Either<Full<Bytes>, BoxBody<Bytes, std::io::Error>>>> {
    // 1. Try file "index.htm"
    if let Ok(file) = File::open(req_path_checked.as_ref().join("index.htm")).await {
        // Based on <https://github.com/hyperium/hyper/blob/4c84e8c1c26a1464221de96b9f39816ce7251a5f/examples/send_file.rs#L81C1-L82C42>
        let reader_stream = ReaderStream::new(file);
        let stream_body = StreamBody::new(reader_stream.map_ok(Frame::data));
        let boxed_body = BodyExt::boxed(stream_body);
        return response_builder.body(Either::Right(boxed_body));
    }
    // 2. Try file "index.html"
    if let Ok(file) = File::open(req_path_checked.as_ref().join("index.html")).await {
        // Based on <https://github.com/hyperium/hyper/blob/4c84e8c1c26a1464221de96b9f39816ce7251a5f/examples/send_file.rs#L81C1-L82C42>
        let reader_stream = ReaderStream::new(file);
        let stream_body = StreamBody::new(reader_stream.map_ok(Frame::data));
        let boxed_body = BodyExt::boxed(stream_body);
        return response_builder.body(Either::Right(boxed_body));
    }
    // 3. Return a directory listing. (Note: This one needs to update itself as well.)
    // TODO: dir listing
    let (status, content_type, body) = not_found();
    response_builder
        .header(header::CONTENT_TYPE, content_type)
        .status(status)
        .body(Either::Left(body))
}

fn server_error() -> (StatusCode, HeaderValue, Full<Bytes>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        HeaderValue::from_static(TEXT_PLAIN),
        INTERNAL_SERVER_ERROR_BODY_TEXT.into(),
    )
}

fn method_not_allowed() -> (StatusCode, HeaderValue, Full<Bytes>) {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        HeaderValue::from_static(TEXT_PLAIN),
        METHOD_NOT_ALLOWED_BODY_TEXT.into(),
    )
}

fn not_found() -> (StatusCode, HeaderValue, Full<Bytes>) {
    (
        StatusCode::NOT_FOUND,
        HeaderValue::from_static(TEXT_PLAIN),
        NOT_FOUND_BODY_TEXT.into(),
    )
}
