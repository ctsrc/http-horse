use clap::crate_version;
use clap::Parser;
use futures_util::future::join;
use hyper::header::HeaderValue;
use hyper::http;
use hyper::service::{make_service_fn, service_fn};
use hyper::{header, Server};
use hyper::{Body, Method, Request, Response, StatusCode};
use std::fs::metadata;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};
use tracing::{debug, error, info};

static NOT_FOUND_BODY_TEXT: &[u8] = b"HTTP 404. File not found.";
static METHOD_NOT_ALLOWED_BODY_TEXT: &[u8] = b"HTTP 405. Method not allowed.";
static INTERNAL_SERVER_ERROR_BODY_TEXT: &[u8] = b"HTTP 500. Internal server error.";

static INTERNAL_INDEX_PAGE: &[u8] = include_bytes!("../webui-src/html/index.htm");
static INTERNAL_STYLESHEET: &[u8] = include_bytes!("../webui-src/style/main.css");
static INTERNAL_JAVASCRIPT: &[u8] = include_bytes!("../webui-src/js/main.js");

// XXX: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#Directives
static CACHE_CONTROL_VALUE_NO_STORE: &str = "no-store";

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    /// Project directory
    #[arg(default_value = ".")]
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

static PROJECT_DIR: OnceLock<String> = OnceLock::new();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    info!("Starting http-horse v{}", crate_version!());

    let args = Cli::parse();

    let project_dir = args.dir.clone();
    let project_dir_md = metadata(project_dir.clone())?;
    if !project_dir_md.is_dir() {
        return Err(format!("File is not a directory: {project_dir}").into());
    }
    PROJECT_DIR.set(project_dir.clone())?;

    let status_addr = SocketAddr::new(args.project_listen_addr, args.project_listen_port);
    let status_tcp = TcpListener::bind(status_addr)?;
    let status_addr = status_tcp.local_addr()?;
    info!("Status pages will be served on http://{status_addr}");

    let project_addr = SocketAddr::new(args.status_listen_addr, args.status_listen_port);
    let project_tcp = TcpListener::bind(project_addr)?;
    let project_addr = project_tcp.local_addr()?;
    info!("Project pages will be served on http://{project_addr}");

    /*
     * Create server builders for status and project.
     * Enable TCP_NODELAY for accepted connections.
     *
     * XXX: For details about TCP_NODELAY, see
     *      https://github.com/hyperium/hyper/issues/1997
     *      https://en.wikipedia.org/wiki/Nagle%27s_algorithm
     *      https://www.extrahop.com/company/blog/2016/tcp-nodelay-nagle-quickack-best-practices/
     */
    let status_server_builder = Server::from_tcp(status_tcp)?.tcp_nodelay(true);
    let project_server_builder = Server::from_tcp(project_tcp)?.tcp_nodelay(true);

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

    let pdir = project_dir.clone();
    let fs_event_observer_task = tokio::task::spawn_blocking(move || {
        let fs_observer = fsevent::FsEvent::new(vec![pdir.clone()]);
        fs_observer.observe(fs_event_tx);
    });

    let fs_event_transformer_task = tokio::task::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(15));
        // TODO: Create initial temp file in project dir
        // TODO: Start a timer so we can check how long has passed since we created initial temp file.
        // TODO: Initial scan of project dir
        'skip_up_to_temp_file: loop {
            match fs_event_rx.recv() {
                Ok(fs_ev) => {
                    debug!("fs event: {:?}", fs_ev);
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
                Err(_e) => error!("fs event recv error!"),
            };
        }
        loop {
            match fs_event_rx.recv() {
                Ok(fs_ev) => {
                    if false
                    // TODO: If event type is move
                    {
                        // TODO: Create temp file in project dir
                        // TODO: Start a timer so we can check how long has passed since we created temp file.
                        // TODO: Rescan of project dir
                        'skip_up_to_temp_file: loop {
                            match fs_event_rx.recv() {
                                Ok(fs_ev) => {
                                    debug!("fs event: {:?}", fs_ev);
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
                                Err(_e) => error!("fs event recv error!"),
                            };
                        }
                    } else {
                        info!("fs event: {:?}", fs_ev)
                    }
                }
                Err(_e) => error!("fs event recv error!"),
            };
        }
    });

    /*
     * Serving of status pages, showing status and history.
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

    let fs_event_tasks = join(fs_event_observer_task, fs_event_transformer_task);

    info!("Starting status and project servers.");
    info!("Access your project through the http-horse status user interface.");
    info!(
        "http-horse status user interface is accessible at http://{}",
        status_addr
    );

    let servers = join(status_server, project_server);
    let ret = join(fs_event_tasks, servers).await;

    (ret.0).0?;
    (ret.0).1?;
    (ret.1).0?;
    (ret.1).1?;

    Ok(())
}

fn connect_event_stream(response_builder: http::response::Builder) -> http::Result<Response<Body>> {
    let (_sender, body) = Body::channel();

    // TODO: Connect the thing

    response_builder.body(body)
}

async fn request_handler_status(req: Request<Body>) -> http::Result<Response<Body>> {
    let (method, uri_path) = (req.method(), req.uri().path());

    debug!("request_handler_status got request");
    debug!("  Method:   {}", method);
    debug!("  URI path: {}", uri_path);

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

async fn request_handler_project(req: Request<Body>) -> http::Result<Response<Body>> {
    let (method, uri_path) = (req.method(), req.uri().path());

    debug!("request_handler_project got request");
    debug!("  Method:   {}", method);
    debug!("  URI path: {}", uri_path);

    let response_builder = Response::builder().header(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_VALUE_NO_STORE),
    );

    let Some(project_dir) = PROJECT_DIR.get() else {
        return server_error(response_builder);
    };

    match (method, uri_path) {
        (&Method::GET, _) => {
            if uri_path == "/" {
                // 1. Try file "index.htm"
                if let Ok(file) = File::open(Path::new(project_dir).join("index.htm")).await {
                    let stream = FramedRead::new(file, BytesCodec::new());
                    let body = Body::wrap_stream(stream);
                    return Ok(Response::new(body));
                }
                // 2. Try file "index.html"
                if let Ok(file) = File::open(Path::new(project_dir).join("index.html")).await {
                    let stream = FramedRead::new(file, BytesCodec::new());
                    let body = Body::wrap_stream(stream);
                    return Ok(Response::new(body));
                }
                // 3. Return a directory listing. (Note: This one needs to update itself as well.)
                // TODO: dir listing
                not_found(response_builder)
            } else {
                // TODO: Look for the file
                not_found(response_builder)
            }
        }
        _ => method_not_allowed(response_builder),
    }
}

fn server_error(response_builder: http::response::Builder) -> http::Result<Response<Body>> {
    response_builder
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(INTERNAL_SERVER_ERROR_BODY_TEXT.into())
}

fn method_not_allowed(response_builder: http::response::Builder) -> http::Result<Response<Body>> {
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
